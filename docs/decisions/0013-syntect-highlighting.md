# 0013. syntect with the syntax set typst already links

- Status: Accepted
- Date: 2026-10-26

## Context

v1 renders highlighted code blocks. typst ([0012](0012-typst-math.md)) already links syntect and
the bat syntax set (`typst::text::RAW_SYNTAXES`, via two-face 0.4). The dependency policy asks to
reuse what egui or typst link, and bars C/C++ build dependencies beyond what egui/wgpu need without
an explicit decision. Notes are untrusted input (invariant 9).

## Decision

- Highlight with syntect 5.3, using the syntax set typst already links. No second syntax dump.
- Pure Rust: syntect's regex engine is fancy-regex.
- Initialize lazily, off the UI thread.

## Consequences

- No new syntax data and no C toolchain; highlighting adds little beyond what typst brings.
- Language coverage is whatever bat's syntax set covers.
- Highlighting runs on render workers, which post results over channels and request a repaint.
  Highlight workers run under `catch_unwind`, so a highlighter bug can't take down unsaved text
  ([0023](0023-errors-panic-isolation.md)).
- Highlighted code blocks land with block layout in M3; docs/rendering.md covers them.
- The choice is tied to typst's syntect and two-face versions; upgrades move together.

## Alternatives considered

- tree-sitter highlighting. Rejected: needs a C toolchain on every target, and the binary grows
  per grammar.
