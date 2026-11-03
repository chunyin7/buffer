# 0035. Find, paragraph, and bracket motions

- Status: Accepted
- Date: 2026-11-03

## Context

Graphemes, lines, and words have motions ([0032](0032-motions-return-targets.md),
[0034](0034-word-motions-stop-at-line-edges.md)). Both presets also find a char (`f`, `t`, `F`,
`T`), move by paragraphs, and jump to the matching bracket. Vim and Helix differ:

- `f` and `t` stay on the line in Vim and cross lines in Helix. Both match chars, so Helix can stop
  inside a grapheme. Both fail when a count asks for more matches than there are.
- Vim's `{` and `}`, and Helix, split paragraphs at empty lines only. Vim's `ip` and `ap` also
  split them at lines of whitespace, and so does CommonMark, so the rendered note does too.
- Vim's `}` lands on the blank line after a paragraph, and `{` on the blank line before it. Helix's
  `]p` and `[p` land on a paragraph's first line; `]p` selects the paragraph and the blank lines
  after it, as `w` selects a word and its spaces. With a count past the last paragraph, Vim fails
  and Helix stops at the end of the text.
- Vim's `%` takes the first bracket at or after the cursor on its line and jumps to its partner.
  Helix's `mm` without a syntax tree needs the cursor on a bracket and scans at most 10,000 chars.
  Vim matches `()`, `[]`, and `{}`, and skips brackets inside quotes, for C. Helix adds `<>`, `‘’`,
  `“”`, `«»`, `「」`, and `（）`.

Notes are prose, often in CJK: `>` starts quotes and arrows, `’` is also an apostrophe, and `「」`
quote speech. On a 1 MB note in a release build, a scan for a char that isn't there takes 0.06 ms
with `str::find` (memchr) and 0.42 ms with a loop that tests each char. A scan to the end that
counts brackets takes 0.79 ms, or 1.95 ms with a grapheme check per bracket. Checking 11,000 lines
for a blank one takes 0.83 ms.

## Decision

- A find matches a grapheme by its first char, so `fe` finds a decomposed `é` and no motion stops
  inside a grapheme.
  - `find_next` and `find_prev` take a char predicate and `LineEdges`. They return the start of the
    count-th matching grapheme after, or before, the offset: on its line for `Stop` (Vim), anywhere
    for `Cross` (Helix).
  - A count past the last match fails, and the motion returns no target. This is an exception to
    0032, where a count stops at the last target.
  - The presets build `t` and `T`, their selections, and the repeats (`;`, `,`, `Alt-.`) from the
    match and grapheme boundaries.
- A paragraph is a run of lines that aren't blank. A blank line is empty or all whitespace, the
  blank of `first_non_blank`.
  - `next_blank_line` and `prev_blank_line` give Vim's `}` and `{`: the blank line past a
    paragraph, after it going forward and before it going backward.
  - `next_paragraph_start` and `prev_paragraph_start` give Helix's `]p` and `[p`: a paragraph's
    first line.
  - Each returns the start of that line, or the end or start of the text when there is none. A
    count that overshoots stops there, as in 0032.
- `matching_bracket` takes the grapheme at an offset. If its first char is a bracket, it returns the
  start of the partner, counting nested pairs of the same kind, across lines.
  - The pairs are `()`, `[]`, `{}`, and the full-width and CJK brackets `（）`, `［］`, `｛｝`,
    `「」`, `『』`, `【】`, `〈〉`, `《》`, and `〔〕`.
  - Brackets inside quotes count like any other.
  - Vim's search along the line is `find_next` with the bracket test, in the Vim preset.
- Scans have no limit. One that comes close to the keystroke budget gets faster, not capped.

## Consequences

- Each preset keeps its own find through `LineEdges`. Helix's `f<Enter>`, which finds a line end,
  stays a line motion in its preset.
- A line of spaces splits paragraphs for `}` and `]p`, where Vim and Helix would join them.
- The Vim preset stops `5}` at the end of the text, where Vim fails.
- `%` doesn't pair `<>` or curly quotes. The pair text objects, which come next, use the same pairs.
- None of these motions knows Markdown. `}` doesn't stop at a heading or a list item that has no
  blank line before it, and `%` doesn't skip code spans. Block motions need the parser (M2), and
  would be separate commands.
- On a 1 MB note, the scans measured above fit the keystroke budget (≤ 4 ms). docs/editor/model.md
  describes each motion as it lands, with its measured costs.

## Alternatives considered

- Matching chars, as both editors do: a find could stop inside a grapheme.
- Matching whole graphemes: `fe` wouldn't find a decomposed `é`, nor `f👍` a `👍🏽`.
- A char instead of a predicate: memchr makes a miss 0.06 ms per MB instead of 0.42 ms, but Vim's
  `%` would need its own bracket search.
- One line rule for find in both presets, as for words (0034): a find has no span to keep off line
  breaks, and `LineEdges` already says which rule a preset wants.
- Stopping at the last match when a count overshoots: `d3fx` with two `x` left would delete up to
  the second.
- Splitting paragraphs at empty lines only: a line of spaces, which the note doesn't show, would
  join two paragraphs.
- Two paragraph methods with a stop kind, as `LineEdges` is for graphemes: four methods name each
  target.
- Vim's pairs, which leave out CJK brackets, or Helix's, whose `<>` and curly quotes pair wrongly
  in prose.
- A scan limit, as Helix's 10,000 chars: `%` would fail on far partners, and the worst cases fit
  the budget.
