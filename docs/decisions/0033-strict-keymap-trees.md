# 0033. Keymaps are strict trees, with no timeouts

- Status: Accepted
- Date: 2026-11-01

## Context

Keymaps bind key sequences to named commands, and user config overrides presets
([0018](0018-vim-helix-presets.md)). Vim lets one sequence be both a command and the start of
longer ones, as when a user maps `,` and `,f`: after `,` it waits `timeoutlen` (1 s by default)
for another key before running the shorter mapping. Helix doesn't: a key leads to a command or to
a submap. Idle must be free (invariant "Idle is free"), so such a wait needs a bounded repaint
timer, and pending-key hints should list exactly the keys that can follow.

## Decision

- A key sequence is bound to a command or starts longer sequences, never both. Dispatch never
  waits on a timer.
- A binding replaces whatever its keys overlap: a command bound where longer sequences started
  unbinds them, and a sequence through a command's keys unbinds that command, turning its keys
  into a prefix. Presets and user config bind in turn, so config overrides a preset by binding
  the same keys.

## Consequences

- Each key press resolves at once to a command, a pending prefix, or nothing; dispatch keeps no
  timer state.
- Pending-key hints list exactly the keys that may come next.
- Vim users can't keep overlapping mappings such as `,` and `,f`; one of them has to move.
- A binding that unbinds another does so silently. Checking presets and config for that can come
  with the TOML format.
- Letting a sequence be both, with a timeout, stays possible later as an addition.
- docs/editor/keymaps.md describes keymaps.

## Alternatives considered

- Vim's `timeoutlen`: keeps overlapping mappings, but the shorter one runs only after a delay,
  and dispatch needs a timer state and a node that is both command and prefix.
- Rejecting a binding that overlaps another: catches mistakes in a preset, but a user override
  would have to unbind the preset's keys first.
