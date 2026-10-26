# 0012. Typst math, rendered by typst as a library

- Status: Accepted
- Date: 2026-10-26

## Context

Notes hold inline (`$…$`) and display (`$$…$$`) math. Obsidian and GitHub read it as LaTeX and
render it with MathJax or KaTeX. buffer renders math in-process, off the UI thread, as textures
that its own inline layout places inside lines ([0015](0015-own-inline-layout.md)). Notes are
untrusted input, and buffer does no network I/O.

## Decision

- `$…$` and `$$…$$` contain typst math, not LaTeX.
- typst 0.15, used as a library, renders it. Our `World` exposes no files, packages, or network.
- Formulas render to cached textures off the UI thread. We call `comemo::evict` to bound typst's
  memo cache.

## Consequences

- A deliberate break from Obsidian and GitHub: math renders wrong across tools in both
  directions. docs/markdown-dialect.md (M2) and docs/math.md (M3, with a typst primer for LaTeX
  users) cover the divergence; a migration aid for imported vaults is an open question for M4.
- Accepted cost: a typst + typst-render test binary is 47.7 MB release / 36.0 MB fat LTO,
  including 9.7 MB of bundled fonts; 58 s clean build; 274 crates. typst has no cargo features to
  trim. The M0 spike measures the real delta inside the app.
- Speed is fine: 109 µs per uncached formula, 2.2 ms one-time init.
- Other decisions reuse what typst already links: toml ([0010](0010-toml-config.md)), the bat
  syntax set ([0013](0013-syntect-highlighting.md)), image and usvg ([0014](0014-images.md)),
  regex and rayon ([0019](0019-search-index.md)).
- typst runs sandboxed and bounded, like every parser of note content.

## Alternatives considered

- LaTeX math via RaTeX (KaTeX-compatible): +4.5–5.9 MB, 15 µs per formula. Rejected: typst
  syntax preferred.
- LaTeX converted to typst via MiTeX: the full typst cost plus a stale converter crate (0.2.4,
  June 2024).
