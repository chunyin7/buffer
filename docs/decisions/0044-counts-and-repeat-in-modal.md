# 0044. Counts and repeat live in buffer-modal

- Status: Accepted
- Date: 2026-11-13

## Context

[0017](0017-selection-based-core.md) lists counts and repeat among the M1 work in `buffer-text`,
while docs/editor/keymaps.md puts counts in `buffer-modal`'s dispatch. Since then the core has
taken shape:

- Motions and text objects are pure functions that take a count
  ([0032](0032-motions-return-targets.md), [0038](0038-text-objects.md)). Nothing in
  `buffer-text` reads keys or holds a pending count.
- What a count means is each preset's. Vim multiplies the two counts of `2d3w`, gives `3J`,
  `3p`, and `3i` meanings of their own, and a count can be a line number (`5G`). Helix counts
  motions and a few commands.
- A `Document` holds one buffer's text, selection, and history
  ([0031](0031-document-owns-history.md)). Its revisions are change sets at byte offsets; a
  repeat runs a command again on whatever is selected now, in any buffer, so it can't replay one.
- What a repeat repeats is each preset's too. Vim's `.` repeats the last change with its count
  and register, Insert session included; Helix's `.` repeats the last insert, and `Alt-.` the
  last find. Registers, which a repeat reads, already live in `buffer-modal`
  ([0040](0040-registers-in-modal.md)).

## Decision

- `buffer-modal` owns counts: dispatch reads the count before a key sequence, and the preset's
  commands decide what it means.
- `buffer-text` takes a count as an argument wherever one repeats a step (motions, text objects,
  `rotate_primary`) and keeps no count of its own.
- `buffer-modal` owns repeat: what it records and how it replays are decided with dispatch and
  the presets.
- `buffer-text` gains nothing for repeat. A replayed command edits through the `Document` as
  the first one did.

## Consequences

- With registers ([0040](0040-registers-in-modal.md)), this takes the last items that aren't
  about text out of the `buffer-text` M1 list in 0017: what is left there is text, motions, text
  objects, selections, edits, and history.
- Counts and repeat are tested in the preset key-sequence tables, counts that overshoot in
  `buffer-text`.
- Open for the dispatch work: whether repeat records keys or commands with their arguments, and
  how it records an Insert session.

## Alternatives considered

- A count on `Document` or `Selection`: per-buffer state for what belongs to a key sequence, with
  each preset's rules pulled down into the core.
- Repeat as replaying the last revision's change set: it holds byte offsets of the place it was
  made, not what to do at another selection.
