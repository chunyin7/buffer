# AGENTS.md

Guidance for anyone (human or agent) changing this repo. Until `docs/` fills in, this file is the
source of truth for scope, decisions, invariants, and conventions. Change a decision here in the
same commit that changes the behavior.

## Implementation Guide

As an agent when contributing to this project, keep contributions as atomic as possible unless asked and ALWAYS defer to the user regarding architectural decisions or finer implementation decisions.

Code should always prioritise human readability and understandability. We want to minimise unnecessary layers of abstraction whenever possible. For non-trivial code changes, always prefer going through an atomic human code review process.

## Status

- Pre-alpha: licenses, the workspace with the target crate layout, lints, cargo-deny, CI, and the
  README, CONTRIBUTING, architecture doc, and ADRs are in place; only `buffer-app` has code (it
  opens an empty window). Everything else below is decided, not implemented, unless marked
  otherwise.
- Current milestone: **M0 Foundations**. Milestones and planned docs: [docs/ROADMAP.md](docs/ROADMAP.md).
- M0 still to do: baselines with docs/performance.md, the typst spike, and a first green CI run
  on all three OSes.

## Product

buffer is a keyboard-driven, tag-based Markdown notes app written in Rust with egui. It targets
Linux, macOS, and Windows first, iPad later. Lean and fast are features.

1. **Plain files.** A vault is a folder of `.md` files. Other editors and sync tools may change
   them at any time.
2. **Tags first.** Navigation is tags, search, and links. Subfolders are indexed so existing vaults
   open as-is, but there is no folder tree and no folder management.
3. **Keyboard first.** Vim and Helix presets ship together. Every action is a named command; the
   mouse is optional.
4. **Two views.** *Edit*: monospace Markdown source with modal editing. *Render*: typeset,
   read-only, keyboard-navigable. Toggling keeps your place. Obsidian-style Live Preview comes
   later, and nothing may preclude it.
5. **Lean.** 0% CPU when idle, fast start, every dependency justified.

- v1 scope: CommonMark + GFM, Obsidian syntax (math excepted), tags, links and backlinks, typst
  math, highlighted code blocks, images, fuzzy and full-text search.
- Later: Live Preview, freehand whiteboard (stylus), native iPad app.
- Not planned (revisit explicitly): built-in sync, plugins/scripting, collaboration, web/wasm
  build, Android, export/publishing, encryption, any network I/O.

## Decisions

Summaries; each decision has an ADR in [docs/decisions/](docs/decisions/README.md) with its
context, consequences, and any rejected alternatives. Measurements cited here: 2026-10-26 on the
dev box (24 cores, rustc 1.98.1), stripped binaries.

### Platform and UI

- **Rust 2024, latest stable.** The dev box uses Fedora's packaged rustc (no rustup).
- **License: MIT OR Apache-2.0.** Constrains dependencies (see Dependency policy).
- **egui/eframe 0.37.** The editor, render view, pickers, command line, and status line are
  hand-rolled widgets. Stock egui widgets only for chrome and settings.
