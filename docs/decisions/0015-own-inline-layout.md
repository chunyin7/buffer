# 0015. Own inline layout

- Status: Accepted
- Date: 2026-10-26

## Context

The render view puts math ([0012](0012-typst-math.md)), images ([0014](0014-images.md)), and chips
inside lines of text. egui's `LayoutJob` ([0003](0003-egui-eframe.md)) has text sections only, with
no inline boxes. Render nodes carry source byte ranges (invariant 5), and per-frame work is
O(visible), with layout virtualized and cached by (content hash, width, style, pixels-per-point)
(invariant 6). Live Preview (M5) must not be precluded.

## Decision

- `buffer-render` does inline layout itself and places math, images, and chips inside lines.
- Text runs use egui fonts, shaped with harfrust.

## Consequences

- We own line breaking and the placement of inline boxes; egui fonts only shape the text runs.
- Layout output keeps source byte ranges for the Edit ↔ Render toggle and for Live Preview, which
  builds on render-tree source ranges (docs/live-preview.md, M5).
- Layout is cached by (content hash, width, style, pixels-per-point) and laid out only for the
  visible range.
- M3 delivers it: emphasis, links, inline code, inline math, images. docs/rendering.md covers block
  and inline layout, caching, virtualization, and the Edit ↔ Render mapping.
- Fonts (bundled vs system) remain an open question for M3.
