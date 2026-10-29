use std::ops;
use std::time::{Duration, Instant};

use crate::history::History;
use crate::transaction::Transaction;
use crate::{ChangeSet, InvalidChange, Range, Selection, Text};

/// An edit this long or longer after the previous one starts a new revision (ADR 0029).
const PAUSE: Duration = Duration::from_secs(1);

/// A text, its selection, and its undo history.
///
/// Every edit, undo, and redo goes through the document, and it builds every change set from its
/// own text. A change set fits only the text it was made for, and applying one checks no more
/// than the length, so one made for another text could edit the wrong bytes or fail halfway.
///
/// Edits accumulate into a pending revision until a commit point ends it: a call to
/// [`commit`](Self::commit), or an edit that comes a second or more after the previous one. The
/// caller passes the time of each edit, so this crate reads no clock.
#[derive(Debug)]
pub struct Document {
    text: Text,
    selection: Selection,
    history: History,
    /// Edits since the last commit point, waiting to become one revision.
    pending: Option<Pending>,
}

#[derive(Debug)]
struct Pending {
    /// The text and selection before the first edit, which undo returns to. Text clones share
    /// structure, so this keeps only the parts the edits replace.
    text: Text,
    selection: Selection,
    /// The edits so far, composed into one.
    edits: Transaction,
    /// When the last edit came.
    last_edit: Instant,
}

impl Document {
    /// A document holding `text` as it was at `now`, with an empty range at its start.
    pub fn new(text: Text, now: Instant) -> Self {
        Self {
            text,
            selection: Selection::single(Range::point(0)),
            history: History::new(now),
            pending: None,
        }
    }

    pub fn text(&self) -> &Text {
        &self.text
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Sets the selection without an edit. It doesn't end the pending revision; a cursor move that
    /// should, such as an arrow key in Insert mode, commits as well.
    ///
    /// # Panics
    ///
    /// Panics if a range of `selection` ends past the text.
    pub fn set_selection(&mut self, selection: Selection) {
        let len = self.text.len();
        assert!(
            selection.ranges().iter().all(|range| range.end() <= len),
            "selection ends past the text ({len} bytes)",
        );
        self.selection = selection;
    }

    /// Replaces each range of the text with its string at `now`, and maps the selection through
    /// the change ([`Selection::map`]). An empty range inserts and an empty string deletes. The
    /// ranges must be sorted and must not overlap; they may touch, and several empty ranges may
    /// share an offset.
    ///
    /// The edit joins the pending revision. If the previous edit came a second or more before
    /// `now`, the pending revision is committed first and this edit starts a new one.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidChange`] for the first range that is reversed, starts before the previous
    /// one ends, runs past the end of the text, or splits a char. Nothing changes then: the text,
    /// the selection, and the pending revision stay as they were. Spans computed from an older
    /// snapshot of the text can do any of that, so the caller recomputes them.
    pub fn edit(
        &mut self,
        changes: impl IntoIterator<Item = (ops::Range<usize>, String)>,
        now: Instant,
    ) -> Result<(), InvalidChange> {
        let selection = self.selection.clone();
        self.edit_with(changes, |changes| selection.map(changes), now)
    }

    /// Like [`edit`](Self::edit), but the selection after the change is the one `selection`
    /// returns, given the change set to map positions through: a paste, say, selects the pasted
    /// text.
    ///
    /// # Errors
    ///
    /// As for [`edit`](Self::edit); `selection` isn't called then.
    ///
    /// # Panics
    ///
    /// Panics if `selection` returns a selection that ends past the changed text. Nothing has
    /// changed by then.
    pub fn edit_with(
        &mut self,
        changes: impl IntoIterator<Item = (ops::Range<usize>, String)>,
        selection: impl FnOnce(&ChangeSet) -> Selection,
        now: Instant,
    ) -> Result<(), InvalidChange> {
        // Whatever can fail comes first, so a rejected edit doesn't even commit on a pause.
        let changes = ChangeSet::new(&self.text, changes)?;
        let selection = selection(&changes);
        let transaction = Transaction::new(changes, selection);
        let paused = self
            .pending
            .as_ref()
            .is_some_and(|pending| now.saturating_duration_since(pending.last_edit) >= PAUSE);
        if paused {
            self.commit();
        }
        let pending = match self.pending.take() {
            Some(pending) => {
                transaction.apply(&mut self.text, &mut self.selection);
                Pending {
                    edits: pending.edits.compose(transaction),
                    last_edit: now,
                    ..pending
                }
            }
            None => {
                let (text, selection) = (self.text.clone(), self.selection.clone());
                transaction.apply(&mut self.text, &mut self.selection);
                Pending {
                    text,
                    selection,
                    edits: transaction,
                    last_edit: now,
                }
            }
        };
        self.pending = Some(pending);
        Ok(())
    }

    /// Ends the pending revision, so the next edit starts a new one. The caller commits where an
    /// undo step ends: on leaving Insert mode, on the checkpoint command, on a cursor move that
    /// isn't typing, and after each Normal-mode command.
    pub fn commit(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        // Edits that cancel out, like typing a char and deleting it, leave nothing to undo.
        if pending.edits.changes().is_identity() {
            return;
        }
        let undo = pending.edits.invert(&pending.text, pending.selection);
        self.history.push(pending.edits, undo, pending.last_edit);
    }

    /// Commits, then undoes the current revision, restoring the text and selection from before
    /// it. Returns false if there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.commit();
        self.history.undo(&mut self.text, &mut self.selection)
    }

