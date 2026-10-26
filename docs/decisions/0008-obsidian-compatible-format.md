# 0008. Obsidian-compatible format, except math

- Status: Accepted
- Date: 2026-10-26

## Context

Vaults are plain Markdown files that other editors and sync tools also open. Existing Obsidian
vaults should open as-is, and notes written in buffer should read correctly elsewhere. v1 scope
is CommonMark + GFM, Obsidian syntax (math excepted), tags, links and backlinks, typst math,
highlighted code blocks, and images.

## Decision

- Syntax: CommonMark; GFM tables, task lists, strikethrough, autolinks, footnotes.
- Obsidian syntax:
  - Links `[[Note]]`, `[[Note|alias]]`, `[[Note#Heading]]`, `[[Note#^block]]`.
  - Embeds `![[…]]` with `|width` / `|WxH` sizing; images first, note transclusion later.
  - Tags `#tag`, `#nested/tag` (letters, digits, `_`, `-`, `/`; at least one non-digit; never
    inside code); frontmatter `tags`, `aliases`, `cssclasses` ([0009](0009-frontmatter-reader.md)).
  - Callouts `> [!type]` with optional `+`/`-` fold marker and title; `==highlight==`;
    `%%comment%%`; `^block-id`.
- Links resolve by file name or vault-relative path, as in Obsidian. Aliases feed the switcher
  and link completion (which inserts `[[Note|alias]]`); they don't resolve bare links.
- Renaming a note rewrites inbound links after a preview. New notes are named `Title.md`,
  sanitized.
- The one exception: `$inline$` and `$$display$$` hold typst math, not LaTeX
  ([0012](0012-typst-math.md)).

## Consequences

- pulldown-cmark covers the CommonMark/GFM base; the Obsidian extras are our own pass over its
  event stream ([0011](0011-pulldown-cmark.md)).
- Math renders wrong across tools in both directions; a migration aid for imported vaults is an
  open question for M4.
- Inbound-link rewrites on rename are minimal span patches ("Never lose or rewrite user bytes").
- The M2 exit criterion includes an Obsidian sample vault that opens and saves with no byte
  changes.
- docs/markdown-dialect.md (M2) holds the full spec and the Obsidian compatibility matrix;
  docs/vault.md (M2) specifies link ambiguity handling.
- Open questions: duplicate note names, resolution order and UI (M2); where pasted images go
  (vault root, `attachments/`, or Obsidian's setting; M2); note transclusion depth limit and
  editing semantics (M3).
