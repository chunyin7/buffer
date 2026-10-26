# 0003. egui/eframe with hand-rolled widgets

- Status: Accepted
- Date: 2026-10-26

## Context

buffer needs a Rust UI toolkit for Linux, macOS, and Windows now and iPad later
([0006](0006-platforms.md)). Its core surfaces are a modal Markdown editor and a typeset render
view, both keyboard-driven, with typst math and images placed inside lines. Lean and fast are
features: 0% CPU when idle.

## Decision

- egui/eframe 0.37.
- The editor, render view, pickers, command line, and status line are hand-rolled widgets. Stock
  egui widgets are used only for chrome and settings.

## Consequences

- We own text layout in the render view: egui's `LayoutJob` has text sections only, so the
  render view places math, images, and chips itself ([0015](0015-own-inline-layout.md)).
- Key dispatch runs before egui widgets see input (`App::raw_input_hook` / start of `logic`), and
  egui's Tab focus traversal and default clipboard shortcuts are disabled where they collide with
  keymaps ([0018](0018-vim-helix-presets.md)).
- egui redraws on demand; invariant 7 (idle is free) holds only if we never request continuous
  repaints: animations use bounded `request_repaint_after`, no cursor blink by default, and
  workers call `ctx.request_repaint()` when results land.
- Custom widgets must expose AccessKit roles and names; `egui_kittest` UI tests fail unnamed input
  widgets. The AccessKit platform feature itself is still undecided
  ([0005](0005-eframe-features.md)).
- egui must stay out of core, text, and modal (invariant 10, [0025](0025-crate-architecture.md)).
- Pen input for the whiteboard needs winit 0.31 events, through egui or our own winit
  integration ([0024](0024-whiteboard-pen-input.md)).
