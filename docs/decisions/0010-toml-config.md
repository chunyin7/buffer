# 0010. TOML config in the platform config dir

- Status: Accepted
- Date: 2026-10-26

## Context

buffer needs user configuration, including keymap overrides: keymaps are data (TOML) binding key
sequences to named commands, and user config overrides the Vim and Helix presets
([0018](0018-vim-helix-presets.md)). The dependency policy says to first reuse what egui or typst
already link ([0026](0026-dependency-policy.md)).

## Decision

- Config is TOML, parsed with `toml` 0.8, which typst already links
  ([0012](0012-typst-math.md)).
- It lives in the platform config dir.
- Keymap overrides live there.

## Consequences

- Keymaps and config share one format and one parser.
- The `toml` major version follows typst's, per the dependency policy.
- Config loading lives in `buffer-app` ([0025](0025-crate-architecture.md)).
- docs/config.md (M1) covers location, options, and keymap overrides.