    /// Commits, then redoes the revision undone or visited last from here, restoring the text and
    /// selection after it. Returns false if there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        self.commit();
        self.history.redo(&mut self.text, &mut self.selection)
    }

    /// Commits, then goes `steps` revisions back in the order they were made, across branches
    /// (Vim `g-`, Helix `Alt-u`). Returns false if the text was at the oldest revision.
    pub fn earlier(&mut self, steps: usize) -> bool {
        self.commit();
        self.history
            .earlier(steps, &mut self.text, &mut self.selection)
    }

    /// Commits, then goes `steps` revisions forward in the order they were made, across branches
    /// (Vim `g+`, Helix `Alt-U`). Returns false if the text was at the newest revision.
    pub fn later(&mut self, steps: usize) -> bool {
        self.commit();
        self.history
            .later(steps, &mut self.text, &mut self.selection)
    }

    /// Commits, then goes to the newest revision made at least `period` before the current one,
    /// or to the text the document started with (`:earlier 10m`). Returns whether it moved.
    pub fn earlier_by(&mut self, period: Duration) -> bool {
        self.commit();
        self.history
            .earlier_by(period, &mut self.text, &mut self.selection)
    }

    /// Commits, then goes to the newest revision made at most `period` after the current one
    /// (`:later 10m`). Returns whether it moved.
    pub fn later_by(&mut self, period: Duration) -> bool {
        self.commit();
        self.history
            .later_by(period, &mut self.text, &mut self.selection)
    }
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::sample::Index;

    use super::*;
    use crate::Assoc;
    use crate::strategies;

    /// Replaces `range` with `insert` at `now`, leaving a cursor after the new text.
    fn replace(document: &mut Document, range: ops::Range<usize>, insert: &str, now: Instant) {
        let end = range.end;
        let cursor =
            |changes: &ChangeSet| Selection::single(Range::point(changes.map(end, Assoc::After)));
        document
            .edit_with([(range, insert.to_owned())], cursor, now)
            .unwrap();
    }

    fn type_at_end(document: &mut Document, insert: &str, now: Instant) {
        let end = document.text().len();
        replace(document, end..end, insert, now);
    }

    fn text(document: &Document) -> String {
        document.text().to_string()
    }

    #[test]
    fn a_pause_of_a_second_after_the_previous_edit_ends_a_revision() {
        let start = Instant::now();
        let at = |millis| start + Duration::from_millis(millis);
        let mut document = Document::new(Text::from("a"), start);
        type_at_end(&mut document, "b", at(0));
        type_at_end(&mut document, "c", at(999));
        // Two seconds after the first edit, but less than one after the previous.
        type_at_end(&mut document, "d", at(1998));
        type_at_end(&mut document, "e", at(2998));
        assert!(document.undo());
        assert_eq!(text(&document), "abcd");
        assert!(document.undo());
        assert_eq!(text(&document), "a");
        assert!(!document.undo());
    }

    #[test]
    fn commit_ends_a_revision() {
        let now = Instant::now();
        let mut document = Document::new(Text::from("a"), now);
        type_at_end(&mut document, "b", now);
        document.commit();
        type_at_end(&mut document, "c", now);
        assert!(document.undo());
        assert_eq!(text(&document), "ab");
    }

    #[test]
    fn edits_that_cancel_out_leave_nothing_to_undo() {
        let now = Instant::now();
        let mut document = Document::new(Text::from("a"), now);
        type_at_end(&mut document, "b", now);
        replace(&mut document, 1..2, "", now);
        assert!(!document.undo());
        assert_eq!(text(&document), "a");
    }

    #[test]
    fn a_rejected_edit_changes_nothing() {
        let start = Instant::now();
        let mut document = Document::new(Text::from("aé"), start);
        type_at_end(&mut document, "b", start);
        let selection = document.selection().clone();
        // Offset 2 is inside 'é'. The edit comes 2 s after "b", so it would end "b"'s revision if
        // the pause were checked first.
        let rejected = document.edit([(2..2, "Z".to_owned())], start + Duration::from_secs(2));
        assert_eq!(rejected, Err(InvalidChange::SplitsChar { range: 2..2 }));
        assert_eq!(text(&document), "aéb");
        assert_eq!(document.selection(), &selection);
        // "b" is still pending: an edit right after it joins its revision.
        type_at_end(&mut document, "c", start);
        assert!(document.undo());
        assert_eq!(text(&document), "aé");
    }

    #[test]
    fn undo_and_redo_restore_the_selections() {
        let now = Instant::now();
        let mut document = Document::new(Text::from("abc"), now);
        let before = Selection::single(Range::new(2, 0));
        document.set_selection(before.clone());
        replace(&mut document, 0..2, "x", now);
        let after = document.selection().clone();
        assert!(document.undo());
        assert_eq!(document.selection(), &before);
        assert!(document.redo());
        assert_eq!(document.selection(), &after);
    }

    #[test]
    fn redo_follows_the_branch_visited_last() {
        let now = Instant::now();
        let mut document = Document::new(Text::new(), now);
        type_at_end(&mut document, "a", now);
        document.commit();
        type_at_end(&mut document, "b", now);
        assert!(document.undo());
        // A second branch from "a".
        type_at_end(&mut document, "c", now);
        // One step back in time is the tip of the first branch.
        assert!(document.earlier(1));
        assert_eq!(text(&document), "ab");
        assert!(document.undo());
        assert_eq!(text(&document), "a");
        // Redo goes back where undo came from, not to the newest branch.
        assert!(document.redo());
        assert_eq!(text(&document), "ab");
        assert!(document.later(1));
        assert_eq!(text(&document), "ac");
    }

    #[test]
    fn time_jumps_go_to_the_newest_revision_made_by_then() {
        let start = Instant::now();
        let at = |seconds| start + Duration::from_secs(seconds);
        let mut document = Document::new(Text::new(), start);
        // Ten seconds apart, so each edit is a revision.
        type_at_end(&mut document, "a", at(10));
        type_at_end(&mut document, "b", at(20));
        type_at_end(&mut document, "c", at(30));
        // From 30 s to one made by 20 s: a revision made right at the limit counts.
        assert!(document.earlier_by(Duration::from_secs(10)));
        assert_eq!(text(&document), "ab");
        // From 20 s to one made by 15 s.
        assert!(document.earlier_by(Duration::from_secs(5)));
        assert_eq!(text(&document), "a");
        // Past the first revision: the text the document started with.
        assert!(document.earlier_by(Duration::from_secs(60)));
        assert_eq!(text(&document), "");
        // From the start to one made by 15 s.
        assert!(document.later_by(Duration::from_secs(15)));
        assert_eq!(text(&document), "a");
        assert!(document.later_by(Duration::from_secs(60)));
        assert_eq!(text(&document), "abc");
        assert!(!document.later_by(Duration::from_secs(60)));
    }

    /// Something a user does to a document.
    #[derive(Clone, Debug)]
    enum Action {
        /// Replaces the text between two char boundaries, picked by index, `pause` milliseconds
        /// after the previous edit.
        Edit {
            start: Index,
            end: Index,
            insert: String,
            pause: u64,
        },
        Commit,
        Undo,
        Redo,
        Earlier(usize),
        Later(usize),
        /// A time jump by a period in milliseconds.
        EarlierBy(u64),
        LaterBy(u64),
    }

    fn actions() -> impl Strategy<Value = Vec<Action>> {
        let edit = (
            any::<Index>(),
            any::<Index>(),
            strategies::insert(),
            0..2000_u64,
        )
            .prop_map(|(start, end, insert, pause)| Action::Edit {
                start,
                end,
                insert,
                pause,
            });
        let action = prop_oneof![
            4 => edit,
            1 => Just(Action::Commit),
            1 => Just(Action::Undo),
            1 => Just(Action::Redo),
            1 => (0..4_usize).prop_map(Action::Earlier),
            1 => (0..4_usize).prop_map(Action::Later),
            1 => (0..5000_u64).prop_map(Action::EarlierBy),
            1 => (0..5000_u64).prop_map(Action::LaterBy),
        ];
        vec(action, 0..48)
    }

    /// Does `action`, adding the text of a revision it commits to `texts`. Returns the revision
    /// it started from: the one it committed, if any.
    fn act(
        document: &mut Document,
        texts: &mut Vec<String>,
        now: &mut Instant,
        action: &Action,
    ) -> usize {
        let before = document.text().to_string();
        let current = document.history.current();
        let count = document.history.revision_count();
        match action {
            Action::Edit {
                start,
                end,
                insert,
                pause,
            } => {
                *now += Duration::from_millis(*pause);
                let boundaries = strategies::char_boundaries(&before);
                let start = boundaries[start.index(boundaries.len())];
                let end = boundaries[end.index(boundaries.len())];
                let range = start.min(end)..start.max(end);
                document.edit([(range, insert.clone())], *now).unwrap();
            }
            Action::Commit => document.commit(),
            Action::Undo => _ = document.undo(),
            Action::Redo => _ = document.redo(),
            Action::Earlier(steps) => _ = document.earlier(*steps),
            Action::Later(steps) => _ = document.later(*steps),
            Action::EarlierBy(millis) => _ = document.earlier_by(Duration::from_millis(*millis)),
            Action::LaterBy(millis) => _ = document.later_by(Duration::from_millis(*millis)),
        }
        // A commit adds one revision at most, before anything moves.
        if document.history.revision_count() == count {
            return current;
        }
        texts.push(before);
        count
    }

    proptest! {
        #[test]
        fn every_revision_keeps_its_text((text, actions) in (strategies::text(), actions())) {
            let mut now = Instant::now();
            let mut document = Document::new(Text::from(text.as_str()), now);
            // The text of each revision, in the order they were made.
            let mut texts = vec![text];
            for action in &actions {
                let from = act(&mut document, &mut texts, &mut now, action);
                let current = document.history.current();
                let newest = texts.len() - 1;
                match action {
                    Action::Earlier(steps) => prop_assert_eq!(current, from.saturating_sub(*steps)),
                    Action::Later(steps) => prop_assert_eq!(current, (from + steps).min(newest)),
                    _ => {}
                }
                match &document.pending {
                    // Pending edits start from the current revision.
                    Some(pending) => prop_assert_eq!(&pending.text.to_string(), &texts[current]),
                    None => prop_assert_eq!(&document.text().to_string(), &texts[current]),
                }
            }
            act(&mut document, &mut texts, &mut now, &Action::Earlier(usize::MAX));
            prop_assert_eq!(&document.text().to_string(), &texts[0]);
            act(&mut document, &mut texts, &mut now, &Action::Later(usize::MAX));
            prop_assert_eq!(&document.text().to_string(), &texts[texts.len() - 1]);
        }
    }
}
