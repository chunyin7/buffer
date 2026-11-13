# Editing model

How `buffer-text` holds a note's text and changes it. The decisions behind it:
[0016](../decisions/0016-crop-rope.md) (crop rope),
[0017](../decisions/0017-selection-based-core.md) (selection-based core),
[0027](../decisions/0027-helix-selection-ranges.md) (Helix-style ranges),
[0028](../decisions/0028-lines-are-breaks-plus-one.md) (lines),
[0029](../decisions/0029-undo-insert-sessions.md) (undo steps),
[0031](../decisions/0031-document-owns-history.md) (documents and history),
[0032](../decisions/0032-motions-return-targets.md) (motions and Normal-mode ranges),
[0034](../decisions/0034-word-motions-stop-at-line-edges.md) (word motions at line edges),
[0035](../decisions/0035-find-paragraph-and-bracket-motions.md) (find, paragraph, and bracket
motions), [0038](../decisions/0038-text-objects.md) (text objects),
[0040](../decisions/0040-registers-in-modal.md) (yank and paste), and
[0043](../decisions/0043-selection-operations.md) (selection operations).

Status: text with lines and grapheme boundaries, grapheme, line, word, find, paragraph, and
bracket motions, text objects, change sets, selections with block cursors and operations on them,
transactions, and documents with undo history exist, and yank and paste. Registers live in
`buffer-modal`.

## Text

- `Text` holds UTF-8 in a `crop` rope. No other crate depends on `crop`, so a later switch stays
  inside `buffer-text`.
- Clones are cheap and copy-on-write: a background worker takes a snapshot by cloning.
- A `Text` changes only through `ChangeSet::apply`, which is crate-private: outside
  `buffer-text`, only a `Document` changes a text, so every edit lands in its history.

## Positions

- Positions are UTF-8 byte offsets from 0 to the text's length, inclusive. Conversion to chars,
  graphemes, or UTF-16 happens only at layout, IME, and platform boundaries.
- An edit starts and ends on char boundaries; `Document::edit` rejects one that doesn't.

## Lines

A text has one more line than it has line breaks
([0028](../decisions/0028-lines-are-breaks-plus-one.md)). `\n` and `\r\n` are line breaks; a lone
`\r` is not.

- Lines count from 0. Every offset from 0 to the length is on exactly one line: `"foo\n"` has two
  lines, the second one empty, and an empty text has one. A line break, and an offset inside a
  `\r\n`, belong to the line the break ends.
- `line_count()` counts the lines; `line_of(offset)` is the line an offset is on.
- `line_start(line)` is where a line starts, and `line_end(line)` where its content ends, before
  its line break. `line_start(line_count())` is the end of the text, so lines `a..b`, with their
  breaks, cover `line_start(a)..line_start(b)` without an end-of-text special case.
- crop's `line_len` leaves out the empty line after a final break, so `Text` counts the breaks
  before the end of the text instead. Every method is a few rope lookups, O(log n).

## Graphemes

