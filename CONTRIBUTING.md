# Contributing

buffer is pre-alpha. [AGENTS.md](AGENTS.md) is the short index of scope, decisions, invariants,
and conventions; [docs/decisions/](docs/decisions/README.md) explains each decision. This file
covers the toolchain, the workflow, and how to apply the rules.

## Toolchain

- Latest stable Rust, edition 2024. A distribution's rustc works when it is current. On Fedora:
  `sudo dnf install rust cargo rustfmt clippy`. Elsewhere: rustup's stable toolchain with the
  `rustfmt` and `clippy` components.
- cargo-deny: `cargo install --locked cargo-deny`; distribution packages can lag the version CI
  runs.
- Linux builds need no system libraries or -devel packages: winit loads libwayland, X11, and
  xkbcommon at runtime.

## Workflow

- VCS is [jj](https://github.com/jj-vcs/jj), colocated with git. Use git only for what jj can't
  do.
- Keep changes atomic: one logical step per change (`jj new`, edit, `jj describe -m "…"`).
- A change description says what changed and why. A new dependency carries its justification
  (see below); performance-sensitive changes include before/after numbers.
- Docs land in the same change as the behavior they describe. A new or changed decision gets an
  ADR in `docs/decisions/` (template in its [README](docs/decisions/README.md)) and its summary in
  AGENTS.md. ADRs are append-only: supersede, don't rewrite.

## Checks

Run these before finishing a change:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

CI runs them on every push to `main` and on pull requests: rustfmt and cargo-deny on Linux,
clippy and tests on Linux, macOS, and Windows, all with `--locked`.

## Code

- Workspace lints live in the root `Cargo.toml`, and every crate opts in with
  `[lints] workspace = true`. Clippy lints warn, and `-D warnings` fails the check.
- No `unwrap`/`expect` outside tests except for documented invariants; no `dbg!`, `todo!`,
  `unimplemented!`, or print macros (output goes through `log`).
- An exception names its reason: `#[expect(clippy::expect_used, reason = "…")]`. Prefer
  `#[expect]` to `#[allow]`: it warns once the exception is no longer needed.
- `unsafe` is denied. Exceptions belong in platform glue only, with `// SAFETY:` comments.
- rustfmt pins `style_edition = "2024"` and otherwise uses the defaults.
- The invariants in AGENTS.md (byte-identical saves, byte offsets, O(visible) frames, idle is
  free, …) bind every change.

## Tests

- Unit and property tests for core, text, and modal: transactions compose and invert, selections
  map through changes, round-trips are byte-identical.
- Keymap tests are tables of key sequence → buffer + selection, run for both presets.
- Fuzz targets for parsers.
- UI tests use `egui_kittest`. Custom widgets expose AccessKit roles and names; kittest fails
  unnamed input widgets.
- Benches for parse, index, search, and layout are checked against the budgets in AGENTS.md.
- Don't test wiring or incidental defaults.

## Adding a dependency

The [dependency policy](docs/decisions/0026-dependency-policy.md) in practice:

1. Check whether egui or typst already link the crate (`cargo tree -i <crate>`); if so, reuse
   that major version.
2. Add the version to `[workspace.dependencies]` with `default-features = false`. In the member
   crate, write `<crate>.workspace = true` and enable only the features it uses.
3. Measure the stripped release binary before and after (`cargo build --release`, then `strip`).
   For a per-crate breakdown, use `cargo bloat --release --crates`
   (`cargo install --locked cargo-bloat`). Growth over 1 MB needs a stated reason.
4. Count new transitive crates with `cargo tree -e normal,build`, and check `cargo tree -d` for
   duplicates.
5. Run `cargo deny check`. An unavoidable upstream duplicate gets a `skip` entry in `deny.toml`
   whose reason names the crate that pulls it in. Async runtimes, C/C++ build dependencies beyond
   what egui/wgpu need, and network crates need an explicit decision first (an ADR).
6. Write the justification into the change description:

```text
New dependency: <crate> <version>, features <…>
- Purpose:
- Why not hand-roll:
- Size delta: <before> → <after> (stripped release)
- Transitive crates: +<n>
- License:
- Maintenance:
```

## License

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
buffer by you, as defined in the Apache-2.0 license, shall be dual licensed as MIT OR Apache-2.0
([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)), without any additional terms or
conditions.
