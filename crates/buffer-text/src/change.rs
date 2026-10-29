use std::ops::Range;

use crate::Text;

/// One step of a change set. The steps walk the text before the change from start to end.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Operation {
    /// Keep the next `n` bytes.
    Retain(usize),
    /// Remove the next `n` bytes.
    Delete(usize),
    /// Insert text at the current position.
    Insert(String),
}

/// Where a mapped position goes relative to text inserted at it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assoc {
    /// Stay before the inserted text.
    Before,
    /// Move past the inserted text.
    After,
}

/// An edit to a whole text: from start to end, which bytes to keep, delete, and insert.
///
/// A [`Document`](crate::Document) builds one from its own text for each edit;
/// [`Document::edit_with`](crate::Document::edit_with) hands it to the caller to map positions
/// through.
///
/// A change set is kept in canonical form, so equal edits compare equal: no operation is empty,
/// adjacent operations differ in kind, and at one position an insert comes before a delete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSet {
    operations: Vec<Operation>,
    len_before: usize,
    len_after: usize,
}

/// Why [`Document::edit`](crate::Document::edit) or
/// [`Document::edit_with`](crate::Document::edit_with) rejected an edit.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InvalidChange {
    #[error("change {range:?} ends before it starts")]
    Reversed { range: Range<usize> },
    #[error("change {range:?} starts before the previous change ends at {previous_end}")]
    OutOfOrder {
        range: Range<usize>,
        previous_end: usize,
    },
    #[error("change {range:?} runs past the end of the text at {len}")]
    PastEnd { range: Range<usize>, len: usize },
    #[error("change {range:?} splits a char")]
    SplitsChar { range: Range<usize> },
}

