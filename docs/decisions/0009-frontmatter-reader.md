# 0009. Own span-preserving frontmatter reader

- Status: Accepted
- Date: 2026-10-26

## Context

Notes carry YAML frontmatter, which Obsidian exposes as Properties. buffer reads `tags`,
`aliases`, and `cssclasses` from it ([0008](0008-obsidian-compatible-format.md)) and edits it
programmatically, e.g. tags. Invariant "Never lose or rewrite user bytes" requires open + save
without edits to be byte-identical, frontmatter formatting included, and programmatic edits to be
minimal span patches. Invariant "Notes are untrusted input" requires parsers that never panic on
any input.

## Decision

- buffer-core has its own frontmatter reader for the Obsidian Properties subset: scalars and
  lists; `tags`, `aliases`, `cssclasses`.
- Programmatic edits patch byte spans and never re-serialize.
- YAML beyond the subset is kept verbatim and shown read-only.

## Consequences

- Formatting, comments, and unsupported YAML survive edits untouched.
- Properties outside the subset can't be edited in buffer.
- We own a parser: it gets a fuzz target like the other parsers, plus byte-identical round-trip
  tests.

## Alternatives considered

- serde_yaml / serde_yml. Rejected: unmaintained.
