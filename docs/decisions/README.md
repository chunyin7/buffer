# Architecture decision records

One file per decision, `NNNN-slug.md`, numbered in order. ADRs are append-only: to change a
decision, write a new ADR that supersedes the old one, and set the old one's status to
`Superseded by [NNNN](NNNN-slug.md)`. That status line is the only edit an accepted ADR gets.
[AGENTS.md](../../AGENTS.md) summarizes the decisions in force.

0001–0026 were backfilled on 2026-10-26 from the decisions in AGENTS.md.

## Template

```markdown
# NNNN. Title

- Status: Accepted | Superseded by [NNNN](NNNN-slug.md)
- Date: YYYY-MM-DD

## Context

The forces at play: the problem, constraints, and measurements.

## Decision

What we do, stated directly.

## Consequences

What gets easier or harder, costs accepted, and follow-up work.

## Alternatives considered

Each rejected option with the reason. Omit the section if none were weighed.
```

## Index

| ADR | Decision |
|---|---|
| [0001](0001-rust-2024-latest-stable.md) | Rust 2024 on the latest stable toolchain |
| [0002](0002-dual-license.md) | Dual license: MIT OR Apache-2.0 |
| [0003](0003-egui-eframe.md) | egui/eframe with hand-rolled widgets |
| [0004](0004-wgpu-everywhere.md) | wgpu on every platform |
| [0005](0005-eframe-features.md) | eframe feature set |
| [0006](0006-platforms.md) | Desktop first, iPad later |
| [0007](0007-files-are-the-truth.md) | Files are the truth; the index is a cache |
| [0008](0008-obsidian-compatible-format.md) | Obsidian-compatible format, except math |
| [0009](0009-frontmatter-reader.md) | Own span-preserving frontmatter reader |
| [0010](0010-toml-config.md) | TOML config in the platform config dir |
| [0011](0011-pulldown-cmark.md) | pulldown-cmark plus our own Obsidian pass |
| [0012](0012-typst-math.md) | Typst math, rendered by typst as a library |
| [0013](0013-syntect-highlighting.md) | syntect with the syntax set typst already links |
| [0014](0014-images.md) | Images via image and usvg |
| [0015](0015-own-inline-layout.md) | Own inline layout |
| [0016](0016-crop-rope.md) | crop rope behind buffer-text |
| [0017](0017-selection-based-core.md) | Selection-based editing core |
| [0018](0018-vim-helix-presets.md) | Vim and Helix presets, keymaps as data |
| [0019](0019-search-index.md) | Own in-memory search index with frizbee |
| [0020](0020-no-async-runtime.md) | No async runtime |
| [0021](0021-file-watching.md) | File watching with notify |
| [0022](0022-logging-profiling.md) | log and profiling facades |
| [0023](0023-errors-panic-isolation.md) | thiserror, and panic isolation for workers |
| [0024](0024-whiteboard-pen-input.md) | Whiteboard pen input needs winit 0.31 |
| [0025](0025-crate-architecture.md) | Crate architecture |
| [0026](0026-dependency-policy.md) | Dependency policy |
| [0027](0027-helix-selection-ranges.md) | Selections are Helix-style ranges |
| [0028](0028-lines-are-breaks-plus-one.md) | A text has one more line than line breaks |
