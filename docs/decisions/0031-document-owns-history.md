# 0031. A document owns its text, selection, and history

- Status: Accepted
- Date: 2026-10-28

## Context

Undo history is a revision tree ([0017](0017-selection-based-core.md)), and
[0029](0029-undo-insert-sessions.md) settles how much one revision covers, with commit points and
timestamps coming from the caller. Still open: what keeps history and text in step, how to reach
the branches undo leaves behind, and where redo goes after an undo.

- A change set fits only the text it was made for, and applying one checks only the length
  ([0030](0030-panics-and-results.md)): a change set made for `"aé"` applied to `"abc"` gave
  `"bcZ"`, without a panic. One that splits a char panics halfway through: made for `"éab"` and
  applied after typing `"b"` into `"aé"`, one inserted `"Z"` and then panicked, leaving `"Zaéb"`
  with neither edit in the history. Undo change sets are made for one revision's exact text, so a
  single edit applied without being recorded corrupts every undo after it.
- Vim and Helix reach other branches by time: `g-`/`g+` and `Alt-u`/`Alt-U` step through
  revisions in the order they were made, and `:earlier`/`:later` take a count or a duration.
- They differ on redo after a jump into an older branch: Vim follows the branch visited last,
  Helix the newest child.

## Decision

- `buffer_text::Document` owns a text, its selection, and its undo history. Edits, selection
  changes, and moves through history all go through it; the history itself isn't public.
- A document builds every change set from its own text. Callers pass `(range, replacement)`
  pairs: `edit(changes, now)` maps the selection through the change, and
  `edit_with(changes, selection, now)` takes the selection after it from a closure given the
  change set. Both check the ranges against the current text before anything changes and return
  0030's `InvalidChange` for a stale one. Transactions, and building, applying, inverting, and
  composing change sets, are internal to `buffer-text`.
- A document has one selection.
- Commit points are explicit: the caller calls `commit()` where 0029 ends a revision. An edit
  applies the pause rule itself. A revision whose edits cancel out isn't recorded.
- Navigation: `undo` and `redo` along a branch; `earlier` and `later` by a number of revisions in
  the order they were made, across branches; `earlier_by` and `later_by` a duration, to the
  newest revision made by the current revision's time minus or plus it, or to the root.
- Redo follows the child made or visited last, as in Vim: undo then redo retraces the way, also
  after a jump into another branch.

## Consequences

- History can't drift from the text: nothing else changes a document's text, and no change set
  made for another text reaches it. A stale range is an `Err` with nothing changed; one that is
  still valid but now in the wrong place edits the wrong bytes, which no cheap check can see.
- Every move through the tree is undos up to the newest common ancestor, then redos down along
  redo pointers set on the way, so a jump needs no path buffer.
- Undo restores the selection from before a revision; redo, the one after it.
- Clean/dirty tracking for saves needs a revision id from `Document`; it lands with save.
- Split views of one note would need a selection per view, which changes `Document`.

## Alternatives considered

- A standalone history, as in Helix: the caller applies the transactions it hands out and records
  every edit itself, and a missed edit silently corrupts undo.
- Redo follows the newest child, as in Helix: after a jump into an older branch, redo leaves it,
  and a jump needs the path down in a buffer.
- Time jumps to the closest revision on either side, as in Helix: `:earlier 10m` could land less
  than ten minutes back.
- Documents taking transactions the caller builds: a transaction made for another text of the
  same length passes the length check, with the results measured above.
- One `edit` taking a selection closure: a single method, but the common case, mapping the
  selection, needs a clone and a closure at every call site.
