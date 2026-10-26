# 0006. Desktop first, iPad later

- Status: Accepted
- Date: 2026-10-26

## Context

buffer targets Linux, macOS, and Windows first and a native iPad app later. The iPad sandbox
gives file access through Files and security-scoped bookmarks, not arbitrary paths. A web/wasm
build and Android are not planned.

## Decision

- Linux, macOS, and Windows now; native iPad later (M7).
- Core crates stay platform-agnostic (invariant 10: no egui, typst, or platform APIs in core,
  text, or modal).
- File access goes through a `Vault` trait so the iOS sandbox (security-scoped bookmarks) fits
  later.

## Consequences

- M0 CI runs on Linux, macOS, and Windows; M4 packages all three.
- Platform code lives in `buffer-app` ([0025](0025-crate-architecture.md)); `unsafe` exceptions
  are allowed only there, with `// SAFETY:` comments.
- M7 adds the eframe iOS build (wgpu/Metal, [0004](0004-wgpu-everywhere.md)), Files and
  security-scoped bookmark vault access, touch and hardware keyboard, on-screen keyboard and IME,
  Apple Pencil for the whiteboard, and save on suspend. docs/platforms/ipados.md (M7) carries the
  detail.
- The whiteboard (M6) depends on pen input from winit 0.31, which also covers iOS Apple Pencil
  ([0024](0024-whiteboard-pen-input.md)).
- Web/wasm and Android need an explicit revisit before any work.
