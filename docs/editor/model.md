# Editing model

How `buffer-text` holds a note's text and changes it. The decisions behind it:
[0016](../decisions/0016-crop-rope.md) (crop rope),
[0017](../decisions/0017-selection-based-core.md) (selection-based core),
[0027](../decisions/0027-helix-selection-ranges.md) (Helix-style ranges),
[0029](../decisions/0029-undo-insert-sessions.md) (undo steps), and
[0031](../decisions/0031-document-owns-history.md) (documents and history).

Status: text, change sets, selections, transactions, and documents with undo history exist.
Lines, motions, text objects, and registers land later in M1.

## Text

- `Text` holds UTF-8 in a `crop` rope. No other crate depends on `crop`, so a later switch stays
  inside `buffer-text`.
- Clones are cheap and copy-on-write: a background worker takes a snapshot by cloning.
- A `Text` changes only through `ChangeSet::apply`, which is crate-private: outside
  `buffer-text`, only a `Document` changes a text, so every edit lands in its history.

## Positions

- Positions are UTF-8 byte offsets from 0 to the text's length, inclusive. Conversion to chars,
  graphemes, or UTF-16 happens only at layout, IME, and platform boundaries.
- An edit starts and ends on char boundaries; `ChangeSet::new` rejects one that doesn't.

## Change sets

A `ChangeSet` is an edit to a whole text: operations that walk the text from start to end.

| Operation | Effect |
|---|---|
| `Retain(n)` | keep the next `n` bytes |
| `Delete(n)` | remove the next `n` bytes |
| `Insert(s)` | insert `s` at the current position |

- Retains and deletes add up to the length before the change; retains and inserts, to the length
  after it.
- `ChangeSet::new(text, changes)` builds one from `(range, replacement)` pairs, sorted and not
  overlapping: an empty range inserts, an empty replacement deletes. It returns `InvalidChange`
  for the first range that is reversed, starts before the previous one ends, runs past the end,
  or splits a char.
- Canonical form: no operation is empty, adjacent operations differ in kind, and at one position
  an insert comes before a delete. Equal edits compare equal, and a delete right after an insert
  is always the bytes that insert replaces.
- `apply` (crate-private) edits a `Text`, one rope edit per insert, delete, or insert-and-delete
  pair.
- `invert(original)` returns the change set that undoes it: retains stay, deletes become inserts
  of the original bytes, and inserts become deletes.
- `a.compose(b)`, where `b` was made for the text `a` produces, returns one change set with the
  effect of both. It walks both lists in step, lining up `a`'s output (retains and inserts) with
  `b`'s input (retains and deletes).

### Mapping positions

`map(position, assoc)` moves a position in the text before a change to the text after it.

| Position | `Assoc::Before` | `Assoc::After` |
|---|---|---|
| in kept bytes | moves with them | moves with them |
| where text is inserted | before the new text | after the new text |
| at the start of or inside deleted bytes | where they were | where they were |
| at the start of replaced bytes | start of the new text | start of the new text |
| inside replaced bytes | start of the new text | end of the new text |

Mapping is monotonic: for one `assoc`, a later position never maps before an earlier one.

## Selections

- A `Range` is two byte offsets, `anchor` and `head`, covering the half-open span from `start()`
  (the smaller) to `end()` (the larger). Extending a range moves its head.
- A `Selection` is one or more ranges, one of them primary. `Selection::new(primary, others)`
  sorts them and merges a range that starts where another starts, or inside it: overlapping
  ranges merge, and so does an empty range inside another. Touching ranges, and an empty range at
  another's end, stay apart. A merged range points backward only if both ranges did, and it is
  primary if either was. Taking the primary range itself means a selection can't be empty or have
  a primary index past its ranges.
- Ranges sit in a `SmallVec` with room for one, so a single-range selection doesn't allocate.
- The block cursor, and the one-grapheme minimum in Normal mode, need grapheme boundaries; they
  land with the motions.

### Mapping selections

`Selection::map(changes)` maps every range, then merges ranges that now overlap.

- A range's start maps with `Assoc::After` and its end with `Assoc::Before`, so text inserted at
  either edge stays outside it, and a range that contains replaced bytes covers the new text.
- An empty range maps with `Assoc::After`: typing at a cursor moves it past the typed text.
- A range that reaches into replaced bytes from outside keeps only its part outside them.
- A range strictly inside replaced bytes maps past its own end; its ends swap, so it covers the
  new text, keeping its direction.
- At an edge of replaced bytes, both ends land on one point: a range from their start to inside
  them collapses to the start of the new text, and one from inside them to their end collapses to
  the end of the new text. Deleting a range's text collapses it too, so a caller that needs
  non-empty ranges (Normal mode) widens them after mapping.
