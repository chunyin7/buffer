# 0005. eframe feature set

- Status: Accepted
- Date: 2026-10-26

## Context

eframe's default features pull in more than buffer uses, including accesskit, whose Linux adapter
(`accesskit_unix`) requires zbus with async-io and async-executor, an async runtime. The dependency
policy ([0026](0026-dependency-policy.md)) requires `default-features = false` and bars async
runtimes without an explicit decision ([0020](0020-no-async-runtime.md)). On Linux, buffer must
build on a dev box without wayland-devel, start on X11-only systems, and show a title bar where
the compositor draws no decorations (GNOME).

## Decision

- eframe 0.37 with `default-features = false`, enabling `wgpu` ([0004](0004-wgpu-everywhere.md)),
  `default_fonts`, `wayland`, and `x11`.
- A direct `winit` 0.30 dependency, used only for its feature flags, adds:
  - `wayland-dlopen`: libwayland loads at runtime, so builds need no wayland-devel and the binary
    starts on X11-only systems.
  - `wayland-csd-adwaita`: client-side decorations draw a title bar where the compositor draws
    none.
- Left off until used: `clipboard` (M1), `links`, `system_fonts`, `persistence`.
  `web_screen_reader` is web-only; glow is not used.
- `accesskit` stays off until decided.
- Ubuntu-font-1.0 joins the allowed licenses globally, for `default_fonts`.

## Consequences

- Measured: release binary 14.0 MB stripped (vs 20.6 MB for an eframe hello world with default
  features); 166 third-party crates in the Linux build graph. The binary links only libc, libm,
  libgcc_s, and ld-linux. Verified: the window opens on Wayland (niri) with the Vulkan backend.
- CSD costs: it adds ab_glyph, and when CSD is active sctk-adwaita spawns fc-match, gsettings, and
  dbus-send. ab_glyph brings ttf-parser, which RustSec flags as unmaintained (RUSTSEC-2026-0192,
  no known vulnerability); `deny.toml` ignores it with a reason, since typst 0.15 depends on
  ttf-parser directly too.
- `default_fonts` pulls `epaint_default_fonts` (Ubuntu-Light, Hack, Noto Emoji, emoji-icon-font),
  licensed `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`. Bundled vs system fonts stays an
  open question for M3.
- Accessibility: custom widgets still expose AccessKit roles and names
  ([0003](0003-egui-eframe.md)), but platform screen-reader support needs a decision on
  accesskit's async runtime before the M4 accessibility pass.
- Enabling each deferred feature is a dependency change under [0026](0026-dependency-policy.md).

## Alternatives considered

- eframe defaults: would bring accesskit's async runtime plus unused features.
- `wayland-csd-adwaita-notitle`: no font dependencies, but the title bar has no text.
- dlopen without CSD: no title bar on GNOME.
- Linking libwayland at build time: needs wayland-devel.
- Dropping `default_fonts` until text lands.
- A license exception scoped to `epaint_default_fonts` instead of allowing Ubuntu-font-1.0
  globally.
