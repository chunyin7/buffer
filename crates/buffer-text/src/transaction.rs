use crate::{ChangeSet, Selection, Text};

/// An edit: a change set and the selection after it. A [`Document`](crate::Document) makes one
/// for each edit, and its history keeps them to undo and redo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Transaction {
    changes: ChangeSet,
    selection: Selection,
}

impl Transaction {
    /// # Panics
    ///
    /// Panics if a range of `selection` ends past the text `changes` produces.
    pub(crate) fn new(changes: ChangeSet, selection: Selection) -> Self {
        let len = changes.len_after();
        assert!(
            selection.ranges().iter().all(|range| range.end() <= len),
            "selection ends past the changed text ({len} bytes)",
        );
        Self { changes, selection }
    }

    pub(crate) fn changes(&self) -> &ChangeSet {
        &self.changes
    }

    /// Applies the change to `text` and sets `selection` to the one after it.
    ///
    /// # Panics
    ///
    /// Panics if `text` doesn't have the length the change set was made for.
    pub(crate) fn apply(&self, text: &mut Text, selection: &mut Selection) {
        self.changes.apply(text);
        selection.clone_from(&self.selection);
    }

    /// The transaction that undoes this one: its change restores `original`, and its selection is
    /// `selection_before`, the one this transaction replaced.
    ///
    /// # Panics
    ///
    /// Panics if `original` doesn't have the length the change set was made for, or if
    /// `selection_before` ends past it.
    pub(crate) fn invert(&self, original: &Text, selection_before: Selection) -> Self {
        Self::new(self.changes.invert(original), selection_before)
    }

    /// Combines this transaction with `next`, made for the text this one produces: one change
    /// with the effect of both, and `next`'s selection.
    ///
    /// # Panics
    ///
    /// Panics if `next` was made for a text of another length than this transaction produces.
    pub(crate) fn compose(self, next: Self) -> Self {
        Self {
            changes: self.changes.compose(next.changes),
            selection: next.selection,
        }
    }
}
