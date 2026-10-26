# 0016. crop rope behind buffer-text

- Status: Accepted
- Date: 2026-10-26

## Context

The editor needs a text buffer that stays fast on large notes: the keystroke budget is ≤ 4 ms p99
(our CPU, edit view) on a 1 MB note. Positions are UTF-8 byte offsets (invariant "Positions are
UTF-8 byte offsets"), converted to chars, graphemes, or UTF-16 only at layout, IME, and platform
boundaries. Background work needs snapshots of the text without blocking the UI thread.

## Decision

- Store note text in a `crop` 0.4 rope.
- `buffer-text` wraps it; no other crate depends on `crop` directly.

## Consequences

- `crop` is byte-indexed, so it matches the byte-offset invariant without a conversion layer.
- Clones are cheap and copy-on-write, so workers get snapshots without copying the text.
- The wrapper keeps a later switch (e.g. ropey 2 once stable) local to `buffer-text`.
- `buffer-text` is a core crate: no egui, typst, or platform APIs, tested without a GPU
  ([0025](0025-crate-architecture.md)).
- M1 exit requires a 1 MB note to meet the keystroke budget. docs/editor/model.md (M1) covers the
  rope, transactions ([0017](0017-selection-based-core.md)), and byte/grapheme rules.
