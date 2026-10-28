# buffer

A keyboard-driven, tag-based Markdown notes app, written in Rust with egui. The design:

- A vault is a folder of plain `.md` files. Navigation is tags, search, and links; there is no
  folder tree.
- Vim and Helix keymaps ship together, and every action is a named command.
- Edit view: monospace Markdown source with modal editing. Render view: typeset, read-only,
  keyboard-navigable. Toggling keeps your place.
- Notes stay Obsidian-compatible, except that math is [typst](https://typst.app), not LaTeX.
- Lean: 0% CPU when idle, fast start, every dependency justified.
- Linux, macOS, and Windows first; iPad later.

## Status

Pre-alpha; nothing is usable yet. Milestone M0 (foundations) is in progress: the workspace,
lints, cargo-deny, CI, and the decision records exist, and `buffer` opens an empty window. M1
(editing) has started with the text core. See the [roadmap](docs/ROADMAP.md).

## Build and run

Needs the latest stable Rust.

```sh
cargo run --release
```

- Linux builds need no system libraries or -devel packages: libwayland, X11, and xkbcommon load
  at runtime.
- Running needs a GPU driver wgpu supports: Vulkan or OpenGL ES on Linux, Metal on macOS,
  DirectX 12 or Vulkan on Windows.

## Docs

- [CONTRIBUTING.md](CONTRIBUTING.md): toolchain, workflow, checks, adding dependencies.
- [AGENTS.md](AGENTS.md): decisions, invariants, and conventions at a glance.
- [docs/architecture.md](docs/architecture.md): crates, threads, frame loop, data flow.
- [docs/decisions/](docs/decisions/README.md): architecture decision records.
- [docs/ROADMAP.md](docs/ROADMAP.md): milestones and planned docs.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
buffer by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
