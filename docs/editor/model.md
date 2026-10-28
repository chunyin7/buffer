# Editing model

How `buffer-text` holds a note's text and changes it. The decisions behind it:
[0016](../decisions/0016-crop-rope.md) (crop rope),
[0017](../decisions/0017-selection-based-core.md) (selection-based core), and
[0027](../decisions/0027-helix-selection-ranges.md) (Helix-style ranges).

Status: text, change sets, selections, and transactions exist. History, lines, motions, text
objects, and registers land later in M1.

## Text

- `Text` holds UTF-8 in a `crop` rope. No other crate depends on `crop`, so a later switch stays
  inside `buffer-text`.
- Clones are cheap and copy-on-write: a background worker takes a snapshot by cloning.
- A `Text` changes only through `ChangeSet::apply`, so every edit can be recorded and inverted.

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
- `apply` edits a `Text`, one rope edit per insert, delete, or insert-and-delete pair.
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
  sorts them and merges ranges that share a byte or start at the same offset; touching ranges,
  and an empty range at another's end, stay apart. A merged range points backward only if both
  ranges did, and it is primary if either was. Taking the primary range itself means a selection
  can't be empty or have a primary index past its ranges.
- Ranges sit in a `SmallVec` with room for one, so a single-range selection doesn't allocate.
- The block cursor, and the one-grapheme minimum in Normal mode, need grapheme boundaries; they
  land with the motions.

### Mapping selections

`Selection::map(changes)` maps every range, then merges ranges that now overlap.

- A range's start maps with `Assoc::After` and its end with `Assoc::Before`, so text inserted at
  either edge stays outside it, and a replaced range covers the new text.
- An empty range maps with `Assoc::After`: typing at a cursor moves it past the typed text.
- A range inside replaced bytes covers the new text, keeping its direction.
- Sorted, non-overlapping ranges give positions in order, so the mapping is one pass over the
  change set and the ranges together.

## Transactions

A `Transaction` is one edit: a change set and the selection after it.

- `apply(text, selection)` applies the change and sets the selection.
- `invert(original, selection_before)` returns the transaction that undoes it, restoring both the
  text and the selection before.
- `compose(next)` combines two transactions into one with both changes and `next`'s selection;
  Insert-mode sessions will compose this way into a single revision
  ([0029](../decisions/0029-undo-insert-sessions.md)).

## Errors and panics

Per [0030](../decisions/0030-panics-and-results.md), input that can be stale gets an error, and a
broken contract only our own bug can cause panics before anything changes.

| Check | On failure |
|---|---|
| `ChangeSet::new`: a range reversed, out of order, past the end, or splitting a char | `Err(InvalidChange)`; spans can be stale, so the caller recomputes |
| `apply`, `invert`, `compose`: a text or change set of another length | panic |
| `ChangeSet::map`: a position past the end of the text | panic |
| `Transaction::new`: a selection past the end of the changed text | panic |
| Internal: the primary range survives merging, and `compose` ends both lists together | panic (`expect`, `unreachable!`) |
| Internal: the mapper gets positions in order | `debug_assert!`; only `Selection::map` uses it, with sorted positions |

## Tests

Property tests (proptest) in `buffer-text` check change sets against a plain `String` model:
`apply` matches it, `invert` restores the original, `compose` equals applying both in turn,
mapping keeps untouched chars in place and is monotonic, and every result is canonical.
Generated texts mix multi-byte chars, combining marks, emoji sequences, and line breaks, and some
span several rope chunks. For selections, they check that merging covers exactly the given
ranges, that mapping in one pass equals mapping each range alone, and that ranges no change
touches keep their text.
