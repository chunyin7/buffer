# 0027. Selections are Helix-style ranges

- Status: Accepted
- Date: 2026-10-28

## Context

The editing core is selection-based ([0017](0017-selection-based-core.md)) and serves both the Vim
and the Helix preset ([0018](0018-vim-helix-presets.md)), but 0017 leaves open how a selection is
represented. Positions are UTF-8 byte offsets, and every edit is a change set that selections map
through. In Normal mode both presets draw a block cursor on a character; in Insert mode, a caret
between characters. Helix selects, then acts on the selection; Vim combines an operator with a
motion.

## Decision

- A range is two byte offsets, `anchor` and `head`, and covers the half-open span between them.
  Extending a selection moves the head; the anchor stays put.
- A selection is one or more ranges, one of them primary. Its ranges are sorted and never
  overlap: ranges that overlap, or start at the same offset, merge.
- The block cursor is the grapheme at the head, or the one before the head when the range points
  forward (`head > anchor`).
- Normal mode keeps every range at least one grapheme wide; Insert mode allows empty ranges.
- Vim's Normal-mode cursor is a one-grapheme range. An operator + motion selects from the cursor
  to the motion's target, then acts on that range.
- Mapping through a change set: a range's start stays after text inserted at it and its end stays
  before such text, so inserts at its edges don't grow it; an empty range moves past text
  inserted at it.

## Consequences

- One representation serves both presets; the Vim preset turns motions into ranges instead of
  keeping a cursor of its own.
- Ranges are plain pairs of offsets, and a selection's ranges are sorted, so mapping a selection
  through a change set is one pass over both.
- The grapheme rules (the block cursor, the one-grapheme minimum in Normal mode) need grapheme
  boundary search, which lands with the motions.
- docs/editor/model.md describes selections.

## Alternatives considered

- Kakoune's inclusive ranges, where both ends sit on characters and a range is never empty: they
  fit a block cursor, but Insert-mode carets and half-open change mapping need ±1 conversions
  everywhere.
- Carets as in VS Code and CodeMirror, where ranges may be empty and the block cursor is only
  drawn: the simplest core, but Helix's select-then-act needs "an empty range means the grapheme
  under the cursor" special cases in every command.