A cursor moves by graphemes: what a reader sees as one character, Unicode's extended grapheme
clusters (UAX #29). An `e` with a combining accent, a flag, an emoji sequence, and a `\r\n` are one
grapheme each, so a cursor never stops inside them.

- `is_grapheme_boundary(offset)` says whether a cursor may stop at an offset; an offset inside a
  char is no boundary.
- `next_grapheme_boundary(offset)` is the first boundary after an offset, and
  `prev_grapheme_boundary(offset)` the last one before it. They stay put at the end and the start
  of the text, and from inside a grapheme they go to its end and its start.
- unicode-segmentation's `GraphemeCursor` finds them; egui links that crate already. `Text`
  slices the rope at the offset and hands the cursor the chunks from there on, forward or
  backward, plus the chunks before them when a rule looks back across a chunk's start, as
  regional indicators do to pair into flags.
- Going forward, a cursor never gets a second chunk. In unicode-segmentation 1.13.3, a cursor
  that does can count a regional indicator twice and split a flag that follows a prepended char,
  such as U+0600 ARABIC NUMBER SIGN. So at a chunk's end a fresh cursor decides whether the end is
  a boundary, and if it isn't, a new cursor goes on in the next chunk. Going backward, a cursor
  decides a chunk's start while it still holds the chunk before it, so the bug doesn't reach it.
- Each call costs a few rope lookups, O(log n), plus the chars the rules read around the offset:
  the grapheme it crosses and, inside a run of regional indicators, the run before it.
- A scan that steps on from a boundary it found reads nothing before it. The text from a grapheme
  boundary on splits into the same graphemes alone as in the whole text: a run of regional
  indicators holds an even number of them before a boundary, and the rules that join emoji and
  Indic conjuncts never reach back past one (UAX #29, GB9, GB9c, GB11). So `next_grapheme` and
  vertical moves read a run of flags once, where a lookup at each flag would read the run back to
  its start each time and cost O(run²).
- Going backward, `prev_grapheme` takes every step with one cursor. Inside a run of regional
  indicators, it reads the run back to its start on the first step and counts down from there, so
  a count costs O(count) plus the run before `offset`, once.
- crop's `graphemes` feature stays off: its `is_grapheme_boundary` walks chunks back from the end
  of the text, so its cost grows with the distance to the end.

## Motions

Motions are `Text` methods ([0032](../decisions/0032-motions-return-targets.md)). From an offset
and a count, each returns a target offset on a grapheme boundary, and the range builders in
[Block cursors and Normal mode](#block-cursors-and-normal-mode) turn it into a range. A count that
overshoots stops at the last target, except in a find, which fails. A motion that can't move
returns its offset, so the caller can tell; a find and a bracket match return `None` instead.
Targets are boundaries: `line_end` is where the line break starts, and the presets put the block
cursor for `$` on the grapheme before it. Vim's motion kinds live in the Vim preset.

| Method | Target | Vim | Helix |
|---|---|---|---|
| `next_grapheme(offset, count, edges)` | `count` graphemes forward | `l` | `l` |
| `prev_grapheme(offset, count, edges)` | `count` graphemes back | `h` | `h` |
| `line_start(line)` | the start of the line | `0` | `gh` |
| `first_non_blank(line)` | the line's first grapheme that isn't blank | `^` | `gs` |
| `line_end(line)` | the end of the line's content | `$` | `gl` |
| `line_down(offset, count, goal)`, `line_up(…)` | the grapheme at the goal column, `count` lines down or up | `j`, `k` | `j`, `k` |
| `next_word_start(offset, count, words)` | `count` word starts forward | `w` | `w` |
| `next_word_end(…)` | `count` word ends forward | `e` | `e` |
| `prev_word_start(…)` | `count` word starts back | `b` | `b` |
| `prev_word_end(…)` | `count` word ends back | `ge` | |
| `find_next(offset, count, edges, matches)` | the `count`-th grapheme forward whose first char matches | `f`, `t` | `f`, `t` |
| `find_prev(…)` | the `count`-th one back | `F`, `T` | `F`, `T` |
| `next_blank_line(offset, count)` | `count` paragraphs forward, to the blank line after each | `}` | |
| `prev_blank_line(…)` | `count` paragraphs back, to the blank line before each | `{` | |
| `next_paragraph_start(offset, count)` | the start of the `count`-th paragraph after `offset` | | `]p` |
| `prev_paragraph_start(…)` | the start of the `count`-th paragraph before `offset` | | `[p` |
| `matching_bracket(offset)` | the partner of the bracket at `offset` | `%` | `mm` |

- `LineEdges::Stop` keeps a grapheme motion on its line, between the line's start and the end of
  its content, as Vim's `h` and `l` do. With `LineEdges::Cross` it stops only at the ends of the
  text, and a line break is one grapheme, as in Helix.
- `first_non_blank` skips graphemes of Unicode whitespace: spaces, tabs, no-break spaces, and the
  ideographic space U+3000 that indents Japanese paragraphs. A grapheme with any other char, such
  as a space with a combining accent, isn't blank. A line that is all blank gives the end of its
  content.
- The start and the end of the text are 0 and `len()`. Line `n` starts at `line_start(n)`, and
  Vim's `G` goes to `first_non_blank(n)`.
- `line_down` and `line_up` keep to a goal column, counted in graphemes from the line's start
  (revisit: display cells, with the edit view). The caller passes the goal it kept, or `None` to
  start from `offset`'s column, and keeps the goal that comes back. A line shorter than the goal
  gives the end of its content, and the goal `usize::MAX` keeps to line ends, as Vim does after
  `$`. A count past the first or the last line stops there.
- Columns read the rope's chunks in place, without copying the line, and only up to the column
  they need. A run of ASCII is one grapheme per byte, so it takes a byte scan instead of the
  segmenter: about 1 ms per MB in a release build. Elsewhere unicode-segmentation splits each
  chunk, at about 20 ns per grapheme. A vertical move costs O(column): from the line's start to
  `offset` without a goal, and to the goal on the line it moves to. `next_grapheme` walks the
  same way after its first step, so a count costs O(count).
- Moving across soft-wrapped lines lands with the edit view.

### Word motions

Word motions take a segmenter, an `impl Words` or a `dyn Words`. It splits a piece of a line's
content into segments, each a `WordKind::Word`, `Punctuation`, or `Whitespace`, and a motion
stops where a segment that isn't blank starts or ends
([0032](../decisions/0032-motions-return-targets.md)). Another segmenter, such as Vim's char
classes or a dictionary for CJK, changes the stops without changing the motions. The segmenter
also reports restart points, where a line can be split and each piece segmented alone with the
same result ([0036](../decisions/0036-word-motions-segment-a-window.md)).

- `UnicodeWords` follows Unicode's word boundaries (UAX #29), with adjacent punctuation merged, so
  `**`, `[[`, `...`, and `->` are one stop each; adjacent whitespace merges too. A segment with a
  letter or digit is a word: `don't`, `3.14`, `example.com`, `foo_bar`. Ideographs and hiragana
  are a word per char, and a run of katakana is one word. UAX #29 can break inside a grapheme,
  after a prepended char such as U+0600; the segment there stays whole, so a motion never stops
  inside a grapheme. The grapheme boundaries come from one pass alongside the word boundaries: a
  lookup at each break would read a run of flags, a word segment per flag, back to its start
  each time.
- `BigWords` splits only at blank graphemes: WORDs, for `W`, `B`, and `E`.
- Both restart where a word starts after a blank: a whitespace char that is a grapheme of its own,
  then a letter or digit after a word boundary. `restarts_between` gets three chars, because a
  prepended char joins a blank after it into its grapheme, and a combining mark after a blank
  makes it punctuation.
- A word span never crosses a line break, as in Helix
  ([0034](../decisions/0034-word-motions-stop-at-line-edges.md)). A step stops at the next stop of
  its kind on the line, or at the line's edge: its end going forward, its start going backward. A
  step that starts at a line's edge first skips the line break and any empty lines, so empty lines
  are never stops.
- A motion returns `(from, target)`. The target is the count-th stop strictly after, or before,
  `offset`. `from` is where the last step's span starts if the step started at a stop or at a
  line's edge. `None` means it started inside a segment, so the span starts at the cursor.

| Key | Vim passes | Helix passes | Helix selects from |
|---|---|---|---|
| `w` | the cursor's start | the cursor's end | `from`, or the cursor's start |
| `e` | the cursor's end | the cursor's end | `from`, or the cursor's start |
| `b` | the cursor's start | the cursor's start | `from`, or the cursor's end |
| `ge` | the cursor's end | | |

- On the space in `foo bar`, Vim's `w` passes 3 and lands on `b`. Helix's passes 4, a stop, and
  selects `bar`. The Vim preset adapts where Vim's motions cross lines: it steps again at line
  edges, and it doesn't stop on empty lines.
- A step segments a window around its start, copied out of the rope: from the last restart point
  before the start to the first one at least 256 bytes after it going forward, mirrored going
  backward. A window without a stop past the start grows toward the far edge by another piece of
  at least 256 bytes until it reaches the line's edge; pieces meet at restart points, so nothing
  is segmented twice. Steps on the same line reuse the window. A step costs about 0.5 µs plus
  14 ns per byte it segments in a release build: 5 to 8 µs anywhere in a 1 MB line of prose. A
  line without restart points, such as minified code, CJK prose without spaces, or a run of
  flags, is segmented whole: about 20 ms for 1 MB of minified code, 12 ms for 1 MB of flags.

### Find

A find looks for a grapheme by its first char
([0035](../decisions/0035-find-paragraph-and-bracket-motions.md)). It takes a char predicate: the
presets pass `|c| c == 'x'` for `fx`, and Vim's `%` passes a bracket test to find the first
bracket on the line.

- A grapheme matches when its first char does, so `fe` finds a decomposed `é` and `f👍` finds
  `👍🏽`. A char inside a grapheme starts no match: the accent of a decomposed `é`, or a `(` after a
  prepended char such as U+0600. A `\r\n` is one grapheme, which starts with `\r`, so Helix's
  `f<Enter>` is a line motion in its preset.
- `LineEdges::Stop` searches the rest of the line's content, as Vim does, and `LineEdges::Cross`
  the rest of the text, as Helix does. The grapheme at `offset` doesn't count.
- A find returns the start of the match, or `None` when a count goes past the last match, as both
  editors fail. A count of 0 returns `offset`.

| Key | Vim moves to | Helix passes | Helix selects from the cursor to |
|---|---|---|---|
| `f` | the match | the cursor's start | the match's end |
| `t` | the grapheme before the match | the cursor's end | the match's start |
| `F` | the match | the cursor's start | the match's start |
| `T` | the grapheme after the match | the boundary before the cursor | the match's end |

- Vim passes the cursor's start. Helix's `t` and `T` pass the grapheme boundary past the cursor,
  so that repeating them moves on, as Vim's `;` does after a `t`. `;`, `,`, and Helix's `Alt-.`
  repeat the last find from the preset.
- A find makes one pass over the chars, whatever the count, and checks a grapheme boundary only at
  chars that match, mostly from the chunk it scans. In a release build, a miss across 1 MB takes
  0.5 ms, and finding the next `(` on a line 0.3 µs. A char that never starts a grapheme pays a
  check at each occurrence: an accent across 1 MB of decomposed `é` takes 5.2 ms.
- Known limit: finding a regional indicator, half of a flag, inside a long run of flags is
  quadratic. Each match checks its grapheme boundary on its own, and inside the run that check
  reads back to the start of the run, which pairs the run into flags; from the second chunk of a
  run on, the chunk can't answer, so the check reads the rope. A miss across 10k flags (80 KB)
  takes 280 ms, and across 30k flags 2.4 s. A fix was tried and left out: in one run, whether
  each indicator starts a grapheme alternates, so the last one checked answers the next. That
  made such a miss linear (2 ms per MB), but checking every match for a regional indicator
  slowed other finds by up to 23%, and the case is rare. Revisit if long runs of flags turn up.

### Paragraphs

A paragraph is a run of lines that aren't blank, and a blank line is empty or all whitespace, the
blank of `first_non_blank` ([0035](../decisions/0035-find-paragraph-and-bracket-motions.md)). So a
line of spaces splits paragraphs, as it does in the rendered note, where Vim and Helix would join
them.

- `next_blank_line` and `prev_blank_line` move as Vim's `}` and `{` do: past any blank lines, then
  a paragraph, to the blank line past it. `{` never stops on the cursor's own line, even from
  inside a line of spaces, except at the start of the text.
- `next_paragraph_start` moves as Helix's `]p` does, to the first paragraph that starts after
  `offset`, and `prev_paragraph_start` as its `[p` does, to the last one that starts before it.
  From inside a paragraph's first line, `[p` goes to the line's start.
- With no such line, a motion goes to the end or the start of the text, and a count that
  overshoots stops there. Vim fails such a count; the Vim preset stops at the edge too.

| Key | Vim passes | Helix passes | Helix selects from |
|---|---|---|---|
| `}` | the cursor's start | | |
| `{` | the cursor's start | | |
| `]p` | | the cursor's end | the cursor's end if a paragraph starts there, else its start |
| `[p` | | the cursor's start | the cursor's start if a paragraph starts there, else its end |

- After a `]p`, Helix's selection ends where a paragraph starts, and its cursor is on the line
  break before it. Passing the cursor's end, the next `]p` moves on and selects that paragraph
  and the blank lines after it.
- A walk reads each char it passes once, however short the lines. In a release build, a walk
  across 1 MB takes 0.8 ms over lines of Markdown, 1.2 ms over 4-byte lines, and 3 ms over a
  million empty lines. Between paragraphs of prose, a step takes under 1 µs.

### Brackets

`matching_bracket(offset)` goes from a bracket to its partner, as Vim's `%` and Helix's `mm` do
([0035](../decisions/0035-find-paragraph-and-bracket-motions.md)).

- The pairs are `()`, `[]`, `{}`, `“”`, and the full-width and CJK brackets `（）`, `［］`, `｛｝`,
  `「」`, `『』`, `【】`, `〈〉`, `《》`, and `〔〕` ([0038](../decisions/0038-text-objects.md)), but
  not `<>` or `‘’`: in prose, `>` also starts quotes and arrows, and `’` is also an apostrophe.
  `is_bracket(ch)` tells whether a char is one of them.
- A bracket is a grapheme whose first char is one, as in a find: a `(` with a combining accent is
  a bracket, and a `(` after a prepended char such as U+0600 is inside that char's grapheme, so it
  is none.
- An opening bracket's partner is the first closing bracket of its kind after it that closes no
  pair opened in between, on any line; a closing bracket's is the opening one found the same way
  back. Brackets of other kinds don't count, and quotes hide none: in `(a [b) c]`, the `(` pairs
  with the `)`.
- It returns where the partner starts, or `None` if no bracket starts at `offset` or it has no
  partner.

| Key | Vim passes | Helix passes |
|---|---|---|
| `%` | the cursor's start if a bracket starts there, else the first bracket after it on the line | |
| `mm` | | the cursor's start |

- The Vim preset finds the bracket after the cursor with `find_next`, `LineEdges::Stop`, and
  `is_bracket`.
- A scan reads each char once on the way to the partner and checks a grapheme boundary only at
  brackets of the pair's kind, mostly from the chunk it scans. A bracket right after an ASCII char
  skips the check: only a prepended char right before a bracket, such as U+0600, keeps it from
  starting a grapheme (UAX #29, rule GB9b), and no prepended char is ASCII. A unit test checks
  this for every ASCII char and bracket, so a Unicode update that breaks it fails there.
- In a release build, a partner 1 MB away takes 0.5 to 0.6 ms across prose, 0.9 to 1 ms across
  Markdown with links, 1.2 to 1.3 ms across CJK text, and 1.1 to 1.3 ms across JSON, where 13% of
  the chars are braces. A bracket with no partner takes 0.6 ms across 1 MB of prose, a pair on one
  line under 1 µs, and a scan across 1 MB of nothing but `()` 4 to 5 ms.

## Text objects

Text objects are `Text` methods too ([0038](../decisions/0038-text-objects.md)). Each returns a
span on grapheme boundaries, or `None` where there is no such object, and takes an `Extent`:
`Inside` or `Around`. The presets build ranges from the span and keep Vim's linewise objects and
its fallbacks.

| Method | Object | Vim | Helix |
|---|---|---|---|
| `word_object(offset, count, extent, words)` | a word, a run of punctuation, or a run of blanks | `iw`, `aw`, `iW`, `aW` | `miw`, `maw` |
| `sentence_object(offset, count, extent)` | a sentence, or the blanks between two | `is`, `as` | |
| `paragraph_object(offset, count, extent)` | a paragraph, or a run of blank lines | `ip`, `ap` | `mip`, `map` |
| `enclosing_bracket_object(span, count, kind, extent)` | the `count`-th pair of brackets around `span` | `i(`, `a(`, … | `mi(`, `ma(`, `mim`, `mam` |
| `quote_object(offset, quote, extent)` | the string `offset` is in | `i"`, `a'`, … | `mi"`, `ma'`, … |
| `pair_object(offset, count, delimiter, extent)` | the `count`-th `delimiter` on each side | | `mi*`, `ma*` |

- Words, sentences, and paragraphs split the text into units, blank or not: segments of a line,
  sentences and the blanks between them, runs of lines. `Inside` takes `count` units from the one
  at `offset`, of either kind. `Around` takes `count` that aren't blank and the blank ones between
  them, and adds the blank unit after the last, or the one before the first if none follows; from
  a blank unit, it takes that and the next `count` that aren't. A count that overshoots stops at
  the last unit: the line's end for words, the paragraph's for sentences, the text's for
  paragraphs. A count of 0 gives `None`.
- A word object never crosses a line break, so there is none at a line's end or on its break. It
  segments a window as a word motion does and grows it as a count runs on: about 10 µs in the
  middle of a 1 MB line of prose, in a release build.
- A paragraph object's run goes from its first line's start to the start of the line after it, so
  it holds its line breaks. The empty line after a final line break, right after a paragraph, is
  no run: `ap` on a note's last paragraph takes the blank lines before it, as in Vim, and there is
  no object on that line. A paragraph object walks the lines of its runs, and the blank lines
  before them for `Around`: about 1 µs for a short paragraph, 0.9 ms for a paragraph of 1 MB.
- Sentences follow Unicode's sentence boundaries (UAX #29) inside the paragraph at `offset`, which
  the segmenter gets with each line break as spaces of the same length, so a hard-wrapped sentence
  stays whole. A sentence leaves out the blank graphemes around it, a boundary inside a grapheme
  moves to the grapheme's end, and there is none on a blank line. The paragraph is copied and
  segmented whole: 0.2 ms for 10 KB, 2 ms for 100 KB.
- A bracket object takes a span, so that a repeat grows the selection. The brackets after a pivot
  that close a pair it is in pair up with those before it that open one, level by level, as `%`
  pairs them: the pivot is the span's start, or the end of the opening bracket that starts it, so
  a bracket at the span's edge belongs to the pair it opens or closes. A pair that closes inside
  the span is no pair around it, and pairs of any kind (`kind` `None`) come in the order they
  close. A pair whose object, its inside for `Inside` or the whole pair for `Around`, lies within
  the span is skipped, unless the span is one grapheme or none: a cursor. So `mim` from the `x` of
  `[((xy))]` gives `xy`, then `(xy)`, then `((xy))`, and `mam` gives `(xy)`, then `((xy))`, then
  all of it. An inside of one grapheme can't grow, as in Vim: a repeat reads it as a cursor.
- A bracket object scans out from the pivot both ways as `%` does: 0.5 ms for a pair of one kind
  1 MB wide, 1.1 ms for any kind, about 2 µs for a pair around the cursor. `matching_bracket`
  keeps a scan of its own, since its `[open, close]` pattern runs faster than a predicate.
- Quotes are `"`, `'`, `` ` ``, and `‘’`; `‘` and `’` both pick `‘’`. They pair on `offset`'s
  line from its start, the first with the next, and a `'` or `’` with a letter or digit on both
  sides is an apostrophe. `don't say 'hi'` pairs the quotes around `hi`. Vim's fallback to the
  next string on the line, and the blanks its `a"` adds, are the Vim preset's. A quote object
  copies the line and reads it up to the string: 0.6 ms for a string at the end of a 1 MB line.
- A pair of chars is the `count`-th `delimiter` before `offset` and the `count`-th after it,
  found as a find finds them but inside the paragraph at `offset`. There is none if `offset` is
  on a delimiter or either side runs out. Finding the paragraph walks it whole: 0.13 ms for 100
  KB.

## Change sets

A `ChangeSet` is an edit to a whole text: operations that walk the text from start to end. A
`Document` builds one from its own text for each edit. Only `map`, `len_before`, `len_after`, and
`is_identity` are public, so the caller of `Document::edit_with` can map positions through it.

| Operation | Effect |
|---|---|
| `Retain(n)` | keep the next `n` bytes |
| `Delete(n)` | remove the next `n` bytes |
| `Insert(s)` | insert `s` at the current position |

- Retains and deletes add up to the length before the change; retains and inserts, to the length
  after it.
- `ChangeSet::new(text, changes)` builds one from `(range, replacement)` pairs, sorted and not
  overlapping: an empty range inserts, an empty replacement deletes. It returns `InvalidChange`
  for the first range that is reversed, starts before the previous one ends, runs past the end,
  or splits a char.
- Canonical form: no operation is empty, adjacent operations differ in kind, and at one position
  an insert comes before a delete. Equal edits compare equal, and a delete right after an insert
  is always the bytes that insert replaces.
- `apply` edits a `Text`, one rope edit per insert, delete, or insert-and-delete pair.
- `invert(original)` returns the change set that undoes it: retains stay, deletes become inserts
  of the original bytes, and inserts become deletes.
- `a.compose(b)`, where `b` was made for the text `a` produces, returns one change set with the
  effect of both. It walks both lists in step, lining up `a`'s output (retains and inserts) with
  `b`'s input (retains and deletes).

### Mapping positions

`map(position, assoc)` moves a position in the text before a change to the text after it.

| Position | `Assoc::Before` | `Assoc::After` |
|---|---|---|
| in kept bytes | moves with them | moves with them |
| where text is inserted | before the new text | after the new text |
| at the start of or inside deleted bytes | where they were | where they were |
| at the start of replaced bytes | start of the new text | start of the new text |
| inside replaced bytes | start of the new text | end of the new text |

Mapping is monotonic: for one `assoc`, a later position never maps before an earlier one.

## Selections

- A `Range` is two byte offsets, `anchor` and `head`, covering the half-open span from `start()`
  (the smaller) to `end()` (the larger). Extending a range moves its head.
- A `Selection` is one or more ranges, one of them primary. `Selection::new(primary, others)`
  sorts them and merges a range that starts where another starts, or inside it: overlapping
  ranges merge, and so does an empty range inside another. Touching ranges, and an empty range at
  another's end, stay apart. A merged range points backward only if both ranges did, and it is
  primary if either was. Taking the primary range itself means a selection can't be empty or have
  a primary index past its ranges.
- Ranges sit in a `SmallVec` with room for one, so a single-range selection doesn't allocate.
- `yank(text)` copies the text of each range, in the order of the ranges, an empty string for an
  empty range. A register value is that list
  ([0040](../decisions/0040-registers-in-modal.md)); `buffer-modal` stores it.

### Mapping selections

`Selection::map(changes)` maps every range, then merges ranges that now overlap.

- A range's start maps with `Assoc::After` and its end with `Assoc::Before`, so text inserted at
  either edge stays outside it, and a range that contains replaced bytes covers the new text.
- An empty range maps with `Assoc::After`: typing at a cursor moves it past the typed text.
- A range that reaches into replaced bytes from outside keeps only its part outside them.
- A range strictly inside replaced bytes maps past its own end; its ends swap, so it covers the
  new text, keeping its direction.
- At an edge of replaced bytes, both ends land on one point: a range from their start to inside
  them collapses to the start of the new text, and one from inside them to their end collapses to
  the end of the new text. Deleting a range's text collapses it too; Normal mode widens such
  ranges again (see below).
- Sorted, non-overlapping ranges give positions in order, so the mapping is one pass over the
  change set and the ranges together.

### Block cursors and Normal mode

In Normal mode a range shows a block cursor on a grapheme, and every range covers at least one
grapheme ([0032](../decisions/0032-motions-return-targets.md)).

- `cursor(text)` is where the block cursor starts: the grapheme at the head, or the one before
  the head when the range points forward. For an empty range, such as an Insert-mode caret, it
  is the head.
- Motions return [targets](#motions). A move to a target is `Range::point(target)`, which Normal
  mode widens to one grapheme; selecting the span a motion crossed is `Range::new(start, target)`;
  and `extend_to(text, target)` extends a range.
- `extend_to` works as Vim's Visual mode and Helix's Select mode do. The range then covers the
  anchor's grapheme, the target's, and those between, with the block cursor on the target's. The
  anchor's grapheme is the one after the anchor, or the one before it when the range points
  backward. The range points backward when the target comes before that grapheme, so when the
  head crosses it, the anchor moves to its other edge. Extending to the end of the text covers
  the rest of it, with the block cursor on the last grapheme.
- `Selection::for_normal_mode(text)` restores Normal mode's rules. `buffer-modal` applies it
  after every Normal-mode command and edit; `Document` knows no modes.
  - A range end inside a grapheme moves outward to the grapheme's edge. An edit can leave one
    there: another cursor adds a combining accent right after a range, say.
  - An empty range covers the grapheme it is in, or the one that starts at it.
  - At the end of the text, an empty range stays where an empty line starts, in an empty text or
    after a final line break, and covers the last grapheme otherwise. The Vim preset keeps its
    cursor off that empty last line itself.
  - Ranges that now overlap merge, as in `Selection::new`.
- Each range costs a few grapheme lookups, O(log n) each.

### Selection operations

Operations that reshape a selection without an edit are `Selection` methods
([0043](../decisions/0043-selection-operations.md)). Each takes the selection, and the text if it
reads it, and returns the new one; the presets bind commands to them and apply
`for_normal_mode` afterwards.

| Method | Each range becomes | Vim | Helix |
|---|---|---|---|
| `collapse(text)` | an empty range at its block cursor | | `;` |
| `flip()` | itself, anchor and head swapped | `o` | `Alt-;` |
| `extend_to_lines(text)` | whole lines, with their line breaks | `V`, linewise operators | `x` |
| `trim(text)` | itself without the blank graphemes at its ends | | `_` |

- `extend_to_lines` grows a range back to the start of its first line and on to the start of the
  line after its last, or to the end of the text. A range keeps its direction, one that covers
  whole lines already stays as it is, and ranges on one line merge. Taking one more line on a
  repeat, as Helix's `x` does, is the preset's. In Vim's linewise Visual mode the document holds
  the charwise range, and the preset extends it when it acts.
- `trim` counts line breaks as blanks. A range that holds nothing else is dropped, and if it was
  primary, the next range is, or the previous one after the last. If every range is blank,
  nothing changes.
- Each costs a few rope lookups per range, O(log n) each; `trim` also reads the blanks it takes
  off.

| Method | The selection becomes | Vim | Helix |
|---|---|---|---|
| `Selection::all(text)` | one range over the whole text | | `%` |
| `keep_primary()` | the primary range alone | | `,` |
| `drop_primary()` | the other ranges | | `Alt-,` |
| `rotate_primary(count, forward)` | the same ranges, with another one primary | | `)`, `(` |
| `merge()` | one range from the first one's start to the last one's end | | `Alt-minus` |
| `split_lines(text)` | each range's part of every line it covers | | `Alt-s` |
| `copy_to_line(text, below)` | its ranges and a copy of each on the lines below or above | | `C`, `Alt-C` |
| `Selection::block(text, anchor, head, to_line_ends)` | one range per line between two corners | `Ctrl-V` | |

- After `drop_primary`, the next range is primary, or the previous one after the last; the only
  range stays. `rotate_primary` wraps around. `merge` points as the primary did.
- `split_lines` leaves the line breaks out and drops empty parts; a range with no other part, an
  empty one or one of line breaks only, stays whole. The parts point as the range did. Of the
  primary range's parts, the one with its block cursor is primary: the one at the head's end,
  which is the nearest one if the cursor is on a line break. It costs a few lookups per line.
- `copy_to_line` puts each copy as many lines away as its range is tall, with both ends at the
  columns they had, counted in graphemes as for `j` and `k` (revisit with them: display cells).
  - The copy's start must land on a grapheme of its line. A line too short for that is passed
    over for the next one, so an empty line never gets a copy of a block cursor on a char. An
    empty range, a caret, may land at a line's end, and a range that starts on a line break on
    another line's break.
  - A copy whose end finds its line too short ends with the line. The copy of a range that ends
    with a line break ends with one too, or with the text on the last line, so whole lines copy
    onto a last line without a break.
  - A line break alone, the cursor at a line's end, copies as the one grapheme at its column: a
    char of a longer line, or the break of a line that ends there, an empty line's too.
  - A range with no line to hold a copy gets none. The primary range's copy is primary, and a
    copy that overlaps a range merges with it. A count is the preset's.
  - It reads each line it tries up to the column.
- `block` builds Vim's blockwise selection from where the two corners' graphemes start. Each
  line from one corner's to the other's gets a range over the columns between the corners',
  both included, counted in graphemes (revisit with `j` and `k`). With `to_line_ends`, every
  range runs to the end of its line's content, as after `$`.
  - A line that ends before the block's first column gets no range, and one that ends inside it
    a shorter one. There is no padding.
  - The ranges point toward the head's column, and the one on the head's line is primary. A
    head's line that ends before the block gets an empty range at its end, so the cursor has a
    place: it holds no part of the block, and the preset leaves it out when it acts.
  - The corners, the goal column, `o` and `O`, and `gv` are the Vim preset's, which builds the
    block again after each move. `I` and `A` are inserts at every range.
  - It reads each line up to the block's last column.

## Transactions

A `Transaction` is one edit: a change set and the selection after it. It is internal to
`buffer-text`: a document makes one for each edit, and its history keeps them.

- `apply(text, selection)` applies the change and sets the selection; a document applies its
  edits, undos, and redos this way.
- `invert(original, selection_before)` returns the transaction that undoes it, restoring both the
  text and the selection before.
- `compose(next)` combines two transactions into one with both changes and `next`'s selection;
  a document composes the edits of one revision this way.

## Documents and history

A `Document` owns a text, its selection, and its undo history
([0031](../decisions/0031-document-owns-history.md)). Every edit and every move through history
goes through it, and it builds every change set from its own text: a change set fits only the
text it was made for, and applying it checks no more than the length, so one made for another
text could edit the wrong bytes, or panic halfway and leave the text out of step with its history.

- `edit(changes, now)` takes `(range, replacement)` pairs, as `ChangeSet::new` does, and maps the
  selection through the change. `edit_with(changes, selection, now)` sets the selection to what
  `selection` returns, given the change set: a paste, say, selects the pasted text.
- Both check the ranges against the current text before anything changes. A stale one gets
  `Err(InvalidChange)`, and the text, the selection, and the pending revision stay as they were.
- An edit joins the pending revision, composed with the edits before it. `set_selection` changes
  only the selection.
- `paste(strings, placement, select, now)` pastes one string per range of the selection, in the
  order `Selection::yank` returns them ([0040](../decisions/0040-registers-in-modal.md)):
  `Placement::Before` at the range's start, `After` at its end, `Replace` in its place. It is an
  `edit_with` over the document's own ranges.
  - `select(index, span)` returns the range to leave for each string, given the span it takes up
    in the changed text: the span to select what was pasted, as Helix does, or a point in it for
    Vim's cursor. The edit records that selection, so a redo restores it; a selection set after
    the paste would be lost there. `select` runs before the change is applied, so it reads the
    strings, not the changed text.
  - The presets do the rest: mapping a register's values onto the ranges, repeating a string for
    a count, and moving the ranges to line edges for a linewise paste.
- `commit()` ends the pending revision. The caller commits where
  [0029](../decisions/0029-undo-insert-sessions.md) ends an undo step: on leaving Insert mode, on
  the checkpoint command, on a cursor move that isn't typing, and after each Normal-mode command.
- An edit that comes a second or more after the previous one commits the pending revision first.
  The caller passes each edit's time, so `buffer-text` reads no clock and tests set the time.
- A pending revision keeps a clone of the text and selection from before its first edit; text
  clones share structure, so this is cheap. On commit, the composed edits and their inverse become
  a revision. Edits that cancel out, like typing a char and deleting it, leave no revision.
- Undo, redo, and jumps commit first.

### Revisions

Revisions form a tree. The root is the text the document started with. Every other revision keeps
the transaction that made it from its parent, the one that undoes it, and the time of its last
edit. An edit after an undo starts a new branch.

| Method | Moves to |
|---|---|
| `undo()` | the parent, restoring the selection from before the revision |
| `redo()` | the child made or visited last, restoring the selection after it |
| `earlier(n)`, `later(n)` | `n` revisions back or forward in the order they were made, across branches (Vim `g-`/`g+`, Helix `Alt-u`/`Alt-U`) |
| `earlier_by(d)` | the newest revision made at least `d` before the current one, or the root (`:earlier 10m`) |
| `later_by(d)` | the newest revision made at most `d` after the current one (`:later 10m`) |

- Each returns whether it moved: `undo` at the root and `redo` without a child return false.
- Revisions sit in a `Vec` in the order they were made, so a parent comes before its children. A
  jump undoes up to the newest common ancestor, found by stepping the later of the two revisions
  to its parent, then points redo down to the target and redoes along it. Redo after any move
  retraces the way back, as in Vim; Helix's redo goes to the newest child instead.
- As long as the caller's times don't go backwards, revisions are made in time order, and a time
  jump finds its target by binary search.

## Errors and panics

Per [0030](../decisions/0030-panics-and-results.md), input that can be stale gets an error, and a
broken contract only our own bug can cause panics before anything changes.

| Check | On failure |
|---|---|
| `Document::edit`, `edit_with`: a range reversed, out of order, past the end, or splitting a char | `Err(InvalidChange)`, before anything changes; spans can be stale, so the caller recomputes |
| `Document::edit_with`: `selection` returns a selection past the end of the changed text | panic, before anything changes |
| `Document::set_selection`: a selection past the end of the text | panic |
| `Document::paste`: not one string per range, a range of the selection inside a char, or `select` returns a range past the end of the changed text | panic, before anything changes |
| `ChangeSet::map`: a position past the end of the text | panic |
| `Text::is_char_boundary`, `is_grapheme_boundary`: an offset past the end of the text | panic |
| `Text::line_of`, `next_grapheme_boundary`, `prev_grapheme_boundary`: an offset past the end of the text or inside a char | panic |
| `Text::next_grapheme`, `prev_grapheme`, `line_down`, `line_up`, the word motions, `find_next`, `find_prev`, the paragraph motions, `matching_bracket`: an offset past the end of the text or inside a char | panic |
| `Text::line_start`: a line past `line_count()`; `line_end`, `first_non_blank`: `line_count()` or more | panic |
| `Range::cursor` (the head), `extend_to` (the anchor and the target), `Selection::for_normal_mode` (every range end): an offset past the end of the text or inside a char | panic |
| Internal: `apply`, `invert`, `compose` get a text or change set of another length | panic |
| Internal: the primary range survives merging, `compose` ends both lists together, and redo never leads to the root | panic (`expect`, `unreachable!`) |
| Internal: the mapper gets positions in order | `debug_assert!`; only `Selection::map` uses it, with sorted positions |

## Tests

Property tests (proptest) in `buffer-text` check change sets against a plain `String` model:
`apply` matches it, `invert` restores the original, `compose` equals applying both in turn,
mapping keeps untouched chars in place and is monotonic, and every result is canonical.
Generated texts mix multi-byte chars, combining marks, emoji sequences, and line breaks, and some
span several rope chunks. For selections, they check that merging covers exactly the given
ranges, that mapping in one pass equals mapping each range alone, and that ranges no change
touches keep their text; `extend_to_lines`, `trim`, `split_lines`, and `block` are checked
against models that split the string into lines and graphemes, and unit tests pin the primary
range's rules and `copy_to_line`. For lines, they split the text at each `\n` and check the line
count, each line's start and end, and the line of every char boundary against the pieces. For
graphemes, they compare each method with unicode-segmentation run over the whole string: at every
char boundary of a short text, and near each chunk edge of a long one, including runs of regional
indicators, combining accents, joined emoji, and flags after prepended chars, long enough to
cross chunks.

For motions, they step through the string's grapheme boundaries, stopping at the line's edges for
`LineEdges::Stop`. They find each line's first grapheme that isn't all whitespace, in texts of
Unicode spaces, combining accents, and line breaks. And they work out each vertical move from the
string's lines, also checking that a move down and back up with the goal kept returns to the start
when the line it moved to is long enough.

For words, they check that `UnicodeWords` keeps to UAX #29 boundaries on grapheme boundaries,
classifies each segment by its chars, and leaves only words next to their own kind, and that
`BigWords` alternates blank and non-blank runs. Unit tests pin the segments of Markdown lines.
Word motions are compared with a model built from the string's lines and their segments, for both
segmenters and all four motions, and unit tests pin Helix's selections, at line ends too.

For finds, they compare `find_next` and `find_prev` with the string's graphemes, from every char
boundary, inside graphemes and line breaks too, on the line and across it, and for chars that
start graphemes and chars that only continue them. Unit tests pin decomposed accents, emoji
modifiers, prepended chars, `\r\n`, and offsets inside a `\r\n`.

For paragraphs, they compare the four motions with a model worked out from where the string's
lines start and whether each is blank. Offsets are char boundaries, most often where a line
starts, in texts of Unicode spaces, lone `\r`, `\r\n`, and chars that aren't blank, some long
enough to span rope chunks. Unit tests pin Vim's and Helix's stops, lines of spaces, a space with
a combining accent, `\r\n`, and a final line break.

For brackets, they compare `matching_bracket` with a model that pairs the string's brackets from
its start, over its graphemes, closing the last bracket of each kind still open. Offsets are char
boundaries, most often where a bracket char is, in texts of three kinds of brackets, brackets
with a combining accent or after a prepended char, and line breaks, some long enough to span rope
chunks. Unit tests pin nesting, other kinds inside a pair, pairs across lines, CJK and full-width
pairs, angle brackets and curly quotes, and brackets inside graphemes. Another checks that a
grapheme starts at every bracket after an ASCII char, which lets a scan skip the check there.

For quotes and pairs of chars, they compare `quote_object` and `pair_object` with models that
walk the graphemes of the line or the paragraph, from every char boundary, in texts of quotes,
letters and digits around them, line breaks, and chars that join the grapheme before or after
them. Unit tests pin apostrophes, curly quotes, counts, and blank lines.

For Normal mode, they work out what each range becomes on the grapheme boundaries of the whole
string, merge the results, and compare them with `for_normal_mode`, for the generated ranges and
for an empty range at the end of each text; every range must end on grapheme boundaries and be
empty only where the rules allow. `extend_to` must cover exactly the anchor's grapheme, the
target's, and those between, point backward only toward a target before the anchor's grapheme,
and put the block cursor on the target.

For documents, a property test runs random edits (with pauses around a second), commits, undos,
redos, and jumps by count and by time. After each, the text must be exactly the text of the
revision the history is at, pending edits must start from it, and a jump by count must land
where it should; at the end, the oldest and the newest revision must be reachable. Unit tests pin
the rules: the pause boundary, commit points, edits that cancel out, rejected edits, restored
selections, redo after a jump into another branch, and the limits of time jumps. For pastes, a
property test checks the text against a model for each placement, and that each span `select`
gets holds its string; unit tests pin the three placements and the selection a redo restores.
