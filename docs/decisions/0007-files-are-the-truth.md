# 0007. Files are the truth; the index is a cache

- Status: Accepted
- Date: 2026-10-26

## Context

A vault is a folder of `.md` files, and other editors and sync tools may change them at any time.
Users bring existing vaults, often with subfolders. Navigation is by tags, search, and links, which
needs an index over the whole vault ([0019](0019-search-index.md)). The target scale is a
10k-note vault.

## Decision

- A vault is a folder of `.md` files, and those files are the only source of truth.
- Subfolders are indexed so existing vaults open as-is. There is no folder tree and no folder
  management.
- Any on-disk index is a disposable, versioned cache: deleting it loses nothing, and a version
  mismatch means rebuild.

## Consequences

- Invariants "Never lose or rewrite user bytes", "The index is a cache", and "External changes
  are reconciled, never clobbered" follow from this: saves are atomic, open + save without edits
  is byte-identical, and external edits reload or prompt rather than being overwritten.
- The cache is never migrated or repaired: a version mismatch means rebuild from files. The open
  budget (interactive ≤ 300 ms, index complete ≤ 1 s for 10k notes, warm cache) has to hold
  with that design.
- File access goes through a `Vault` trait, so the iOS sandbox fits later
  ([0006](0006-platforms.md)).
- docs/vault.md (M2) carries vault layout, note identity, external edits and conflicts, atomic
  saves, and the index cache.

## Alternatives considered

- SQLite storage or index. Rejected: files are the truth, and an in-memory index is enough at
  target scale.
