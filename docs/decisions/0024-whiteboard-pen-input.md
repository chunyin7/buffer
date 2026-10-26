# 0024. Whiteboard pen input needs winit 0.31

- Status: Accepted
- Date: 2026-10-26

## Context

A freehand whiteboard with stylus input comes later (M6), with Apple Pencil on iPad (M7). Pen
pressure and tilt need winit 0.31, still in beta, which supports them on Wayland, Windows, Web,
and iOS (Apple Pencil). egui 0.37 still uses winit 0.30 ([0003](0003-egui-eframe.md),
[0005](0005-eframe-features.md)).

## Decision

- Pen input waits for winit 0.31.
- Ink input carries pressure, tilt, and timestamps from the start.

## Consequences

- M6 has a prerequisite: pen events, either from winit 0.31 reaching egui or from our own winit
  integration.
- The ink data model never needs a format change to add pressure, tilt, or timing.
- Nothing before M6 depends on this; the whiteboard lives in a later `buffer-ink` crate.
- The whiteboard file format (taggable and linkable; sidecar vs embedded in `.md`) is an open
  question for M6 and gets its own ADR. docs/whiteboard.md (M6) carries the pen input pipeline,
  and docs/platforms/ipados.md (M7) covers Pencil input.
