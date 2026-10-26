# 0022. log and profiling facades

- Status: Accepted
- Date: 2026-10-26

## Context

buffer needs diagnostics and profiling across crates, including the performance budgets and
before/after numbers for performance-sensitive changes. egui already instruments itself through
the `log` and `profiling` facades.

## Decision

- Logging goes through the `log` facade.
- Profiling uses `profiling` scopes, with a puffin backend behind a cargo feature.
- These are the same facades egui uses.

## Consequences

- Our scopes and egui's appear in the same logs and profiles.
- Library crates depend only on the facades; the profiling backend compiles in only when the
  cargo feature is enabled.
- docs/performance.md (M0) covers how to measure.
