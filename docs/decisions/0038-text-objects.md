# 0038. Text objects

- Status: Accepted
- Date: 2026-11-09

## Context

M1 needs text objects for both presets ([0017](0017-selection-based-core.md),
[0018](0018-vim-helix-presets.md)). Vim has `iw` `aw` `iW` `aW` `is` `as` `ip` `ap`, bracket
blocks (`i(`, `a(`, …; a count goes out a level), and quotes (`i"`, `a'`, …). Helix has
`mi`/`ma` with words, WORDs, paragraphs, brackets, the closest pair of any kind (`m`), any char
as its own pair (`mi*`), and tree-sitter objects.

- Quotes open and close with the same char. Vim pairs the quotes on the line from its start, the
  first with the second and so on, and from outside a string takes the next one on the line.
  Helix takes the nearest quote before the cursor and the nearest after it, anywhere in the text,
  so between two strings it selects the text between them. Neither knows apostrophes:
  `don't say 'hi'` pairs the `'` of `don't`.
- The pairs of [0035](0035-find-paragraph-and-bracket-motions.md) leave out curly quotes. `“”`
  means nothing else in prose; `’` is also the typographic apostrophe.
- Vim's sentences end at `.`, `!`, or `?`, then closing brackets or quotes, then a blank or the
  line's end; that misses `。` and `！`. Unicode's sentence boundaries (UAX #29), which the
  unicode-segmentation we already link provides, handle them. With unicode-segmentation 1.13:
  `See e.g. the docs. Then go.` is two sentences, `Version 3.14 is out. Ok.` two, and
  `日本語です。次の文です！最後` three; `Ask Mr. Smith now.` breaks after `Mr.`, as Vim's rule
  does, and every line break ends a sentence, so a hard-wrapped one splits.
- Vim skips escaped quotes and brackets; Helix doesn't. In Markdown, a backslash escapes nothing
  inside code spans and blocks, and telling those apart needs the parser (M2).

## Decision

- A text object is a `Text` method that returns `Option<Range<usize>>`: a span on grapheme
  boundaries, or `None` where there is no such object. Each takes an `Extent`, `Inside` or
  `Around`. The presets build ranges from the span and keep Vim's linewise objects and the
  fallbacks below.
- A delimiter is a grapheme whose first char is one, as in finds (0035). A backslash escapes
  nothing.
- Words and WORDs take a `Words` segmenter ([0032](0032-motions-return-targets.md),
  [0036](0036-word-motions-segment-a-window.md)). `Inside` is the segment at the offset: a word,
  a run of punctuation, or a run of blanks. `Around` adds the blanks after it, or before it if
  none follow; from blanks, it takes them and the segment after. A count takes that many
  segments: `Inside` counts runs of blanks, `Around` doesn't. No span crosses a line break
  ([0034](0034-word-motions-stop-at-line-edges.md)).
- Paragraphs use the blank lines of 0035. `Inside` is the run of lines at the offset, a paragraph
  or blank lines, from its first line's start to the start of the line after it. `Around` adds the
  blank lines after it, or before it if none follow. Counts as for words. The empty line after a
  final line break (0028), right after a paragraph, is no blank line here: `Around` on a note's
  last paragraph takes the blank lines before it, and that line holds no object.
- Sentences follow UAX #29 inside the paragraph at the offset, which the segmenter gets with its
  line breaks as spaces of the same length, so a hard-wrapped sentence stays whole. `Inside` is
  the sentence without the blanks after it, or a run of blanks; `Around` adds the blanks after it,
  or before it if none follow. Counts as for words.
- Bracket objects use the pairs of 0035 plus `“”`, which `matching_bracket` pairs too. The method
  takes a span, a count, and a pair kind or none for any kind (Helix's `m`), and returns the
  count-th pair that encloses the span, on any line, without a limit. A bracket at the span's edge
  belongs to the pair it opens or closes. A pair whose object, its inside for `Inside` or the
  whole pair for `Around`, lies within the span is skipped for the next one out, so a repeat grows
  the selection; a span of one grapheme or none is a cursor and skips none, so `i(` on the `x` of
  `(x)` gives `x`, as Vim does. `Inside` lies between the brackets and may be empty; `Around`
  includes them.
- Quotes are `"`, `'`, `` ` ``, and `‘’`, in both presets. They pair on the offset's line from its
  start, as in Vim. A `'` or `’` with a letter or digit on both sides is an apostrophe and doesn't
  count. The method returns the string the offset is in, a quote at the offset included. `Around`
  includes the quotes only; the blanks Vim's `a"` adds are the Vim preset's. No count.
- Any other char pairs with itself, as in Helix's `mi*`: the count-th one before the offset and
  the count-th one after it, inside the offset's paragraph. `None` if the offset is on one, or if
  either side runs out.
- Fallbacks stay in the presets. Outside any pair, Vim's `i(` takes the next `(` (found with
  `find_next`) and its pair, and Vim's quote objects take the next string on the line; Helix
  fails. Vim's error on an empty `i(` is the Vim preset's.

## Consequences

- One method per kind of object serves both presets; Helix's `mi"` pairs as Vim's `i"` does.
- `%` pairs `“”` too. This supersedes the pair list of 0035.
- `the users' notes` still pairs the possessive `'`; `rock 'n' roll` pairs right.
- A selection of one grapheme is a cursor, so a bracket object's inside of one grapheme doesn't
  grow on a repeat, as in Vim: `i(` on the `x` of `((x))` gives `x` again.
- A sentence breaks after `Mr.`, and list items without blank lines between them or final full
  stops are one sentence. Both need the parser (M2) or a list of abbreviations.
- `\"` and `\*` count as delimiters, even outside code. Revisit with the parser.
- Bracket objects scan as `%` does; quotes read one line; any-char pairs and sentences read up to a
  paragraph, and segmenting it costs O(paragraph). A window, as in 0036, if measurements call for
  one. docs/editor/model.md records the costs.

## Alternatives considered

- Helix's nearest quotes: between two strings, they select the text between them.
- `‘’` as brackets: the `’` of `don’t` closes `‘don’t’` early.
- Vim's sentence rule: misses `。` and `！`, and breaks after `Mr.` as UAX #29 does.
- UAX #29 over the raw text: a hard-wrapped sentence splits at each line break.
- Any-char pairs across the text, as in Helix: a stray `*` pairs with one pages away, and inline
  Markdown never crosses a blank line.
- Escapes outside code spans, from a scan of the paragraph: fenced and indented code blocks need
  the parser, and the scan would be a second, partial Markdown parser.
- Pair objects from an offset rather than a span: each preset would need its own logic to grow
  a selection.
- Skipping a pair whose inside or whole equals the span: a cursor on the one grapheme inside
  `(x)` would skip that pair, and on `((x))`, `a(` from `(x)`, the inner pair and the outer one's
  inside, would skip both.
