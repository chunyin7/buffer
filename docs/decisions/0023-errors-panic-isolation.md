# 0023. thiserror, and panic isolation for workers

- Status: Accepted
- Date: 2026-10-26

## Context

Invariant 1: never lose or rewrite user bytes. Invariant 9: notes are untrusted input; parsers
never panic on any input (fuzzed), and typst runs sandboxed and bounded. Math, highlighting, and
image decoding run large third-party code on note content in worker threads
([0012](0012-typst-math.md), [0013](0013-syntect-highlighting.md), [0014](0014-images.md)).

## Decision

- Library crates define errors with `thiserror`.
- No `unwrap`/`expect` outside tests except for documented invariants.
- Math, highlight, and image workers run under `catch_unwind`.
- Release builds keep `panic = "unwind"`, so a renderer bug can't take down unsaved text.

## Consequences

- A panic in a render worker costs one rendered element, not the session.
- `panic = "abort"` is not available as a release-profile size lever.
- Panic isolation complements, not replaces, fuzzing parsers so they never panic.
- Crash recovery for unsaved buffers (M4) covers failures outside the workers.
