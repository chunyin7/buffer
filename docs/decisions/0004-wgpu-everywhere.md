# 0004. wgpu on every platform

- Status: Accepted
- Date: 2026-10-26

## Context

eframe can render through wgpu or glow; wgpu is its default since 0.34. buffer targets Linux,
macOS, Windows, and later iPad ([0006](0006-platforms.md)). Baseline: an eframe 0.37 hello world
is 20.6 MB release, 12.1 MB with fat LTO, 273 crates.

## Decision

- wgpu is the renderer on every platform: one backend, Metal on macOS and iPad.
- We stay on epaint, with no custom shaders, until a feature needs them.

## Consequences

- One rendering path to test and debug across targets.
- wgpu is large. The size lever is trimming wgpu backends per target via eframe's
  `wgpu_no_default_features`. The M0 baselines measure binary size with default vs trimmed
  backends and record them in docs/performance.md.
- Writing custom shaders later is a new decision, not a default.
- The eframe feature set that selects wgpu is in [0005](0005-eframe-features.md).

## Alternatives considered

- glow renderer: rejected to keep one backend.