- **wgpu on every platform** (eframe's default since 0.34): one backend, Metal on macOS/iPad.
  Stay on epaint (no custom shaders) until a feature needs them. Size lever: trim wgpu backends
  per target via eframe's `wgpu_no_default_features`. Baseline: eframe 0.37 hello world is
  20.6 MB release, 12.1 MB with fat LTO, 273 crates.
- **eframe features:** `default-features = false`, plus `wgpu`, `default_fonts`, `wayland`, and
  `x11`. A direct `winit` dependency, used only for its feature flags, adds `wayland-dlopen` (no
  build-time libwayland; starts on X11-only systems) and `wayland-csd-adwaita` (a title bar where
  the compositor draws no decorations, e.g. GNOME). `clipboard` waits for M1. `accesskit` stays
  off until decided: its Linux adapter pulls in zbus + async-io, an async runtime.
- **Platforms:** Linux, macOS, Windows now; native iPad later. Core crates stay
  platform-agnostic, and file access goes through a `Vault` trait so the iOS sandbox
  (security-scoped bookmarks) fits later.

### Data

- **Vault = folder of `.md` files; files are the truth.** Any on-disk index is a disposable,
  versioned cache.
- **Obsidian-compatible format, except math** (see Markdown dialect).
- **Frontmatter: own span-preserving reader** for the Obsidian Properties subset (scalars and
  lists; `tags`, `aliases`, `cssclasses`). Programmatic edits patch byte spans and never
  re-serialize. YAML beyond the subset is kept verbatim and shown read-only. Not
  serde_yaml/serde_yml (unmaintained).
- **Config: TOML** (`toml` 0.8, already linked by typst) in the platform config dir. Keymap
  overrides live there.

### Markdown and rendering

- **pulldown-cmark 0.13.** Its offset iterator gives byte ranges. Built in: tables, task lists,
  strikethrough, footnotes, math, wikilinks, metadata blocks. Obsidian extras (`#tags`,
  `==highlight==`, `%%comments%%`, callout types/titles/folding, `^block-ids`, embeds) are our
  own pass over the event stream.
- **Math: typst syntax, rendered by typst 0.15 used as a library.** `$…$` and `$$…$$` contain
  typst math, not LaTeX. This is a deliberate break from Obsidian/GitHub (MathJax/KaTeX): math
  renders wrong across tools in both directions.
  - Accepted cost: a typst + typst-render test binary is 47.7 MB release / 36.0 MB fat LTO,
    including 9.7 MB of bundled fonts; 58 s clean build; 274 crates. typst has no cargo features
    to trim.
  - Speed is fine: 109 µs per uncached formula, 2.2 ms one-time init.
  - Our `World` exposes no files, packages, or network. Formulas render to cached textures off
    the UI thread. Call `comemo::evict` to bound typst's memo cache.
- **Code highlighting: syntect 5.3 with the bat syntax set typst already links**
  (`typst::text::RAW_SYNTAXES`, two-face 0.4). No second syntax dump; pure Rust (fancy-regex).
  Initialize lazily, off the UI thread.
- **Images:** `image` 0.25 (png, jpeg, gif, webp) and usvg, both already linked by typst. Decode
  off the UI thread; downscale to display size.
- **Inline layout is ours.** egui's `LayoutJob` has text sections only (no inline boxes), so the
  render view places math, images, and chips inside lines itself, using egui fonts (harfrust
  shaping) for text runs.

### Editing

- **Rope: `crop` 0.4.** Byte-indexed, with cheap copy-on-write clones for background snapshots.
  Wrapped by `buffer-text`, so a later switch (e.g. ropey 2 once stable) stays local.
- **Selection-based core, Helix/Kakoune style.** Every edit is a transaction (change set plus
  resulting selections). Selections map through changes. Undo history is a revision tree. Vim's
  operator + motion becomes "select, then act", so one core serves both presets.
- **Vim and Helix presets at launch.**
  - Keymaps are data (TOML) binding key sequences to named commands; user config overrides
    presets.
  - Key dispatch runs before egui widgets see input (`App::raw_input_hook` / start of `logic`).
    egui's Tab focus traversal and default clipboard shortcuts are disabled where they collide.
  - Character bindings use logical (layout-aware) keys. Insert mode supports IME.

### Search

- **Own markdown-aware in-memory index + `frizbee` 0.13** (MIT, SIMD, typo-tolerant fuzzy
  matching).
  - Indexed: file names, aliases, tags (nested), links/backlinks, headings, block ids, text.
  - Content search: literal via memchr/aho-corasick, regex mode (regex already linked by typst),
    frizbee as fuzzy fallback. Built in parallel with rayon (already linked by typst).
- fff ([dmtrKovalenko/fff](https://github.com/dmtrKovalenko/fff)) is the design reference, not a
  dependency: `fff-search` hard-requires libgit2 + LMDB C builds, its mmap cache can SIGBUS when
  sync tools truncate files, and it knows nothing about tags or links.

### Infrastructure

- **No async runtime.** std threads + channels; rayon for data-parallel work. Workers call
  `ctx.request_repaint()` when results land.
- **File watching: notify 8** (stable), debounced.
- **Logging/profiling:** `log` facade; `profiling` scopes with a puffin backend behind a cargo
  feature (the same facades egui uses).
- **Errors:** `thiserror` in library crates. Math/highlight/image workers run under
  `catch_unwind`, and release builds keep `panic = "unwind"`, so a renderer bug can't take down
  unsaved text.
- **Whiteboard (later):** pen pressure/tilt needs winit 0.31 (beta: Wayland, Windows, Web, iOS
  Apple Pencil); egui 0.37 still uses winit 0.30. Ink input must carry pressure, tilt, and
  timestamps from the start.

### Considered and rejected

- LaTeX math via RaTeX (KaTeX-compatible; +4.5–5.9 MB, 15 µs per formula): typst syntax preferred.
- LaTeX → typst via MiTeX: full typst cost plus a stale converter crate (0.2.4, Jun 2024).
- `fff-search`: see Search.
- tree-sitter highlighting: C toolchain on every target; binary grows per grammar.
- SQLite storage or index: files are the truth; an in-memory index is enough at target scale.
- glow renderer: wgpu chosen to keep one backend.

## Architecture (target; detail in [docs/architecture.md](docs/architecture.md))

```
crates/
  buffer-core    vault, note identity, markdown + Obsidian extensions, frontmatter, index, search
  buffer-text    rope wrapper, transactions, selections, history, motions, text objects
  buffer-modal   modes, command registry, keymap engine, Vim + Helix presets
  buffer-render  render tree with source ranges, block + inline layout, paint
  buffer-math    typst World, compile + raster, texture cache
  buffer-ui      editor view, render view, pickers, command line, status line
  buffer-app     binary `buffer`: eframe setup, config, platform glue
  (later)        buffer-ink (whiteboard), iPad shell
```

- Dependencies point down: app → ui → {render, modal} → {text, core}; render → math. core, text,
  and modal never depend on egui or typst and are tested without a GPU.
- Threads: UI (egui frames); IO/index worker (scan, parse, watch, save); render workers (math,
  highlighting, image decode). Workers post results over channels.
- Frame: input → keymap dispatch → commands mutate state → lay out the visible range (cached) →
  paint.

## Invariants

1. **Never lose or rewrite user bytes.** Saves are atomic (temp file in the same dir, fsync,
   rename). Open + save without edits is byte-identical: line endings, BOM, final newline,
   frontmatter formatting. Programmatic edits (tags, link rewrites on rename) are minimal span
   patches.
2. **The index is a cache.** Deleting it loses nothing; a version mismatch means rebuild.
3. **External changes are reconciled, never clobbered.** Watcher plus (mtime, size, hash): a clean
   buffer reloads in place and keeps the cursor; a dirty buffer gets a conflict prompt.
4. **Positions are UTF-8 byte offsets.** Convert to chars/graphemes/UTF-16 only at layout, IME,
   and platform boundaries. Cursor motion respects grapheme clusters.
5. **Render nodes carry source byte ranges.** The Edit ↔ Render toggle maps position through them;
   Live Preview will build on them.
6. **Per-frame work is O(visible).** Never O(document) or O(vault) on the UI thread. Layout is
   virtualized and cached by (content hash, width, style, pixels-per-point).
7. **Idle is free.** No continuous repaint; animations use bounded `request_repaint_after`. No
   cursor blink by default.
8. **Everything is a command:** named, in the palette, bindable. Mouse actions invoke the same
   commands.
9. **Notes are untrusted input.** Parsers never panic on any input (fuzzed); typst runs sandboxed
   and bounded; no network I/O.
10. **Core crates are pure.** No egui, typst, or platform APIs in core, text, or modal.
    `deny.toml` bans direct egui, eframe, and typst dependencies outside render, math, ui, and
    app; transitive paths are left to review.

## Markdown dialect (summary; full spec in docs/markdown-dialect.md, M2)

- CommonMark; GFM tables, task lists, strikethrough, autolinks, footnotes.
- Obsidian:
  - Links: `[[Note]]`, `[[Note|alias]]`, `[[Note#Heading]]`, `[[Note#^block]]`.
  - Embeds: `![[…]]`, with `|width` / `|WxH` sizing; images first, note transclusion later.
  - Tags: `#tag`, `#nested/tag` (letters, digits, `_`, `-`, `/`; at least one non-digit; never
    inside code). Frontmatter `tags`, `aliases`, `cssclasses`.
  - Callouts `> [!type]` with optional `+`/`-` fold marker and title; `==highlight==`;
    `%%comment%%`; `^block-id`.
- Math: `$inline$` and `$$display$$` hold **typst** math.
- Links resolve by file name or vault-relative path, as in Obsidian; ambiguity handling is
  specified in docs/vault.md. Aliases feed the switcher and link completion (which inserts
  `[[Note|alias]]`); they don't resolve bare links. Renaming a note rewrites inbound links after a
  preview.
- New notes are named after their title (`Title.md`, sanitized).

## Performance budgets

Targets until M0 records baselines in docs/performance.md.

| Metric | Target |
|---|---|
| Idle CPU | 0%, no repaints |
| Keystroke → frame (our CPU, 1 MB note, edit view) | ≤ 4 ms p99 |
| Edit ↔ Render toggle (100 KB note, 50 formulas) | ≤ 50 ms |
| Vault open, 10k notes, warm cache | interactive ≤ 300 ms; index complete ≤ 1 s |
| Fuzzy switcher, 10k notes | ≤ 5 ms per keystroke |
| Full-text search, 50 MB of notes | ≤ 50 ms |
| RSS with a 10k-note vault | ≤ 200 MB |
| Binary size | tracked per milestone; growth > 1 MB needs a stated reason |

## Dependency policy

- Every new crate needs a stated reason in the change description: purpose, why not hand-roll,
  size delta (`cargo bloat`), transitive crates (`cargo tree`), license, maintenance status.
- First check whether egui or typst already link it (`cargo tree -i <crate>`), and reuse that
  major version. No duplicate versions (`cargo tree -d`); unavoidable upstream ones get a
  reasoned `skip` in `deny.toml`.
- `default-features = false`; enable only what's used. Versions live in
  `[workspace.dependencies]`.
- Not allowed without an explicit decision here: async runtimes, C/C++ build dependencies beyond
  what egui/wgpu need, network crates (banned in `deny.toml`).
- Allowed licenses: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0,
  CC0-1.0, BSL-1.0, MPL-2.0, OFL-1.1 and Ubuntu-font-1.0 (fonts; egui's `default_fonts` bundles
  Ubuntu-Light). Not allowed: GPL, LGPL, AGPL, SSPL, non-commercial.
- `cargo deny check` enforces licenses, RustSec advisories, duplicates, bans, and crates.io-only
  sources. Each ignored advisory and skipped duplicate in `deny.toml` carries a reason.

## Working in this repo

Toolchain setup, CI, and the dependency policy step by step: [CONTRIBUTING.md](CONTRIBUTING.md).

- VCS is **jj**, colocated with git: `jj st`, `jj diff`, `jj describe -m`, `jj new`. Use git only
  for what jj can't do.
- Before finishing a change: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `cargo deny check`.
- Tests:
  - Unit and property tests for core/text/modal: transactions compose and invert, selection
    mapping, byte-identical round-trips.
  - Keymap tests are tables of key sequence → buffer + selection, run for both presets.
  - Fuzz targets for parsers.
  - UI via `egui_kittest`. Custom widgets must expose AccessKit roles and names; kittest fails
    unnamed input widgets.
  - Benches for parse, index, search, and layout, checked against the budgets.
  - Don't test wiring or incidental defaults.
- Performance-sensitive changes include before/after numbers.
- Code: crates are `buffer-*` under `crates/`. `unsafe_code = "deny"` workspace-wide; exceptions
  only in platform glue, with `// SAFETY:` comments. No `unwrap`/`expect` outside tests except
  for documented invariants. No `dbg!`, `todo!`, `unimplemented!`, or print macros (output goes
  through `log`). Workspace lints in the root `Cargo.toml` enforce all of this; an exception is
  `#[expect(lint, reason = "…")]`.
- Docs land with the behavior they describe. A new or changed decision gets an ADR in
  `docs/decisions/` (append-only: supersede, don't rewrite) and its summary here, in the same
  change.
