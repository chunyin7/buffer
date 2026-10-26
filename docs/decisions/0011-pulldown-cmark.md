# 0011. pulldown-cmark plus our own Obsidian pass

- Status: Accepted
- Date: 2026-10-26

## Context

The format is CommonMark + GFM plus Obsidian syntax, except math
([0008](0008-obsidian-compatible-format.md)). Positions are UTF-8 byte offsets (invariant 4), and
render nodes carry source byte ranges so the Edit ↔ Render toggle, and later Live Preview, can map
positions (invariant 5). Notes are untrusted input: parsers never panic on any input, and they are
fuzzed (invariant 9).

## Decision

- Parse with pulldown-cmark 0.13. Its offset iterator gives byte ranges for every event.
- Use its built-in extensions: tables, task lists, strikethrough, footnotes, math, wikilinks,
  metadata blocks.
- Obsidian extras are our own pass over the event stream: `#tags`, `==highlight==`,
  `%%comments%%`, callout types, titles, and folding, `^block-ids`, and embeds.

## Consequences

- Byte ranges come straight from the parser, so render nodes ([0015](0015-own-inline-layout.md))
  and index extraction carry source offsets without a second mapping.
- The Obsidian pass is our code in `buffer-core` and must meet invariant 9: it never panics, and it
  gets fuzz targets like every parser.
- The pass owns the edge cases of the dialect, e.g. tags never inside code, at least one non-digit.
  docs/markdown-dialect.md (M2) specifies the syntax and the Obsidian compatibility matrix.
- `$…$` and `$$…$$` reach the renderer as math events holding typst source
  ([0012](0012-typst-math.md)).
- Embed resolution starts with images ([0014](0014-images.md)); note transclusion depth and editing
  semantics are an open question for M3.
