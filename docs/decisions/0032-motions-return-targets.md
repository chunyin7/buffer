# 0032. Motions return targets; words follow Unicode

- Status: Accepted
- Date: 2026-11-01

## Context

M1 needs grapheme-aware motions for both presets ([0018](0018-vim-helix-presets.md)), on
Helix-style ranges ([0027](0027-helix-selection-ranges.md)) and the line model
([0028](0028-lines-are-breaks-plus-one.md)). `Text` finds grapheme boundaries. The presets use a
motion in different ways:

- Vim's Normal mode moves a one-grapheme cursor to the motion's target. An operator acts from the
  cursor to the target, and the motion decides how: exclusive (`dw` stops before the target),
  inclusive (`de`, `df`, `d$`), or linewise (`dj`, `dG`), with special cases such as `cw` acting
  as `ce`. Visual mode keeps the anchor and moves the head.
- Helix's `h`/`j`/`k`/`l` move the cursor. Its word motions select the span they cross, starting
  past the cursor when a word begins right after it: from `f` in `foo bar baz`, `w` selects
  `foo `, and `w` again selects `bar `. Select mode keeps the anchor and moves the head.
- `h`/`l` stop at line edges in Vim and cross them in Helix.

Vim and Helix find words by classifying chars (letters, digits, and `_`; punctuation; whitespace),
which splits `don't`, `3.14`, and `foo.bar` in prose. Unicode's word boundaries (UAX #29) keep
those whole but give every punctuation char its own segment: with unicode-segmentation 1.13,
`**bold**` is five segments (`*`, `*`, `bold`, `*`, `*`), `a...b` is five, and `[[Note|alias]]`
seven. Ideographs and hiragana are a segment per char (`日本語の` is four), katakana runs stay
together, and no word crosses a line break. unicode-segmentation finds word boundaries in a `&str`
only; unlike graphemes, it has no cursor that walks chunks.

An edit can leave a range's end inside a grapheme: another cursor types a combining accent right
after a selected `e`. Repeated `j`/`k` return to the column they started from, the goal column.
Moving across soft-wrapped lines, which notes want, needs the layout, and only the edit view in
`buffer-ui` has it.

## Decision

- Motions are pure functions in `buffer-text`. From an offset and a count, a motion returns its
  target offset. Word motions also return where the span they cross starts: at the cursor, or just
  past it when that is already one of the motion's stops.
- Three range builders turn a target into a range, with the block-cursor rules of 0027:
  - move: a one-grapheme range at the target (Vim's Normal mode, Helix's `h`/`j`/`k`/`l`);
  - select: the span the motion crossed (Helix's word motions);
  - extend: the anchor stays and the head moves to the target (Vim's Visual mode, Helix's Select
    mode).
- Vim's motion kinds (exclusive, inclusive, linewise) and its special cases belong to the Vim
  preset in `buffer-modal`.
- A count repeats a motion. When the text runs out, the motion stops at the last target it
  reached, and the caller can tell when it didn't move at all.
- Grapheme motions take a flag: stop at line edges (Vim) or cross them (Helix).
- In Normal mode, every range covers at least one grapheme, except an empty range at the end of
  the text where that end starts an empty line: an empty text, or after a final line break.
  - After an edit, a range end inside a grapheme moves outward to the grapheme's edge, and an
    empty range moves to the start of its grapheme.
  - An empty range widens to the grapheme after it, or at the end of the text to the one before
    it, except where the end starts an empty line.
  - `buffer-text` provides the function that restores this; `buffer-modal` applies it after every
    Normal-mode command and edit. `Document` stays mode-agnostic.
  - Rules on top stay in the presets: Vim's cursor keeps off line breaks, except on an empty line,
    and off the empty line after a final break (0028).
- Words follow UAX #29 word boundaries (unicode-segmentation), with adjacent punctuation merged into
  one segment, so `**`, `[[`, `...`, and `->` are one stop each. A segment with a letter or digit
  is a word; whitespace and line breaks are their own kinds; any other segment is punctuation.
  WORD motions (`W`, `B`, `E`) merge every segment between whitespace and line breaks.
- Word motions get their segments through one interface. UAX #29 with merged punctuation is its
  first implementation; Vim's or Helix's classes, or a dictionary for CJK, would be others, without
  changes to the motions.
- `j`/`k` in `buffer-text` move across lines split by line breaks. The goal column counts graphemes
  from the line start. The caller keeps one per range while `j`/`k` repeat, passes it in, and gets
  back the one to keep; `Range` stays a pair of offsets.
- Moving across soft-wrapped lines lands with the edit view in `buffer-ui`; the presets' keymaps
  then pick which lines `j`/`k` move across.

## Consequences

- One target function per motion serves Vim's moves and operators, Helix's selections, and both
  extend modes. Presets differ in keymaps and builders, not in motion code.
- Prose words stay whole (`don't`, `3.14`, `example.com`), and Markdown syntax is one stop per run
  of punctuation. Unlike in Vim, ideographs and hiragana are a word per char; treating a run as one
  word needs another segmenter.
- No word crosses a line break, so word motions segment within one line. unicode-segmentation needs
  a contiguous `&str`, so a motion copies the part of the line it scans. A note written as one long
  line costs O(line) per word motion; a bounded window fixes that if measurements show it.
- Grapheme columns let `j`/`k` drift by a cell on lines with wide chars or tabs. Revisit with the
  edit view: display cells need `unicode-width` as a direct dependency (it is in the build through
  naga) and a tab-width setting.
- Normal-mode ranges stay valid only where `buffer-modal` restores them; the key-sequence tables
  ([0018](0018-vim-helix-presets.md)) check the selection each sequence leaves.
- docs/editor/model.md describes motions once they land.

## Alternatives considered

- Motions that take and return a `Range`, as in Helix: select-on-move would be part of every
  motion, and Vim's moves and operators would have to undo it.
- Separate code for select-on-move and the extend modes: they share the target and differ only in
  the anchor.
- Vim's or Helix's char classes: familiar, but they split `don't`, `3.14`, and `foo.bar`. They stay
  possible segmenters.
- UAX #29 without merging punctuation: `**bold**` takes five `w` presses.
- Re-aligning inward, or to the nearest boundary: either can empty a one-grapheme range, which
  Normal mode then widens again, possibly onto another grapheme.
- A goal column in `Range`, as in Helix: it survives mapping, but equality, merging, and every
  builder have to handle it, and the soft-wrapped motion keeps an x position in the view anyway.
- Display-cell columns now: a new direct dependency and a tab-width setting before the edit view
  can show the difference.
- Failing a motion whose count overshoots: a long count near the end of a note would do nothing.
