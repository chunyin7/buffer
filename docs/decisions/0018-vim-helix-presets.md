# 0018. Vim and Helix presets, keymaps as data

- Status: Accepted
- Date: 2026-10-26

## Context

buffer is keyboard first: Vim and Helix presets ship together, and the mouse is optional. Every
action is a named command, in the palette and bindable (invariant "Everything is a command").
egui widgets consume some keys themselves (Tab focus traversal, clipboard shortcuts), and the
editor is a hand-rolled widget ([0003](0003-egui-eframe.md)). Both presets run on one
selection-based core ([0017](0017-selection-based-core.md)).

## Decision

- Vim and Helix presets at launch.
- Keymaps are data: TOML binding key sequences to named commands. User config overrides presets
  ([0010](0010-toml-config.md)).
- Key dispatch runs before egui widgets see input (`App::raw_input_hook` / start of `logic`).
  egui's Tab focus traversal and default clipboard shortcuts are disabled where they collide.
- Character bindings use logical (layout-aware) keys. Insert mode supports IME.

## Consequences

- Presets and user overrides share one format and one keymap engine in `buffer-modal`; adding a
  binding needs no code, only a named command.
- `buffer-modal` is a core crate: no egui ([0025](0025-crate-architecture.md)), so the egui-side
  dispatch hook lives above it.
- Keymap tests are tables of key sequence → buffer + selection, run for both presets. M1 exit
  requires both presets to pass them.
- docs/editor/keymaps.md (M1) covers the command registry, keymap format, and Vim and Helix
  coverage matrices; docs/config.md (M1) covers keymap overrides.
