# 0017. Selection-based editing core

- Status: Accepted
- Date: 2026-10-26

## Context

buffer ships Vim and Helix presets together ([0018](0018-vim-helix-presets.md)). Vim edits with
operator + motion; Helix and Kakoune select first, then act. Text lives in a rope wrapped by
`buffer-text` ([0016](0016-crop-rope.md)), with positions as UTF-8 byte offsets.

## Decision

- `buffer-text` uses a selection-based core, Helix/Kakoune style.
- Every edit is a transaction: a change set plus the resulting selections.
- Selections map through changes.
- Undo history is a revision tree.
- Vim's operator + motion becomes "select, then act", so one core serves both presets.

## Consequences

- One editing model for both presets; the Vim preset translates into selections instead of
  running its own engine.
- Unit and property tests in `buffer-text` cover the core: transactions compose and invert, and
  selections map through changes. Keymap test tables run for both presets against the same core.
- M1 builds the core: transactions, selections, revision-tree history, grapheme-aware motions,
  text objects, registers, counts, repeat. docs/editor/model.md (M1) documents it.
- Open questions for M1: undo granularity in Insert mode, and whether undo history persists across
  sessions.
