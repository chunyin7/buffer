# 0043. Selection operations are named methods in buffer-text

- Status: Accepted
- Date: 2026-11-13

## Context

Both presets ([0018](0018-vim-helix-presets.md)) reshape selections without editing. Helix
collapses, flips, and trims ranges, extends them to whole lines (`x`), splits them into lines
(`Alt-s`), merges them, copies them onto the next line (`C`), and keeps, drops, or rotates the
primary range. Vim has linewise Visual mode (`V`), where whole lines are selected but the cursor
keeps its column, and blockwise Visual mode (`Ctrl-V`), a rectangle between two corners.

A `Selection` ([0027](0027-helix-selection-ranges.md)) offers `new`, `ranges`, `map`, and
`for_normal_mode`, and a `Range` offers `cursor` and `extend_to`
([0032](0032-motions-return-targets.md)). A preset can rebuild any selection from those, but
then each preset holds its own copy of the rules, tested only through key sequences.

- Most of these operations read the text (lines, graphemes, blanks) or change which range is
  primary, which a selection keeps consistent itself.
- Helix's `s`, `S`, `K`, and `Alt-K` select, split, and filter by a regex. `buffer-text` has no
  regex dependency, and in-note search, which needs one too, lands with the edit view.
- A block is a rectangle on screen, and columns count graphemes for now: `j` and `k` keep a goal
  column in graphemes until the edit view defines display cells (0032). A wide char, a tab, or a
  soft-wrapped line makes the two differ.
- A block is two corners, not a set of ranges: every move of the cursor corner changes the range
  on every line. In Vim, a line that ends before the block has no part in it, `$` makes the block
  ragged to each line's end, `A` pads short lines with spaces, and a blockwise register pastes
  back as a block.

## Decision

- Each operation is a method of `Selection` in `buffer-text` that takes the selection, and the
  text if it reads it, and returns the new selection. The presets bind commands to them and, as
  after every Normal-mode command, apply `for_normal_mode` (0032).
- Each range, alone:
  - `collapse`: an empty range at the range's block cursor.
  - `flip`: anchor and head swap.
  - `extend_to_lines`: the range grows to the start of its first line and to the start of the
    line after its last, its line break included, or to the end of the text. It keeps its
    direction. A range that covers whole lines already stays as it is; taking one more line on a
    repeat, as Helix's `x` does, is the preset's.
  - `trim`: the range loses the blank graphemes at both ends, line breaks too. A range of blanks
    only is dropped, unless every range is.
- The selection as a whole:
  - `all`: one range over the whole text.
  - `keep_primary`, `drop_primary`, and `rotate_primary` by a count, forward or back, wrapping.
  - `merge`: one range from the first range's start to the last one's end, pointing as the
    primary does.
  - `split_lines`: each range becomes one range per line it covers, its part of that line
    without the line break. Empty parts are dropped; a range with no other part stays whole.
  - `copy_to_line`, below or above: each range gets a copy as many lines away as the range is
    tall, at the same columns, counted in graphemes as for `j` and `k`. The copy's start lands on
    a grapheme of its line: a line too short for that, an empty one too, is passed over for the
    next one. An empty range may land at a line's end. A range that ends with a line break gets
    a copy that ends with one, so whole lines copy as whole lines, and a line break alone, the
    cursor at a line's end, copies as the one grapheme at its column: a char, or the break of a
    line that ends there. The primary's copy becomes primary. A count is the preset's.
  - `block`, from two corners: one range per line between the corners' lines, covering the
    columns between the corners' columns, both corners' graphemes included. A line that ends
    before the block's first column gets no range, and one that ends inside it a shorter one. A
    flag takes every range to its line's end instead, as Vim's `$` does. The range on the head
    corner's line is primary, and the ranges point toward the head corner's column. If the head
    corner's line ends before the block, it gets an empty range at its end, so the cursor has a
    place; the preset leaves that range out when it acts.
- The primary range after a change of shape: when a range splits, the part that holds its block
  cursor is primary. When the primary is dropped, the next range takes over, or the previous one
  if it was last. Dropping the only range changes nothing, since a selection is never empty.
- Vim's linewise Visual mode is the Vim preset's. The document holds the charwise range, so the
  cursor keeps its column; the preset knows the mode, applies `extend_to_lines` when it acts, and
  tells the edit view to draw whole lines.
- Vim's blockwise Visual mode is the Vim preset's too. It keeps the two corners and the goal
  column, and sets the document's selection from `block` after each move. `I` and `A` are inserts
  at every range. The blockwise register type, pasting a block back, and what `A` does on short
  lines belong to the preset.
- Regex operations wait for in-note search.

## Consequences

- One tested method per operation serves both presets, a mouse action, or the palette.
- `extend_to_lines` serves Helix's `x`, Vim's `V`, and Vim's linewise operators.
- A block is an ordinary selection of several ranges: operators, yank, paste, undo, and redo need
  nothing new. The corners live only in the preset, so `gv` keeps them there.
- Columns are graphemes, so a block isn't a rectangle across wide chars or tabs, and a copy onto
  the next line can sit a cell off. Both follow `j` and `k` when columns become display cells.
- No padding: a block skips lines that end before it, where Vim's `A` pads them with spaces. The
  Vim preset can pad with an edit of its own if we want that.
- `Selection` grows by about a dozen methods; docs/editor/model.md lists them.

## Alternatives considered

- Generic builders (transform each range, replace each range with several) with the operations
  written in each preset: a smaller core, but two copies of the rules, and the primary-range
  rules would be tested only through keys.
- Whole lines in the document's selection for `V`, with the cursor's column kept by the preset:
  simpler to draw, but the range no longer says where the cursor is, and every motion in `V`
  would have to undo the widening first.
- Blockwise Visual mode after the edit view, once columns are display cells: it would leave Vim's
  `Ctrl-V` out of M1 for a rule that changes in one place either way.
- An empty range on each short line of a block: Normal mode widens empty ranges to a grapheme
  (0032), so the block would take the line break or a char outside it.
