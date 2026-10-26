# 0014. Images via image and usvg

- Status: Accepted
- Date: 2026-10-26

## Context

v1 renders images, including Obsidian embeds `![[…]]` with `|width` / `|WxH` sizing
([0008](0008-obsidian-compatible-format.md)). typst ([0012](0012-typst-math.md)) already links
`image` and usvg, and the dependency policy asks to reuse what typst links. Per-frame work must be
O(visible) (invariant 6), and notes and their attachments are untrusted input (invariant 9).

## Decision

- Raster images: `image` 0.25 with png, jpeg, gif, and webp.
- SVG: usvg.
- Decode off the UI thread; downscale to display size.

## Consequences

- No new image crates beyond what typst brings.
- Image decode runs on render workers under `catch_unwind`, so a decoder bug can't take down
  unsaved text ([0023](0023-errors-panic-isolation.md)). Workers post results over channels and
  request a repaint.
- Downscaled textures keep memory tied to display size, not source size.
- Our inline layout places images inside lines ([0015](0015-own-inline-layout.md)); embed sizing
  feeds it. Images land in M3; docs/rendering.md covers them.
- Embeds start with images; note transclusion comes later (open question for M3).
- Where pasted images go (vault root, `attachments/`, or Obsidian's setting) is an open question
  for M2.
