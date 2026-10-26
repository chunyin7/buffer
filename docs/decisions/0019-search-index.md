# 0019. Own in-memory search index with frizbee

- Status: Accepted
- Date: 2026-10-26

## Context

Navigation is tags, search, and links, with no folder tree. Search must understand Markdown and
Obsidian structure: aliases, nested tags, links and backlinks, headings, block ids. Budgets: fuzzy
switcher ≤ 5 ms per keystroke on 10k notes; full-text search ≤ 50 ms over 50 MB of notes. Files
are the truth and any index is a disposable cache ([0007](0007-files-are-the-truth.md)); per-frame
work on the UI thread is O(visible). Sync tools may change or truncate files at any time.

## Decision

- Own markdown-aware in-memory index in `buffer-core`, plus `frizbee` 0.13 (MIT, SIMD,
  typo-tolerant fuzzy matching).
- Indexed: file names, aliases, tags (nested), links/backlinks, headings, block ids, text.
- Content search: literal via memchr/aho-corasick, regex mode, frizbee as fuzzy fallback.
- The index is built in parallel with rayon.
- fff ([dmtrKovalenko/fff](https://github.com/dmtrKovalenko/fff)) is the design reference, not a
  dependency.

## Consequences

- regex and rayon are already linked by typst ([0012](0012-typst-math.md)), so they add no new
  crates.
- The index is a cache (invariant "The index is a cache"): deleting it loses nothing; a version
  mismatch means rebuild. Indexing and queries run off the UI thread, keeping per-frame work
  O(visible).
- Benches for index and search are checked against the budgets; M2 exit requires a synthetic
  10k-note vault to meet them.
- docs/search.md (M2) covers query syntax and index design.

## Alternatives considered

- `fff-search`: hard-requires libgit2 + LMDB C builds, its mmap cache can SIGBUS when sync tools
  truncate files, and it knows nothing about tags or links.
- SQLite index: files are the truth; an in-memory index is enough at target scale.