- Sorted, non-overlapping ranges give positions in order, so the mapping is one pass over the
  change set and the ranges together.

## Transactions

A `Transaction` is one edit: a change set and the selection after it.

- `apply(text, selection)` (crate-private) applies the change and sets the selection; a document
  applies its edits, undos, and redos this way.
- `invert(original, selection_before)` returns the transaction that undoes it, restoring both the
  text and the selection before.
- `compose(next)` combines two transactions into one with both changes and `next`'s selection;
  a document composes the edits of one revision this way.

## Documents and history

A `Document` owns a text, its selection, and its undo history
([0031](../decisions/0031-document-owns-history.md)). Every edit and every move through history
goes through it: a change set fits only the text it was made for, and applying it checks no more
than the length, so a history kept apart from its text could undo the wrong bytes.
`ChangeSet::apply` and `Transaction::apply` are crate-private, so code outside `buffer-text` can't
edit a text any other way.

- `apply(transaction, now)` applies an edit and adds it to the pending revision, composed with the
  edits before it. `set_selection` changes only the selection.
- `commit()` ends the pending revision. The caller commits where
  [0029](../decisions/0029-undo-insert-sessions.md) ends an undo step: on leaving Insert mode, on
  the checkpoint command, on a cursor move that isn't typing, and after each Normal-mode command.
- An edit that comes a second or more after the previous one commits the pending revision first.
  The caller passes each edit's time, so `buffer-text` reads no clock and tests set the time.
- A pending revision keeps a clone of the text and selection from before its first edit; text
  clones share structure, so this is cheap. On commit, the composed edits and their inverse become
  a revision. Edits that cancel out, like typing a char and deleting it, leave no revision.
- Undo, redo, and jumps commit first.

### Revisions

Revisions form a tree. The root is the text the document started with. Every other revision keeps
the transaction that made it from its parent, the one that undoes it, and the time of its last
edit. An edit after an undo starts a new branch.

| Method | Moves to |
|---|---|
| `undo()` | the parent, restoring the selection from before the revision |
| `redo()` | the child made or visited last, restoring the selection after it |
| `earlier(n)`, `later(n)` | `n` revisions back or forward in the order they were made, across branches (Vim `g-`/`g+`, Helix `Alt-u`/`Alt-U`) |
| `earlier_by(d)` | the newest revision made at least `d` before the current one, or the root (`:earlier 10m`) |
| `later_by(d)` | the newest revision made at most `d` after the current one (`:later 10m`) |

- Each returns whether it moved: `undo` at the root and `redo` without a child return false.
- Revisions sit in a `Vec` in the order they were made, so a parent comes before its children. A
  jump undoes up to the newest common ancestor, found by stepping the later of the two revisions
  to its parent, then points redo down to the target and redoes along it. Redo after any move
  retraces the way back, as in Vim; Helix's redo goes to the newest child instead.
- As long as the caller's times don't go backwards, revisions are made in time order, and a time
  jump finds its target by binary search.

## Errors and panics

Per [0030](../decisions/0030-panics-and-results.md), input that can be stale gets an error, and a
broken contract only our own bug can cause panics before anything changes.

| Check | On failure |
|---|---|
| `ChangeSet::new`: a range reversed, out of order, past the end, or splitting a char | `Err(InvalidChange)`; spans can be stale, so the caller recomputes |
| `apply`, `invert`, `compose`: a text or change set of another length | panic |
| `ChangeSet::map`: a position past the end of the text | panic |
| `Transaction::new`: a selection past the end of the changed text | panic |
| `Document::apply`: a transaction made for a text of another length | panic, before the pending revision changes |
| `Document::set_selection`: a selection past the end of the text | panic |
| Internal: the primary range survives merging, `compose` ends both lists together, and redo never leads to the root | panic (`expect`, `unreachable!`) |
| Internal: the mapper gets positions in order | `debug_assert!`; only `Selection::map` uses it, with sorted positions |

## Tests

Property tests (proptest) in `buffer-text` check change sets against a plain `String` model:
`apply` matches it, `invert` restores the original, `compose` equals applying both in turn,
mapping keeps untouched chars in place and is monotonic, and every result is canonical.
Generated texts mix multi-byte chars, combining marks, emoji sequences, and line breaks, and some
span several rope chunks. For selections, they check that merging covers exactly the given
ranges, that mapping in one pass equals mapping each range alone, and that ranges no change
touches keep their text.

For documents, a property test runs random edits (with pauses around a second), commits, undos,
redos, and jumps by count and by time. After each, the text must be exactly the text of the
revision the history is at, pending edits must start from it, and a jump by count must land
where it should; at the end, the oldest and the newest revision must be reachable. Unit tests pin
the rules: the pause boundary, commit points, edits that cancel out, restored selections, redo
after a jump into another branch, and the limits of time jumps.
