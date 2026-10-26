# 0020. No async runtime

- Status: Accepted
- Date: 2026-10-26

## Context

buffer does local work only: scanning, parsing, watching, and saving files, plus rendering math,
highlighting code, and decoding images. It does no network I/O. The UI thread runs egui frames,
and idle must be free (Invariant 7: no continuous repaint). The dependency policy bars async
runtimes without an explicit decision here.

## Decision

- No async runtime. Concurrency uses std threads and channels; rayon handles data-parallel work.
- Threads: the UI thread (egui frames); an IO/index worker (scan, parse, watch, save); render
  workers (math, highlighting, image decode). Workers post results over channels.
- Workers call `ctx.request_repaint()` when results land, so the UI repaints only when there is
  something new.

## Consequences

- Idle stays free: nothing wakes the UI thread except input and finished work.
- rayon is already linked by typst ([0012](0012-typst-math.md)), and search builds its index with
  it ([0019](0019-search-index.md)).
- Any crate that brings an async runtime needs its own decision first. eframe's `accesskit`
  feature stays off for this reason: its Linux adapter (accesskit_unix) requires zbus with
  async-io and async-executor ([0005](0005-eframe-features.md)). M4 plans an accessibility pass
  (AccessKit), which has to resolve this.
- cargo-deny does not encode async runtimes as bans; review enforces this decision
  ([0026](0026-dependency-policy.md)).
- docs/architecture.md (M0) carries the threading model.
