# 0025. Crate architecture

- Status: Accepted
- Date: 2026-10-26

## Context

buffer splits into a vault and search core, a text-editing core, modal editing, a render view,
typst math, the UI widgets, and the app shell. The core logic must stay testable without a GPU
and free of egui, typst, and platform APIs (invariant 10, *Core crates are pure*). Per-frame work
must stay O(visible) (invariant 6) and idle must be free (invariant 7). A native iPad app and a
whiteboard come later.

## Decision

- A cargo workspace: virtual manifest, `members = ["crates/*"]`, `resolver = "3"`. Crates are
  `buffer-*` under `crates/`:
  - `buffer-core`: vault, note identity, markdown + Obsidian extensions, frontmatter, index,
    search.
  - `buffer-text`: rope wrapper, transactions, selections, history, motions, text objects.
  - `buffer-modal`: modes, command registry, keymap engine, Vim + Helix presets.
  - `buffer-render`: render tree with source ranges, block + inline layout, paint.
  - `buffer-math`: typst `World`, compile + raster, texture cache.
  - `buffer-ui`: editor view, render view, pickers, command line, status line.
  - `buffer-app`: binary `buffer`; eframe setup, config, platform glue.
  - Later: `buffer-ink` (whiteboard) and an iPad shell.
- Dependencies point down: app → ui → {render, modal} → {text, core}; render → math. core, text,
  and modal never depend on egui or typst and are tested without a GPU.
- `[workspace.package]` shares version 0.1.0, edition 2024, `license = "MIT OR Apache-2.0"`, and
  `publish = false`. `[workspace.dependencies]` holds the internal path crates and third-party
  versions; every crate opts into `[workspace.lints]`.
- Threads: UI (egui frames); IO/index worker (scan, parse, watch, save); render workers (math,
  highlighting, image decode). Workers post results over channels
  ([0020](0020-no-async-runtime.md)).
- Frame: input → keymap dispatch → commands mutate state → lay out the visible range (cached) →
  paint.

## Consequences

- The workspace exists with the full crate layout and the inter-crate edges declared now,
  matching the graph above. `buffer-app` opens an empty eframe/wgpu window; the other crates are
  empty.
- `cargo deny` bans egui, eframe, and typst as direct dependencies outside `buffer-render`,
  `buffer-math`, `buffer-ui`, and `buffer-app` (plus egui's own crates); each of those joins the
  ban's wrappers list when it starts using one. Transitive paths and platform APIs in core crates
  are left to review.
- Long-running work (scan, parse, math, highlighting, image decode) leaves the UI thread, so the
  frame loop only lays out and paints the visible range from caches.
- New dependencies follow [0026](0026-dependency-policy.md).
- docs/architecture.md (M0) carries the crate map, dependency rules, threading, frame loop, and
  data flow.