impl ChangeSet {
    /// A change set that replaces each range of `text` with its string: an empty range inserts,
    /// an empty string deletes. The ranges must be sorted and must not overlap; they may touch,
    /// and several empty ranges may share an offset.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidChange`] for the first range that is reversed, starts before the previous
    /// one ends, runs past the end of `text`, or splits a char. Spans computed from an older
    /// snapshot of the text can do any of that, so the caller recomputes them.
    pub(crate) fn new(
        text: &Text,
        changes: impl IntoIterator<Item = (Range<usize>, String)>,
    ) -> Result<Self, InvalidChange> {
        let changes = changes.into_iter();
        let mut set = Self::with_capacity(2 * changes.size_hint().0 + 1);
        let mut position = 0;
        for (range, insert) in changes {
            // In this order, `is_char_boundary` only sees offsets inside the text.
            if range.start > range.end {
                return Err(InvalidChange::Reversed { range });
            }
            if range.start < position {
                return Err(InvalidChange::OutOfOrder {
                    range,
                    previous_end: position,
                });
            }
            if range.end > text.len() {
                return Err(InvalidChange::PastEnd {
                    range,
                    len: text.len(),
                });
            }
            if !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
                return Err(InvalidChange::SplitsChar { range });
            }
            set.retain(range.start - position);
            set.insert(insert);
            set.delete(range.end - range.start);
            position = range.end;
        }
        set.retain(text.len() - position);
        Ok(set)
    }

    /// The length of the text before the change.
    pub fn len_before(&self) -> usize {
        self.len_before
    }

    /// The length of the text after the change.
    pub fn len_after(&self) -> usize {
        self.len_after
    }

    /// Whether the change leaves the text as it is.
    pub fn is_identity(&self) -> bool {
        self.operations
            .iter()
            .all(|operation| matches!(operation, Operation::Retain(_)))
    }

    /// Applies the change to `text`.
    ///
    /// # Panics
    ///
    /// Panics if `text` doesn't have the length this change set was made for.
    pub(crate) fn apply(&self, text: &mut Text) {
        assert_eq!(
            text.len(),
            self.len_before,
            "change set applied to a text of another length",
        );
        let mut position = 0;
        let mut operations = self.operations.iter().peekable();
        while let Some(operation) = operations.next() {
            match operation {
                Operation::Retain(n) => position += n,
                Operation::Delete(n) => text.replace(position..position + n, ""),
                Operation::Insert(insert) => {
                    // An insert and the delete after it are one rope edit.
                    let deleted =
                        match operations.next_if(|next| matches!(next, Operation::Delete(_))) {
                            Some(Operation::Delete(n)) => *n,
                            _ => 0,
                        };
                    text.replace(position..position + deleted, insert);
                    position += insert.len();
                }
            }
        }
    }

    /// The change set that undoes this one: applied to the changed text, it restores `original`.
    ///
    /// # Panics
    ///
    /// Panics if `original` doesn't have the length this change set was made for.
    pub(crate) fn invert(&self, original: &Text) -> Self {
        assert_eq!(
            original.len(),
            self.len_before,
            "change set inverted against a text of another length",
        );
        let mut inverse = Self::with_capacity(self.operations.len());
        let mut position = 0;
        for operation in &self.operations {
            match operation {
                Operation::Retain(n) => {
                    inverse.retain(*n);
                    position += n;
                }
                Operation::Delete(n) => {
                    inverse.insert(original.to_string_in(position..position + n));
                    position += n;
                }
                Operation::Insert(text) => inverse.delete(text.len()),
            }
        }
        inverse
    }

    /// Combines this change set with `next`, made for the text this one produces, into one
    /// change set with the effect of both.
    ///
    /// # Panics
    ///
    /// Panics if `next` was made for a text of another length than this change set produces.
    pub(crate) fn compose(self, next: Self) -> Self {
        assert_eq!(
            self.len_after, next.len_before,
            "composed change sets don't line up",
        );
        let mut composed = Self::with_capacity(self.operations.len() + next.operations.len());
        let mut first = self.operations.into_iter();
        let mut second = next.operations.into_iter();
        // Walk both lists in step: the first's output (retains and inserts) lines up with the
        // second's input (retains and deletes). A partly used operation comes back shortened.
        let mut a = first.next();
        let mut b = second.next();
        loop {
            (a, b) = match (a, b) {
                (None, None) => break,
                // Bytes the first change deletes never reach the second.
                (Some(Operation::Delete(n)), b) => {
                    composed.delete(n);
                    (first.next(), b)
                }
                // Text the second change inserts takes nothing from the first.
                (a, Some(Operation::Insert(text))) => {
                    composed.insert(text);
                    (a, second.next())
                }
                (Some(Operation::Retain(i)), Some(Operation::Retain(j))) => {
                    let n = i.min(j);
                    composed.retain(n);
                    (
                        rest(Operation::Retain, i - n, &mut first),
                        rest(Operation::Retain, j - n, &mut second),
                    )
                }
                (Some(Operation::Retain(i)), Some(Operation::Delete(j))) => {
                    let n = i.min(j);
                    composed.delete(n);
                    (
                        rest(Operation::Retain, i - n, &mut first),
                        rest(Operation::Delete, j - n, &mut second),
                    )
                }
                (Some(Operation::Insert(mut text)), Some(Operation::Retain(j))) => {
                    if text.len() <= j {
                        let n = text.len();
                        composed.insert(text);
                        (first.next(), rest(Operation::Retain, j - n, &mut second))
                    } else {
                        let tail = text.split_off(j);
                        composed.insert(text);
                        (Some(Operation::Insert(tail)), second.next())
                    }
                }
                // The second change deletes text the first inserted: neither survives.
                (Some(Operation::Insert(mut text)), Some(Operation::Delete(j))) => {
                    if text.len() <= j {
                        let n = text.len();
                        (first.next(), rest(Operation::Delete, j - n, &mut second))
                    } else {
                        text.replace_range(..j, "");
                        (Some(Operation::Insert(text)), second.next())
                    }
                }
                (None, Some(_)) | (Some(_), None) => {
                    unreachable!("the lengths match, so both lists end together")
                }
            };
        }
        composed
    }

    /// Maps `position` in the text before the change to the text after it.
    ///
    /// With [`Assoc::Before`], a position stays before text inserted at it; with
    /// [`Assoc::After`], it moves past that text. A position inside deleted bytes moves to where
    /// they were. If new text replaces them, a position inside them moves to the start of the new
    /// text with `Before` and past it with `After`, and the position at their start moves to the
    /// start of the new text with either.
    ///
    /// # Panics
    ///
    /// Panics if `position` is past the end of the text before the change.
    pub fn map(&self, position: usize, assoc: Assoc) -> usize {
        Mapper::new(self).map(position, assoc)
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            operations: Vec::with_capacity(capacity),
            len_before: 0,
            len_after: 0,
        }
    }

    fn retain(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        self.len_before += n;
        self.len_after += n;
        if let Some(Operation::Retain(count)) = self.operations.last_mut() {
            *count += n;
        } else {
            self.operations.push(Operation::Retain(n));
        }
    }

    fn delete(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        self.len_before += n;
        if let Some(Operation::Delete(count)) = self.operations.last_mut() {
            *count += n;
        } else {
            self.operations.push(Operation::Delete(n));
        }
    }

    fn insert(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.len_after += text.len();
        let last = match self.operations.as_mut_slice() {
            // Text inserted at one position is one insert, also across a delete there.
            [.., Operation::Insert(previous)]
            | [.., Operation::Insert(previous), Operation::Delete(_)] => {
                previous.push_str(&text);
                return;
            }
            // An insert goes before a delete at the same position.
            [.., last @ Operation::Delete(_)] => std::mem::replace(last, Operation::Insert(text)),
            _ => Operation::Insert(text),
        };
        self.operations.push(last);
    }
}

