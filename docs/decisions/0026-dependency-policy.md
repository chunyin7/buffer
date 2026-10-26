# 0026. Dependency policy

- Status: Accepted
- Date: 2026-10-26

## Context

Lean is a product principle: 0% CPU when idle, fast start, every dependency justified. Binary
size is tracked per milestone, and growth > 1 MB needs a stated reason. egui and typst already
link many crates that other features can reuse ([0012](0012-typst-math.md)). The license is
MIT OR Apache-2.0 ([0002](0002-dual-license.md)), which constrains what dependencies may carry.
buffer does no network I/O.

## Decision

- Every new crate needs a stated reason in the change description: purpose, why not hand-roll,
  size delta (`cargo bloat`), transitive crates (`cargo tree`), license, maintenance status.
- First check whether egui or typst already link it (`cargo tree -i <crate>`), and reuse that
  major version. No duplicate versions (`cargo tree -d`).
- `default-features = false`; enable only what's used. Versions live in
  `[workspace.dependencies]`.
- Not allowed without an explicit decision: async runtimes, C/C++ build dependencies beyond what
  egui/wgpu need, network crates.
- Allowed licenses: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, CC0-1.0,
  BSL-1.0, MPL-2.0, OFL-1.1, and Ubuntu-font-1.0 (fonts). Not allowed: GPL, LGPL, AGPL, SSPL,
  non-commercial.
- `cargo deny` enforces from M0:
  - the license allow list;
  - RustSec advisories, each ignore with a reason (today RUSTSEC-2026-0192, ttf-parser
    unmaintained);
  - duplicate versions denied, with a skip entry and reason for each existing upstream duplicate
    (today all inside eframe's tree);
  - sources restricted to crates.io;
  - bans on network crates: reqwest, hyper, ureq, curl, openssl, native-tls, rustls;
  - layer purity: egui, eframe, and typst banned as direct dependencies outside `buffer-render`,
    `buffer-math`, `buffer-ui`, `buffer-app` (plus egui's own crates); transitive paths are left
    to review ([0025](0025-crate-architecture.md)).
- Async runtimes and C/C++ build dependencies are not encoded as bans; review enforces them.

## Consequences

- Ubuntu-font-1.0 is allowed globally because eframe's `default_fonts`
  ([0005](0005-eframe-features.md)) pulls `epaint_default_fonts` (Ubuntu-Light, Hack, Noto Emoji,
  emoji-icon-font), licensed `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`. Whether fonts
  are bundled or system stays an open question for M3.
- The policy already shapes decisions: `accesskit` stays off because its Linux adapter needs an
  async runtime ([0005](0005-eframe-features.md), [0020](0020-no-async-runtime.md)); toml,
  syntect's syntax set, image, usvg, regex, and rayon reuse what typst links.
- Every skip entry for an upstream duplicate needs a reason, and new duplicates fail the check.
- CONTRIBUTING.md (M0) explains how to apply the policy; docs/performance.md (M0) records the
  binary-size baselines that growth is measured against.

## Alternatives considered

- Drop `default_fonts` until text lands, instead of allowing Ubuntu-font-1.0.
- A crate-scoped license exception for `epaint_default_fonts` only, instead of a global allow.
