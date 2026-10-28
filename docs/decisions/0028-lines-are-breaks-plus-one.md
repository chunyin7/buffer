# 0028. A text has one more line than line breaks

- Status: Accepted
- Date: 2026-10-28

## Context

Line numbers drive vertical motion, line-wise operations, and edit-view layout. Positions are byte
offsets, and a cursor may sit at any offset from 0 to the text's length, including after a final
line break. crop ([0016](0016-crop-rope.md)) treats a final line break as the end of the last
line: `"foo\n"` has one line by `line_len`, yet `line_of_byte` puts offset 4 on line 1, past the
last line. Vim also shows `"foo\n"` as one line; Helix, CodeMirror (Obsidian's editor), and VS Code
show two, the second one empty. Open + save is byte-identical, so the model must not add or drop a
final line break.

## Decision

- A text has one more line than it has line breaks. `\n` and `\r\n` are line breaks; a lone `\r`
  is not.
- Every offset from 0 to the length lies on exactly one line: `"foo\n"` has two lines, the second
  empty, and an empty text has one empty line.
- `buffer-text` exposes this model, not crop's `line_len`.
- The Vim preset keeps its Normal-mode cursor off the empty line after a final line break, since
  Vim shows no such line.

## Consequences

- Line math has no end-of-text special cases, and the offset after a final line break is always
  addressable.
- The model matches Helix, CodeMirror, and VS Code; the Vim preset clamps its cursor instead.
- The model only addresses bytes: whether a file ends with a line break is left as it is.
- docs/editor/model.md describes lines once `buffer-text` exposes them.

## Alternatives considered

- crop's and Vim's model, where a final line break ends the last line: the offset after it belongs
  to no line, so every line computation needs an end-of-text special case.
