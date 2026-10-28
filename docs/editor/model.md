# Editing model

How `buffer-text` holds a note's text and changes it. The decisions behind it:
[0016](../decisions/0016-crop-rope.md) (crop rope) and
[0017](../decisions/0017-selection-based-core.md) (selection-based core).

Status: text and change sets exist. Selections, transactions, history, lines, motions, text
objects, and registers land later in M1.

## Text

- `Text` holds UTF-8 in a `crop` rope. No other crate depends on `crop`, so a later switch stays
  inside `buffer-text`.
- Clones are cheap and copy-on-write: a background worker takes a snapshot by cloning.
- A `Text` changes only through `ChangeSet::apply`, so every edit can be recorded and inverted.

## Positions

- Positions are UTF-8 byte offsets from 0 to the text's length, inclusive. Conversion to chars,
  graphemes, or UTF-16 happens only at layout, IME, and platform boundaries.
- An edit starts and ends on char boundaries; `ChangeSet::new` panics otherwise, since a position
  inside a char is a bug in the caller.

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
  overlapping: an empty range inserts, an empty replacement deletes.
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

## Tests

Property tests (proptest) in `buffer-text` check change sets against a plain `String` model:
`apply` matches it, `invert` restores the original, `compose` equals applying both in turn,
mapping keeps untouched chars in place and is monotonic, and every result is canonical.
Generated texts mix multi-byte chars, combining marks, emoji sequences, and line breaks, and some
span several rope chunks.
