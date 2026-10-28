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
  `"bcZ"`, without a panic. Undo change sets are made for one revision's exact text, so a single
  edit applied without being recorded corrupts every undo after it.
- Vim and Helix reach other branches by time: `g-`/`g+` and `Alt-u`/`Alt-U` step through
  revisions in the order they were made, and `:earlier`/`:later` take a count or a duration.
- They differ on redo after a jump into an older branch: Vim follows the branch visited last,
  Helix the newest child.

## Decision

- `buffer_text::Document` owns a text, its selection, and its undo history. Edits
  (`apply(transaction, now)`), selection changes, and moves through history all go through it;
  the history itself isn't public.
- A document has one selection.
- Commit points are explicit: the caller calls `commit()` where 0029 ends a revision. `apply`
  applies the pause rule itself. A revision whose edits cancel out isn't recorded.
- Navigation: `undo` and `redo` along a branch; `earlier` and `later` by a number of revisions in
  the order they were made, across branches; `earlier_by` and `later_by` a duration, to the
  newest revision made by the current revision's time minus or plus it, or to the root.
- Redo follows the child made or visited last, as in Vim: undo then redo retraces the way, also
  after a jump into another branch.

## Consequences

- History can't drift from the text: nothing else changes a document's text.
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
