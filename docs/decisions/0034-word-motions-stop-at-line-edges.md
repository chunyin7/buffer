# 0034. Word motions stop at line edges, as Helix's do

- Status: Accepted
- Date: 2026-11-02

## Context

Word motions follow Unicode's word boundaries and return a target, plus where the span they cross
starts ([0032](0032-motions-return-targets.md)). Vim and Helix treat line breaks differently:

- Vim counts a line break as blank and an empty line as a word. `w` on a line's last word goes to
  the next line's first word, stopping on each empty line on the way. `b` and `ge` stop on empty
  lines too; `e` doesn't.
- Helix never lets a word span cross a line break. `w` on a line's last word selects up to the
  line's end, and the next `w` skips the break and any empty lines. `e` does the same, and `b`
  stops at line starts.

The presets also start from different ends of the cursor. On the space in `foo bar`, Vim's `w`
lands on `b`, while Helix's selects `bar`: Vim searches from the cursor's start, Helix from its
end.

## Decision

- Word motions follow Helix. A step stops at the next boundary of its kind on the line, or at the
  line's edge: its end going forward, its start going backward. A step that starts at a line's
  edge first skips the line break and any empty lines. Empty lines are never stops.
- `buffer-text` has this one behavior, with no parameter for line breaks.
- A word motion returns the count-th stop strictly after, or before, the offset it gets. It also
  returns where the last step's span starts when that isn't at the cursor: at the previous stop,
  at the offset when it is a stop, or at the line edge the step started from.
- The presets pass the cursor's start or its end. Helix passes the end going forward and the start
  going backward. Vim passes the start for `w` and `b`, and the end for `e` and `ge`.

## Consequences

- A step stays on one line, so a motion segments one line per step.
- Helix's word selections never hold a line break, so `wd` on a line's last word keeps the break.
- The Vim preset adapts where Vim differs. Its motions also stop at line edges, where Vim's don't
  and where its cursor sometimes can't stand, so it steps again there. Its `w`, `b`, and `ge` no
  longer stop on empty lines.
- docs/editor/model.md describes word motions.

## Alternatives considered

- Both behaviors, chosen by `LineEdges` or by a word-only enum: Vim's motions exactly, for one more
  parameter on every word motion.
- Vim's behavior for both presets: Helix's `w` would select across line ends, so `wd` on a line's
  last word would also delete the line break.
