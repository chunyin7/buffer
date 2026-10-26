# 0001. Rust 2024 on the latest stable toolchain

- Status: Accepted
- Date: 2026-10-26

## Context

buffer is written in Rust and is pre-alpha. The dev box uses Fedora's packaged rustc, no rustup.

## Decision

- All crates use edition 2024, set once in `[workspace.package]`.
- We build with the latest stable Rust toolchain.

## Consequences

- Contributors need a current stable toolchain; a distribution's rustc works when it is current,
  as on the dev box.
- Measurements in AGENTS.md and docs/performance.md (M0) name the rustc they were taken with.
- CONTRIBUTING.md (M0) documents the toolchain.