/// The rest of a partly used operation, or the next operation once it is used up.
fn rest(
    operation: fn(usize) -> Operation,
    n: usize,
    operations: &mut impl Iterator<Item = Operation>,
) -> Option<Operation> {
    if n > 0 {
        Some(operation(n))
    } else {
        operations.next()
    }
}

/// Maps positions through a change set in one forward pass, so mapping sorted positions costs
/// O(positions + operations). Positions must come in non-decreasing order.
pub(crate) struct Mapper<'a> {
    operations: &'a [Operation],
    /// The next operation to look at.
    index: usize,
    /// Where that operation starts in the text before the change.
    old: usize,
    /// Where that operation starts in the text after the change.
    new: usize,
}

impl<'a> Mapper<'a> {
    pub(crate) fn new(changes: &'a ChangeSet) -> Self {
        Self {
            operations: &changes.operations,
            index: 0,
            old: 0,
            new: 0,
        }
    }

    /// Maps `position` as [`ChangeSet::map`] does.
    ///
    /// # Panics
    ///
    /// Panics if `position` is past the end of the text before the change.
    pub(crate) fn map(&mut self, position: usize, assoc: Assoc) -> usize {
        debug_assert!(position >= self.old, "positions must be mapped in order");
        // Pass operations that end before `position` and stop at the one holding it. An insert at
        // `position` stays current, so the same position can be mapped again with another assoc.
        while let Some(operation) = self.operations.get(self.index) {
            match operation {
                Operation::Retain(n) => {
                    if position < self.old + n {
                        return self.new + (position - self.old);
                    }
                    self.old += n;
                    self.new += n;
                    self.index += 1;
                }
                Operation::Delete(n) => {
                    if position < self.old + n {
                        return self.new;
                    }
                    self.old += n;
                    self.index += 1;
                }
                Operation::Insert(text) => {
                    let inserted = text.len();
                    // In canonical form, a delete right after an insert holds the replaced bytes.
                    let deleted = match self.operations.get(self.index + 1) {
                        Some(Operation::Delete(n)) => *n,
                        _ => 0,
                    };
                    if position == self.old {
                        let past = assoc == Assoc::After && deleted == 0;
                        return if past { self.new + inserted } else { self.new };
                    }
                    if position < self.old + deleted {
                        return match assoc {
                            Assoc::Before => self.new,
                            Assoc::After => self.new + inserted,
                        };
                    }
                    self.old += deleted;
                    self.new += inserted;
                    self.index += if deleted == 0 { 1 } else { 2 };
                }
            }
        }
        assert_eq!(
            position, self.old,
            "position is past the end of the text before the change",
        );
        self.new
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::strategies::{self, Change};

    fn change_set(text: &Text, changes: &[Change]) -> ChangeSet {
        ChangeSet::new(text, changes.iter().cloned()).expect("generated changes are valid")
    }

    fn applied(text: &Text, changes: &ChangeSet) -> Text {
        let mut text = text.clone();
        changes.apply(&mut text);
        text
    }

    fn assert_canonical(set: &ChangeSet) {
        for operation in &set.operations {
            let empty = match operation {
                Operation::Retain(n) | Operation::Delete(n) => *n == 0,
                Operation::Insert(text) => text.is_empty(),
            };
            assert!(!empty, "empty operation in {set:?}");
        }
        for pair in set.operations.windows(2) {
            let merged_or_ordered = !matches!(
                pair,
                [Operation::Retain(_), Operation::Retain(_)]
                    | [Operation::Delete(_), Operation::Delete(_)]
                    | [Operation::Insert(_), Operation::Insert(_)]
                    | [Operation::Delete(_), Operation::Insert(_)]
            );
            assert!(merged_or_ordered, "not canonical: {set:?}");
        }
    }

    #[test]
    fn new_rejects_invalid_changes() {
        // "abé": 'é' is bytes 2..4.
        let text = Text::from("abé");
        let reject = |changes: Vec<Change>| ChangeSet::new(&text, changes).unwrap_err();
        let reversed = Range { start: 2, end: 1 };
        assert_eq!(
            reject(vec![(reversed.clone(), String::new())]),
            InvalidChange::Reversed { range: reversed }
        );
        assert_eq!(
            reject(vec![(1..2, String::new()), (0..1, String::new())]),
            InvalidChange::OutOfOrder {
                range: 0..1,
                previous_end: 2
            }
        );
        // Past the end wins over a split char, and nothing panics on the out-of-bounds offset.
        assert_eq!(
            reject(vec![(3..5, String::new())]),
            InvalidChange::PastEnd {
                range: 3..5,
                len: 4
            }
        );
        assert_eq!(
            reject(vec![(0..3, String::new())]),
            InvalidChange::SplitsChar { range: 0..3 }
        );
    }

    #[test]
    fn map_at_an_insert_follows_assoc() {
        let set = ChangeSet::new(&Text::from("abcd"), [(2..2, "XY".to_owned())]).unwrap();
        assert_eq!(set.map(2, Assoc::Before), 2);
        assert_eq!(set.map(2, Assoc::After), 4);
    }

    #[test]
    fn map_inside_a_delete_goes_where_it_was() {
        // "abcdef" -> "aef"
        let set = ChangeSet::new(&Text::from("abcdef"), [(1..4, String::new())]).unwrap();
        for position in 1..4 {
            assert_eq!(set.map(position, Assoc::Before), 1);
            assert_eq!(set.map(position, Assoc::After), 1);
        }
        assert_eq!(set.map(4, Assoc::Before), 1);
    }

    #[test]
    fn map_in_a_replacement() {
        // "abcdef" -> "aXYef"
        let set = ChangeSet::new(&Text::from("abcdef"), [(1..4, "XY".to_owned())]).unwrap();
        assert_eq!(set.map(1, Assoc::Before), 1);
        assert_eq!(set.map(1, Assoc::After), 1);
        assert_eq!(set.map(2, Assoc::Before), 1);
        assert_eq!(set.map(2, Assoc::After), 3);
        assert_eq!(set.map(4, Assoc::Before), 3);
    }

    proptest! {
        #[test]
        fn apply_matches_model((text, changes) in strategies::text_and_changes()) {
            let original = Text::from(text.as_str());
            let set = change_set(&original, &changes);
            let expected = strategies::apply(&text, &changes);
            prop_assert_eq!(applied(&original, &set).to_string(), expected.as_str());
            prop_assert_eq!(set.len_before(), text.len());
            prop_assert_eq!(set.len_after(), expected.len());
            assert_canonical(&set);
        }

        #[test]
        fn invert_restores_original((text, changes) in strategies::text_and_changes()) {
            let original = Text::from(text.as_str());
            let set = change_set(&original, &changes);
            let inverse = set.invert(&original);
            prop_assert_eq!(applied(&applied(&original, &set), &inverse), original);
            assert_canonical(&inverse);
        }

        #[test]
        fn compose_equals_applying_in_turn(
            (text, first, second) in strategies::text_and_two_changes(),
        ) {
            let original = Text::from(text);
            let a = change_set(&original, &first);
            let middle = applied(&original, &a);
            let b = change_set(&middle, &second);
            let expected = applied(&middle, &b);
            let composed = a.compose(b);
            prop_assert_eq!(applied(&original, &composed), expected);
            assert_canonical(&composed);
        }

        #[test]
        fn map_keeps_untouched_chars((text, changes) in strategies::text_and_changes()) {
            let set = change_set(&Text::from(text.as_str()), &changes);
            let changed = strategies::apply(&text, &changes);
            for (offset, char) in text.char_indices() {
                if changes.iter().any(|(range, _)| range.contains(&offset)) {
                    continue;
                }
                let start = set.map(offset, Assoc::After);
                let end = set.map(offset + char.len_utf8(), Assoc::Before);
                let mut buffer = [0; 4];
                let expected: &str = char.encode_utf8(&mut buffer);
                prop_assert_eq!(changed.get(start..end), Some(expected));
            }
        }

        #[test]
        fn map_is_monotonic((text, changes) in strategies::text_and_changes()) {
            let set = change_set(&Text::from(text.as_str()), &changes);
            let mut previous = (0, 0);
            for offset in strategies::char_boundaries(&text) {
                let before = set.map(offset, Assoc::Before);
                let after = set.map(offset, Assoc::After);
                prop_assert!(before <= after && after <= set.len_after());
                prop_assert!(previous.0 <= before && previous.1 <= after);
                previous = (before, after);
            }
        }
    }
}
