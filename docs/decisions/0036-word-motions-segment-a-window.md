# 0036. Word motions segment a window between restart points

- Status: Accepted
- Date: 2026-11-08

## Context

Word motions segmented the whole line of every step they took
([0032](0032-motions-return-targets.md), [0034](0034-word-motions-stop-at-line-edges.md)), and
0032 left a bounded window for when measurements called for it. They do: on a 1 MB line, `w`,
`e`, `b`, and `W` took about 20 ms each in a release build, against a 4 ms budget per keystroke.
`split_word_bound_indices` took 12.5 ms of it, and the grapheme check on each segment of
`UnicodeWords` about 4.7 ms.

Word and grapheme boundaries depend on the chars around them, so a piece cut out of a line
segments differently near its edges. `a.b` is one word, but cut after `a.`, the piece ends in a
stop after `a`. A line break is always a boundary, and no rule looks across it, so a line was the
smallest piece a motion could segment alone.

## Decision

- `Words` has a required method, `restarts_between(earlier, before, after)`: whether there is a
  restart point between `before` and `after`, three chars next to each other on a line. Splitting
  a line at its restart points and segmenting each piece gives the same segments as segmenting the
  whole line. `segments` takes such a piece, between line edges or restart points.
- `UnicodeWords` and `BigWords` restart where a word starts after a blank: `before` is whitespace,
  `after` is a letter or digit, both sides of `before` are grapheme boundaries, and the side before
  `after` is a word boundary. Graphemes reach across the chars on either side, so this takes three
  chars, not two:
  - A prepended char such as U+0600 ARABIC NUMBER SIGN joins the blank after it into its grapheme:
    `"\u{600} a"` is one WORD, and in `"\u{600}  ."` the blanks join the punctuation before them,
    which merges with the `.`.
  - A combining mark makes a blank grapheme punctuation: `". \u{301}"` is one segment.
  - Only a word after the blank is safe: a word never merges with the segment before it.
- A step segments a window around its start: from the last restart point before the start to the
  first one at least 256 bytes after it going forward, mirrored going backward. If the window holds
  no stop past the start, it grows toward the far edge by another piece of at least 256 bytes,
  until it reaches the line's edge. Pieces meet at restart points, so nothing is segmented twice.
  Steps on the same line reuse the window.
- A line without restart points is segmented whole, as before.

## Consequences

- A word motion costs time in proportion to the distance it moves plus 256 bytes, not to the
  line. On a 1 MB line, `w`, `e`, `b`, and `W` take 5 to 8 µs in ASCII, Cyrillic, and Japanese
  text with spaces.
- Lines without a blank before a letter or digit stay O(line) per step: minified code, base64,
  and CJK prose written without spaces. A 1 MB line of `lorem_ipsum,dolor(sit)amet.` still takes
  about 22 ms.
- Each segmenter has to state its restart points. A property test checks, for both segmenters,
  that splitting lines at them leaves their segments unchanged.
- 256 bytes is where setting up a window stops mattering: a window costs about 0.5 µs plus about
  14 ns a byte, so 256 bytes take about 4 µs, and smaller windows pay mostly for the setup.
- docs/editor/model.md describes the windows.

## Alternatives considered

- Restart points from two chars, `before` and `after`: no rule on two chars holds for both
  segmenters, because of the prepended chars and combining marks above.
- Restart points where a run of blanks starts, or where one ends before any char that isn't
  blank: both fail on the cases above.
- A default of `false` for `restarts_between`, so a segmenter could leave it out and keep
  segmenting whole lines: explicit is clearer, and both segmenters define it.
- Caching each line's stops across keypresses: the first press, and every press after an edit,
  still costs O(line).
- One piece between two restart points as the smallest window: about a word, so most of its cost
  is the setup. 1 KiB: four times the work of 256 bytes for no measurable gain.
