# 0030. Panic on broken contracts, Result for stale input

- Status: Accepted
- Date: 2026-10-28

## Context

`buffer-text` checks what its callers pass in: change ranges must be sorted, inside the text, and
on char boundaries, and a change set must meet the text it was made for. 0023 settles that library
errors use `thiserror` and that workers run under `catch_unwind`, but not when a failed check
panics, returns an error, or runs only in debug builds.

- A panic on the UI thread ends the app, and every unsaved edit with it, until crash recovery
  lands (M4).
- Some input can be wrong through no bug in its caller: link rewrites on rename and tag edits
  patch spans computed from a snapshot of the text, which the user may have edited since.
- A check compiled out of release builds hides the bug instead. With `ChangeSet::new`'s checks as
  `debug_assert!`, a release build applied changes given out of order to the wrong bytes
  (`"0123456789"` with 5..7 → `"a"`, then 3..4 → `"b"`, became `"01b34a789"`, not
  `"012b4a789"`), and applied a change set made for a 6-byte text to an 8-byte one, both without a
  panic. Release builds don't check integer overflow, and the wrapped arithmetic even left the
  lengths consistent.
- The checks are cheap: constant work per change, plus two rope lookups for char boundaries.

## Decision

- A broken contract that only a bug in our own code can cause panics before anything is mutated,
  with a message saying what broke: `assert!`, or `expect` on a documented invariant, and a
  `# Panics` section in the docs.
- Input that can be stale through no bug in its caller (spans computed from a snapshot, or coming
  from another thread or the platform) is validated before anything changes and rejected with a
  `Result`, its error type defined with `thiserror`.
- A check that guards text bytes is never `debug_assert!` only. `debug_assert!` is for internal
  invariants whose failure can't change bytes, and for checks too costly for release builds.
- In `buffer-text`, `ChangeSet::new` returns `Result<ChangeSet, InvalidChange>`, and applying,
  inverting, or composing a change set with a text or change set of another length panics.

## Consequences

- A stale span costs one rejected edit, which the caller recomputes. A bug in our own code still
  stops the app where it happens, before any text is changed.
- Callers of `ChangeSet::new` handle the error; commands will pass it up to the dispatcher with
  `?`.
- Panics that remain (our bugs, crop, std) still end the session. Containing them around command
  dispatch, extending 0023's `catch_unwind` to the UI thread, and crash recovery (M4) are separate
  decisions.
- docs/editor/model.md lists which `buffer-text` checks panic and which return errors.

## Alternatives considered

- `Result` everywhere: error handling for checks only our own bugs can fail, with the pull to
  `.expect()` at call sites; it still doesn't cover panics in crop or std.
- Panics everywhere: a stale span from a rename or a tag edit would end the session and take
  unsaved edits with it.
- `debug_assert!` only: corrupts text silently in release builds, as measured above.
