# 0029. Undo per Insert session, split by pauses; history in memory

- Status: Accepted
- Date: 2026-10-28

## Context

Undo history is a revision tree ([0017](0017-selection-based-core.md)), and 0017 leaves two
questions for M1: how much Insert-mode typing one undo step covers, and whether history outlives
the session. Vim and Helix make one revision per Insert session, from entering Insert mode to
leaving it, and split it on an explicit checkpoint (Vim `<C-g>u`, Helix `C-s`); Vim also splits it
on cursor keys. Notes are prose, so one Insert session can hold several paragraphs. Idle must be
free: nothing runs while nothing happens (invariant "Idle is free").

## Decision

- Insert-mode edits accumulate into one revision per Insert session. A new revision starts at:
  - an explicit checkpoint command, bound in each preset where Vim has `<C-g>u` and Helix `C-s`;
  - a cursor move that isn't typing, such as an arrow key or a mouse click;
  - a typing pause: an edit that comes 1 s or more after the previous one.
- The pause is detected from timestamps when the next edit arrives; no timer runs.
- Undo history lives in memory with its buffer and is dropped when the buffer closes.

## Consequences

- Typing without pauses undoes as in Vim and Helix; pauses give long prose sessions finer undo
  steps, which diverges from both.
- The history API takes commit points and timestamps from its caller, so `buffer-text` stays pure
  and its tests control time.
- Revisions are plain data (change sets and selections), so persisting them later needs no
  redesign.
- Closing a buffer, or the app, discards its undo history.
- docs/editor/model.md describes history once it lands.

## Alternatives considered

- One revision per Insert session only, as in Vim and Helix: a long session undoes as one block.
- A revision per word or line break: the finest steps, but it floods the revision tree.
- Persisting history in the platform cache dir, keyed by path and content hash: needs a versioned
  format, a size cap, and cleanup.
