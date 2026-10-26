# 0021. File watching with notify

- Status: Accepted
- Date: 2026-10-26

## Context

A vault is a folder of plain `.md` files, and other editors and sync tools may change them at any
time ([0007](0007-files-are-the-truth.md)). Invariant 3: external changes are reconciled, never
clobbered.

## Decision

- Watch the vault with notify 8 (stable), debounced.
- The watcher runs on the IO/index worker thread, not an async runtime
  ([0020](0020-no-async-runtime.md)).
- Reconcile each change with the watcher plus (mtime, size, hash): a clean buffer reloads in
  place and keeps the cursor; a dirty buffer gets a conflict prompt.

## Consequences

- External edits never overwrite unsaved text silently, and saves never overwrite external edits
  silently.
- The watcher lands with vault open in M2, alongside the parallel scan, disposable index cache,
  and conflict prompt.
- docs/vault.md (M2) specifies external edits and conflicts.
