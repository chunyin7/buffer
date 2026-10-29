use std::time::{Duration, Instant};

use crate::transaction::Transaction;
use crate::{Selection, Text};

/// The revisions of a text, as a tree.
///
/// The root is the text the history starts from. Every other revision is one undo step: it keeps
/// the transaction that made it from its parent and the one that takes it back. An edit after an
/// undo starts a new branch, and the old one stays reachable through the order revisions were
/// made in ([`earlier`](Self::earlier), [`later`](Self::later)).
///
/// The history keeps no text. Moving through it applies transactions to the text and selection it
/// is given, which must be the ones it recorded; [`Document`](crate::Document) keeps them together.
#[derive(Debug)]
pub(crate) struct History {
    /// In the order they were made: the root first, and every parent before its children.
    revisions: Vec<Revision>,
    /// The revision the text is at.
    current: usize,
}

#[derive(Debug)]
struct Revision {
    /// The revision this one was made from. The root is its own parent.
    parent: usize,
    /// The child redo moves to: the one made or visited last.
    redo_child: Option<usize>,
    /// The transactions between the parent and this revision; the root has none.
    edit: Option<Edit>,
    /// When the text got to this revision.
    time: Instant,
}

/// The transaction that makes a revision from its parent, and the one that takes it back.
#[derive(Debug)]
struct Edit {
    redo: Transaction,
    undo: Transaction,
}

impl History {
    /// A history whose root is the text as it was at `time`.
    pub(crate) fn new(time: Instant) -> Self {
        Self {
            revisions: vec![Revision {
                parent: 0,
                redo_child: None,
                edit: None,
                time,
            }],
            current: 0,
        }
    }

    /// Adds a revision after the current one, made at `time` by `redo` and undone by `undo`, and
    /// moves to it.
    pub(crate) fn push(&mut self, redo: Transaction, undo: Transaction, time: Instant) {
        let revision = self.revisions.len();
        self.revisions.push(Revision {
            parent: self.current,
            redo_child: None,
            edit: Some(Edit { redo, undo }),
            time,
        });
        self.revisions[self.current].redo_child = Some(revision);
        self.current = revision;
    }

    /// Moves to the parent revision, undoing the current one. Returns false at the root.
    pub(crate) fn undo(&mut self, text: &mut Text, selection: &mut Selection) -> bool {
        let revision = &self.revisions[self.current];
        let Some(edit) = &revision.edit else {
            return false;
        };
        edit.undo.apply(text, selection);
        let child = self.current;
        self.current = revision.parent;
        // Redo comes back here.
        self.revisions[self.current].redo_child = Some(child);
        true
    }

    /// Moves to the child revision made or visited last, redoing it. Returns false if there is
    /// none.
    pub(crate) fn redo(&mut self, text: &mut Text, selection: &mut Selection) -> bool {
        let Some(child) = self.revisions[self.current].redo_child else {
            return false;
        };
        let Some(edit) = &self.revisions[child].edit else {
            unreachable!("only the root has no edit, and it is no revision's child");
        };
        edit.redo.apply(text, selection);
        self.current = child;
        true
    }

    /// Moves `steps` revisions back in the order they were made, across branches, stopping at the
    /// root. Returns whether it moved.
    pub(crate) fn earlier(
        &mut self,
        steps: usize,
        text: &mut Text,
        selection: &mut Selection,
    ) -> bool {
        self.jump(self.current.saturating_sub(steps), text, selection)
    }

    /// Moves `steps` revisions forward in the order they were made, across branches, stopping at
    /// the newest. Returns whether it moved.
    pub(crate) fn later(
        &mut self,
        steps: usize,
        text: &mut Text,
        selection: &mut Selection,
    ) -> bool {
        let newest = self.revisions.len() - 1;
        let target = self.current.saturating_add(steps).min(newest);
        self.jump(target, text, selection)
    }

    /// Moves to the newest revision made at least `period` before the current one, or to the
    /// root if there is none. Returns whether it moved.
    pub(crate) fn earlier_by(
        &mut self,
        period: Duration,
        text: &mut Text,
        selection: &mut Selection,
    ) -> bool {
        let target = self.revisions[self.current]
            .time
            .checked_sub(period)
            .map_or(0, |time| self.newest_made_by(time));
        self.jump(target.min(self.current), text, selection)
    }

    /// Moves to the newest revision made at most `period` after the current one. Returns whether
    /// it moved.
    pub(crate) fn later_by(
        &mut self,
        period: Duration,
        text: &mut Text,
        selection: &mut Selection,
    ) -> bool {
        let newest = self.revisions.len() - 1;
        let target = self.revisions[self.current]
            .time
            .checked_add(period)
            .map_or(newest, |time| self.newest_made_by(time));
        self.jump(target.max(self.current), text, selection)
    }

    /// Moves to `target`: undoes up to the newest revision both share, then redoes down to
    /// `target`. Redo pointers on the way are left pointing back, so redo after an undo, or after
    /// a jump into another branch, retraces the way.
    fn jump(&mut self, target: usize, text: &mut Text, selection: &mut Selection) -> bool {
        if target == self.current {
            return false;
        }
        let ancestor = self.common_ancestor(self.current, target);
        while self.current != ancestor && self.undo(text, selection) {}
        // Point redo from the ancestor down to the target, then follow it.
        let mut child = target;
        while child != ancestor {
            let parent = self.revisions[child].parent;
            self.revisions[parent].redo_child = Some(child);
            child = parent;
        }
        while self.current != target && self.redo(text, selection) {}
        true
    }

    /// The newest revision that both `a` and `b` are or descend from.
    fn common_ancestor(&self, mut a: usize, mut b: usize) -> usize {
        // A parent comes before its children, so of two different revisions, the later one
        // can't be an ancestor of the other.
        while a != b {
            if a > b {
                a = self.revisions[a].parent;
            } else {
                b = self.revisions[b].parent;
            }
        }
        a
    }

    /// The newest revision made at or before `time`, or the root if every revision is newer.
    fn newest_made_by(&self, time: Instant) -> usize {
        // Revisions are made in time order.
        self.revisions
            .partition_point(|revision| revision.time <= time)
            .saturating_sub(1)
    }
}

#[cfg(test)]
impl History {
    pub(crate) fn current(&self) -> usize {
        self.current
    }

    pub(crate) fn revision_count(&self) -> usize {
        self.revisions.len()
    }
}
