# 0040. Registers live in buffer-modal; yank and paste in buffer-text

- Status: Accepted
- Date: 2026-11-09

## Context

[0017](0017-selection-based-core.md) lists registers among the M1 work in `buffer-text`, while
docs/editor/keymaps.md puts them in `buffer-modal`'s dispatch. The two presets
([0018](0018-vim-helix-presets.md)) disagree on what a register is:

- Vim: the unnamed `"`, `0` for the last yank, `1`–`9` shifting on each delete, `-` for small
  deletes, `a`–`z` with uppercase appending, the black hole `_`, and a charwise, linewise, or
  blockwise type stored with each value.
- Helix: one value per selection range, spread over the ranges on paste; no numbered registers;
  linewise when a value ends in a line break.

Both have registers computed from editor state: `.` (last insert), `:` (last command), `/` (last
search), `%` (file name), Helix's `#` (selection indices). Both have `+` and `*` for the system
clipboard and PRIMARY.

A `Document` holds one buffer's text, selection, and history
([0031](0031-document-owns-history.md)); registers are shared by every buffer. Vim's motion kinds
already live in the Vim preset, not in `buffer-text` ([0032](0032-motions-return-targets.md)).
Core crates use no platform APIs (invariant 10).

## Decision

- `buffer-modal` owns the register store, register names, and each preset's rules: Vim's numbered
  shift, appending, black hole, and register types; Helix's per-range values; and the computed
  registers.
- A register value is a list of strings, one per range it was yanked from.
- `buffer-text` owns yank and paste, which know nothing of registers: yank returns the text of
  each range of a selection; paste takes one string per range and whether to put it before,
  after, or in place of the range, and edits through the `Document`, which sets the resulting
  selection. Mapping a register's values onto the ranges (Helix repeats the last value, Vim joins
  them) is the preset's.
- `+` and `*` go through a clipboard trait defined in `buffer-modal` and implemented in
  `buffer-ui` or `buffer-app`, so the core stays free of platform APIs.

## Consequences

- One paste primitive serves both presets, and its selections, grapheme edges, and Normal-mode
  widths are tested in `buffer-text` without keys, as motions and text objects are.
- Register semantics are tested in the preset key-sequence tables.
- Registers outlive a buffer and are shared across buffers; whether they persist across sessions
  is open.
- This moves registers out of the `buffer-text` M1 list in 0017.

## Alternatives considered

- Registers in `buffer-text`: a session-wide store in a crate whose state is per document, with
  each preset's rules and the computed registers pulled down into the core.
- Registers and paste both in `buffer-modal`: paste's selection logic would be tested only through
  key sequences, and duplicated if another caller (mouse, palette) needs it.
