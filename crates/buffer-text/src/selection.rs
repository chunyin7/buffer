use std::cmp::Ordering;

use smallvec::{SmallVec, smallvec};

use crate::change::{Assoc, Mapper};
use crate::{ChangeSet, Text};

/// A span of text between an anchor and a head, as byte offsets.
///
/// It covers the bytes from [`start`](Self::start) up to, not including, [`end`](Self::end).
/// Extending it moves the head and keeps the anchor's grapheme covered
/// ([`extend_to`](Self::extend_to)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub anchor: usize,
    pub head: usize,
}

impl Range {
    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    /// An empty range at `position`.
    pub fn point(position: usize) -> Self {
        Self::new(position, position)
    }

    /// The smaller of anchor and head.
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    /// The larger of anchor and head.
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Where the block cursor starts: the grapheme at the head, or the one before the head when
    /// the range points forward (ADR 0027). For an empty range, such as a caret in Insert mode,
    /// it is the head.
    ///
    /// # Panics
    ///
    /// Panics if the head is past the end of `text` or inside a char.
    pub fn cursor(&self, text: &Text) -> usize {
        if self.head > self.anchor {
            text.prev_grapheme_boundary(self.head)
        } else {
            // In the other branch, `prev_grapheme_boundary` checks the head.
            assert!(
                text.is_char_boundary(self.head),
                "head {} is inside a char",
                self.head,
            );
            self.head
        }
    }

    /// Extends the range to the grapheme at `target`, as Vim's Visual mode and Helix's Select
    /// mode do: it then covers the anchor's grapheme, the target's, and those between, with the
    /// block cursor on the target's.
    ///
    /// The anchor's grapheme is the one after the anchor, or the one before it when the range
    /// points backward. The range points backward when the target comes before that grapheme, so
    /// when the head crosses it, the anchor moves to its other edge. Extending to the end of the
    /// text covers the rest of it, with the block cursor on the last grapheme.
    ///
    /// # Panics
    ///
    /// Panics if the anchor or `target` is past the end of `text` or inside a char.
    pub fn extend_to(self, text: &Text, target: usize) -> Self {
        let (anchor_start, anchor_end) = if self.head < self.anchor {
            (text.prev_grapheme_boundary(self.anchor), self.anchor)
        } else {
            (self.anchor, text.next_grapheme_boundary(self.anchor))
        };
        let start = anchor_start.min(target);
        let end = anchor_end.max(text.next_grapheme_boundary(target));
        if target < anchor_start {
            Self::new(end, start)
        } else {
            Self::new(start, end)
        }
    }

    /// Whether one range starts where the other starts or inside it, before its end. Touching
    /// ranges don't overlap; an empty range inside another does.
    fn overlaps(&self, other: &Self) -> bool {
        self.start() == other.start() || (self.end() > other.start() && other.end() > self.start())
    }

    /// The smallest range covering both; it points backward only if both do.
    fn merge(self, other: Self) -> Self {
        if self.anchor > self.head && other.anchor > other.head {
            Self::new(self.anchor.max(other.anchor), self.head.min(other.head))
        } else {
            Self::new(self.start().min(other.start()), self.end().max(other.end()))
        }
    }

    /// A range over `start..end` that points as this one does.
    fn spanning(self, start: usize, end: usize) -> Self {
        if self.anchor > self.head {
            Self::new(end, start)
        } else {
            Self::new(start, end)
        }
    }

    /// The range grown to whole lines; see [`Selection::extend_to_lines`].
    fn extend_to_lines(self, text: &Text) -> Self {
        let start = text.line_start(text.line_of(self.start()));
        self.spanning(start, text.line_start(self.last_line(text) + 1))
    }

    /// The line of the last byte the range covers: a range that ends where a line starts holds
    /// the line before it whole, break and all. For an empty range, it is the line it is on.
    fn last_line(self, text: &Text) -> usize {
        let line = text.line_of(self.end());
        if !self.is_empty() && self.end() == text.line_start(line) {
            line - 1
        } else {
            line
        }
    }

    /// The range's part of each line it covers, without the line breaks; see
    /// [`Selection::split_lines`].
    fn split_lines(self, text: &Text) -> impl Iterator<Item = Self> {
        (text.line_of(self.start())..=text.line_of(self.end())).filter_map(move |line| {
            let start = self.start().max(text.line_start(line));
            let end = self.end().min(text.line_end(line));
            (start < end).then(|| self.spanning(start, end))
        })
    }

    /// A copy of the range on the nearest lines below it, or above it, that can hold it; see
    /// [`Selection::copy_to_line`].
    fn copy_to_line(self, text: &Text, below: bool) -> Option<Self> {
        let (first, last) = (text.line_of(self.start()), self.last_line(text));
        let start_column = text.column_of(self.start());
        // No column for an end where a line starts: the range holds its last line's break, and
        // so does its copy.
        let end_column = (text.line_of(self.end()) == last).then(|| text.column_of(self.end()));
        // A range that starts on a line break may start on one again, where the line has one.
        let starts_on_break = !self.is_empty() && self.start() == text.line_end(first);
        // As many lines away as the range is tall, or further, past lines that are too short.
        let height = last - first + 1;
        (height..)
            .map_while(|lines| {
                if below {
                    (last + lines < text.line_count()).then_some((first + lines, last + lines))
                } else {
                    Some((first.checked_sub(lines)?, last - lines))
                }
            })
            .find_map(|(first, last)| {
                // The start lands on a grapheme of the line; an empty range may sit at its end.
                let start = text.at_column(first, start_column)?;
                let limit = if starts_on_break {
                    text.len()
                } else {
                    text.line_end(first)
                };
                if start >= limit && !self.is_empty() {
                    return None;
                }
                let end = if starts_on_break && first == last {
                    // A line break alone copies as the one grapheme at its column.
                    text.next_grapheme_boundary(start)
                } else {
                    match end_column {
                        Some(column) => text.at_column(last, column),
                        None => Some(text.line_start(last + 1)),
                    }
                    .unwrap_or_else(|| text.line_end(last))
                };
                Some(self.spanning(start, end))
            })
    }

    /// Maps the range through a change. Its start stays after text inserted at it and its end
    /// before, so inserts at its edges don't grow it; an empty range moves past such text.
    fn map(self, mapper: &mut Mapper<'_>) -> Self {
        if self.is_empty() {
            return Self::point(mapper.map(self.head, Assoc::After));
        }
        // The start first: the mapper takes positions in order.
        let start = mapper.map(self.start(), Assoc::After);
        let end = mapper.map(self.end(), Assoc::Before);
        // A range strictly inside replaced bytes maps past its own end; swapped, it covers the
        // new text.
        let (start, end) = (start.min(end), start.max(end));
        if self.anchor < self.head {
            Self::new(start, end)
        } else {
            Self::new(end, start)
        }
    }

    /// The range as Normal mode keeps it; see [`Selection::for_normal_mode`].
    fn for_normal_mode(self, text: &Text) -> Self {
        // The grapheme boundary at or before an offset, and the one at or after it.
        let floor = |offset| {
            if text.is_grapheme_boundary(offset) {
                offset
            } else {
                text.prev_grapheme_boundary(offset)
            }
        };
        let ceil = |offset| {
            if text.is_grapheme_boundary(offset) {
                offset
            } else {
                text.next_grapheme_boundary(offset)
            }
        };
        match self.anchor.cmp(&self.head) {
            Ordering::Less => Self::new(floor(self.anchor), ceil(self.head)),
            Ordering::Greater => Self::new(ceil(self.anchor), floor(self.head)),
            Ordering::Equal => {
                let start = floor(self.head);
                let len = text.len();
                if start < len {
                    Self::new(start, text.next_grapheme_boundary(start))
                } else if text.line_start(text.line_of(len)) == len {
                    // An empty line starts at the end: the text is empty or ends with a break.
                    Self::point(len)
                } else {
                    Self::new(text.prev_grapheme_boundary(len), len)
                }
            }
        }
    }
}

/// One or more ranges, one of them primary.
///
/// The ranges are sorted and never overlap: a range that starts where another starts, or inside
/// it, merges with it. Touching ranges stay apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    ranges: SmallVec<[Range; 1]>,
    primary_index: usize,
}

impl Selection {
    /// A selection of one range.
    pub fn single(range: Range) -> Self {
        Self {
            ranges: smallvec![range],
            primary_index: 0,
        }
    }

    /// A selection of `primary` and `others`. Sorts the ranges and merges overlapping ones; the
    /// primary is the range `primary` ends up in. `others` may include `primary` again.
    pub fn new(primary: Range, others: impl IntoIterator<Item = Range>) -> Self {
        let mut ranges: SmallVec<[Range; 1]> = smallvec![primary];
        ranges.extend(others);
        let mut selection = Self {
            ranges,
            primary_index: 0,
        };
        selection.normalize();
        selection
    }

    /// One range over the whole of `text`, as Helix's `%` selects it (ADR 0043).
    pub fn all(text: &Text) -> Self {
        Self::single(Range::new(0, text.len()))
    }

    /// The ranges, sorted by start.
    pub fn ranges(&self) -> &[Range] {
        &self.ranges
    }

    pub fn primary(&self) -> Range {
        self.ranges[self.primary_index]
    }

    pub fn primary_index(&self) -> usize {
        self.primary_index
    }

    /// Maps every range through `changes`, then merges ranges that now overlap.
    pub fn map(mut self, changes: &ChangeSet) -> Self {
        if changes.is_identity() {
            return self;
        }
        // Sorted, non-overlapping ranges give positions in order: one pass over the change set.
        let mut mapper = Mapper::new(changes);
        for range in &mut self.ranges {
            *range = range.map(&mut mapper);
        }
        self.normalize();
        self
    }

    /// The selection as Normal mode keeps it (ADR 0032): every range covers at least one
    /// grapheme, except an empty range at the end of the text where an empty line starts, in an
    /// empty text or after a final line break.
    ///
    /// - A range end inside a grapheme moves outward to the grapheme's edge. An edit can leave it
    ///   there: another cursor adds a combining accent right after the range, say.
    /// - An empty range covers the grapheme it is in, or the one that starts at it. At the end of
    ///   the text, where no empty line starts, it covers the last grapheme.
    /// - Ranges that now overlap merge.
    ///
    /// # Panics
    ///
    /// Panics if a range ends past the end of `text` or inside a char.
    pub fn for_normal_mode(mut self, text: &Text) -> Self {
        for range in &mut self.ranges {
            *range = range.for_normal_mode(text);
        }
        self.normalize();
        self
    }

    /// The text of each range, in the order of [`ranges`](Self::ranges), for a register to hold
    /// (ADR 0040). An empty range gives an empty string.
    ///
    /// # Panics
    ///
    /// Panics if a range ends past the end of `text` or inside a char.
    pub fn yank(&self, text: &Text) -> Vec<String> {
        self.ranges
            .iter()
            .map(|range| text.to_string_in(range.start()..range.end()))
            .collect()
    }

    /// Collapses every range to an empty one at its block cursor, as Helix's `;` does
    /// (ADR 0043).
    ///
    /// # Panics
    ///
    /// Panics if a range's head is past the end of `text` or inside a char.
    pub fn collapse(mut self, text: &Text) -> Self {
        for range in &mut self.ranges {
            *range = Range::point(range.cursor(text));
        }
        self.normalize();
        self
    }

    /// Swaps every range's anchor and head, as Helix's `Alt-;` and Vim's `o` do (ADR 0043).
    pub fn flip(mut self) -> Self {
        for range in &mut self.ranges {
            *range = Range::new(range.head, range.anchor);
        }
        self
    }

    /// Grows every range to whole lines, as Helix's `x` and Vim's linewise selections do
    /// (ADR 0043): back to the start of its first line, and on to the start of the line after
    /// its last, line break included, or to the end of the text. A range keeps its direction,
    /// and one that covers whole lines already stays as it is. Ranges on one line merge.
    ///
    /// # Panics
    ///
    /// Panics if a range ends past the end of `text` or inside a char.
    pub fn extend_to_lines(mut self, text: &Text) -> Self {
        for range in &mut self.ranges {
            *range = range.extend_to_lines(text);
        }
        self.normalize();
        self
    }

    /// Takes the blank graphemes off both ends of every range, line breaks among them, as
    /// Helix's `_` does (ADR 0043). A range that holds nothing else is dropped, and if it was
    /// primary, the next range is, or the previous one after the last. If every range is blank,
    /// nothing changes.
    ///
    /// # Panics
    ///
    /// Panics if a range ends past the end of `text` or inside a char.
    pub fn trim(self, text: &Text) -> Self {
        let trimmed: Vec<Option<Range>> = self
            .ranges
            .iter()
            .map(|range| {
                let span = text.trim_blanks(range.start()..range.end())?;
                Some(range.spanning(span.start, span.end))
            })
            .collect();
        let (before, from_primary) = trimmed.split_at(self.primary_index);
        let primary = from_primary
            .iter()
            .chain(before.iter().rev())
            .flatten()
            .next();
        match primary {
            Some(&primary) => Self::new(primary, trimmed.into_iter().flatten()),
            None => self,
        }
    }

    /// Keeps only the primary range, as Helix's `,` does (ADR 0043).
    pub fn keep_primary(self) -> Self {
        Self::single(self.primary())
    }

    /// Drops the primary range, as Helix's `Alt-,` does (ADR 0043). The next range is primary
    /// then, or the previous one after the last. The only range stays.
    pub fn drop_primary(mut self) -> Self {
        if self.ranges.len() > 1 {
            self.ranges.remove(self.primary_index);
            self.primary_index = self.primary_index.min(self.ranges.len() - 1);
        }
        self
    }

    /// Makes the range `count` ranges after the primary one primary, or before it, as Helix's
    /// `)` and `(` do (ADR 0043), wrapping around.
    pub fn rotate_primary(mut self, count: usize, forward: bool) -> Self {
        let len = self.ranges.len();
        let steps = count % len;
        self.primary_index = (self.primary_index + if forward { steps } else { len - steps }) % len;
        self
    }

    /// Merges the ranges into one, from the first one's start to the last one's end, pointing as
    /// the primary does, as Helix's `Alt-minus` does (ADR 0043).
    pub fn merge(self) -> Self {
        let (first, last) = (self.ranges[0], self.ranges[self.ranges.len() - 1]);
        Self::single(self.primary().spanning(first.start(), last.end()))
    }

    /// Splits every range into its part of each line it covers, without the line breaks, as
    /// Helix's `Alt-s` does (ADR 0043). Empty parts are dropped, and a range with no other part,
    /// an empty one or one of line breaks only, stays whole. The parts point as the range did.
    /// Of the primary range's parts, the one with its block cursor is primary, or the nearest
    /// one if the cursor is on a line break.
    ///
    /// # Panics
    ///
    /// Panics if a range ends past the end of `text` or inside a char.
    pub fn split_lines(self, text: &Text) -> Self {
        let mut parts = Vec::with_capacity(self.ranges.len());
        let mut primary = self.primary();
        for (index, range) in self.ranges.iter().enumerate() {
            let first = parts.len();
            parts.extend(range.split_lines(text));
            if parts.len() == first {
                parts.push(*range);
            }
            if index == self.primary_index {
                // The block cursor is at the head's end of the range.
                let at_head = if range.head < range.anchor {
                    first
                } else {
                    parts.len() - 1
                };
                primary = parts[at_head];
            }
        }
        Self::new(primary, parts)
    }

    /// Adds a copy of every range on the lines below it, or above it, as Helix's `C` and `Alt-C`
    /// do (ADR 0043). The copy is as many lines away as the range is tall, with its ends at the
    /// same columns, counted in graphemes. A line too short for the copy's start to land on a
    /// grapheme is passed over for the next one; an empty range may land at a line's end, and a
    /// range that starts on a line break on another one. A copy whose end finds its line too
    /// short ends with the line. The copy of a range that ends with a line break ends with one
    /// too, or with the text on the last line; a line break alone, a cursor at a line's end,
    /// copies as the one grapheme at its column. A range with no such lines gets no copy. The
    /// primary range's copy is primary, and copies that overlap a range merge with it.
    ///
    /// # Panics
    ///
    /// Panics if a range ends past the end of `text` or inside a char.
    pub fn copy_to_line(self, text: &Text, below: bool) -> Self {
        let copy = |range: &Range| range.copy_to_line(text, below);
        let primary = copy(&self.primary()).unwrap_or(self.primary());
        let copies = self.ranges.iter().filter_map(copy);
        Self::new(primary, self.ranges.iter().copied().chain(copies))
    }

    #[expect(
        clippy::expect_used,
        reason = "the primary range is kept or merged into a kept range"
    )]
    fn normalize(&mut self) {
        if self.ranges.len() < 2 {
            return;
        }
        let mut primary = self.ranges[self.primary_index];
        self.ranges.sort_unstable_by_key(Range::start);
        self.ranges.dedup_by(|range, previous| {
            if !previous.overlaps(range) {
                return false;
            }
            let merged = previous.merge(*range);
            if *previous == primary || *range == primary {
                primary = merged;
            }
            *previous = merged;
            true
        });
        self.primary_index = self
            .ranges
            .iter()
            .position(|range| *range == primary)
            .expect("the primary range is in the selection");
    }
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::sample::select;
    use unicode_segmentation::UnicodeSegmentation;

    use super::*;
    use crate::Text;
    use crate::strategies::{self, Change};

    fn mapped(text: &str, changes: &[Change], range: Range) -> Range {
        let changes = ChangeSet::new(&Text::from(text), changes.iter().cloned()).unwrap();
        Selection::single(range).map(&changes).primary()
    }

    #[test]
    fn typing_moves_a_cursor_past_the_text() {
        let typed = [(2..2, "XY".to_owned())];
        assert_eq!(mapped("abcd", &typed, Range::point(2)), Range::point(4));
    }

    #[test]
    fn inserts_at_the_edges_stay_outside() {
        let inserts = [(1..1, "X".to_owned()), (3..3, "Y".to_owned())];
        // "abcd" -> "aXbcYd": still "bc", either way round.
        assert_eq!(mapped("abcd", &inserts, Range::new(1, 3)), Range::new(2, 4));
        assert_eq!(mapped("abcd", &inserts, Range::new(3, 1)), Range::new(4, 2));
    }

    #[test]
    fn replaced_bytes_map_to_the_new_text() {
        let replace = [(1..5, "XY".to_owned())];
        // "abcdef" -> "aXYf": the replaced range, or one inside it, covers "XY".
        assert_eq!(
            mapped("abcdef", &replace, Range::new(1, 5)),
            Range::new(1, 3)
        );
        assert_eq!(
            mapped("abcdef", &replace, Range::new(5, 1)),
            Range::new(3, 1)
        );
        assert_eq!(
            mapped("abcdef", &replace, Range::new(2, 4)),
            Range::new(1, 3)
        );
    }

    #[test]
    fn ranges_merge_when_they_share_a_byte_or_a_start() {
        let merge = |a: Range, b: Range| Selection::new(a, [b]).ranges().to_vec();
        assert_eq!(
            merge(Range::new(0, 3), Range::new(2, 4)),
            [Range::new(0, 4)]
        );
        assert_eq!(merge(Range::point(2), Range::new(2, 4)), [Range::new(2, 4)]);
        assert_eq!(merge(Range::point(2), Range::point(2)), [Range::point(2)]);
        // Touching ranges, and a point at a range's end, stay apart.
        let touching = [Range::new(0, 2), Range::new(2, 4)];
        assert_eq!(merge(touching[0], touching[1]), touching);
        assert_eq!(
            merge(Range::new(0, 2), Range::point(2)),
            [Range::new(0, 2), Range::point(2)]
        );
    }

    #[test]
    fn merged_ranges_point_backward_only_if_both_do() {
        let backward = Selection::new(Range::new(4, 1), [Range::new(6, 3)]);
        assert_eq!(backward.ranges(), [Range::new(6, 1)]);
        let mixed = Selection::new(Range::new(1, 4), [Range::new(6, 3)]);
        assert_eq!(mixed.ranges(), [Range::new(1, 6)]);
    }

    #[test]
    fn collapse_leaves_the_block_cursors_and_flip_swaps_the_ends() {
        let text = Text::from("abc def");
        // `abc` forward, its cursor on `c`, and `def` backward, the primary, its cursor on `d`.
        let selection = Selection::new(Range::new(7, 4), [Range::new(0, 3)]);
        let collapsed = selection.clone().collapse(&text);
        assert_eq!(collapsed.ranges(), [Range::point(2), Range::point(4)]);
        assert_eq!(collapsed.primary(), Range::point(4));
        let flipped = selection.flip();
        assert_eq!(flipped.ranges(), [Range::new(3, 0), Range::new(4, 7)]);
        assert_eq!(flipped.primary(), Range::new(4, 7));
    }

    #[test]
    fn extend_to_lines_takes_whole_lines_with_their_breaks() {
        // Lines `ab`, `cd`, an empty one, and `ef` start at 0, 3, 6, and 7.
        let text = Text::from("ab\ncd\n\nef");
        let extend = |range| Selection::single(range).extend_to_lines(&text).primary();
        assert_eq!(extend(Range::point(1)), Range::new(0, 3));
        assert_eq!(extend(Range::point(6)), Range::new(6, 7));
        // The last line has no break. A range across lines keeps its direction.
        assert_eq!(extend(Range::point(8)), Range::new(7, 9));
        assert_eq!(extend(Range::new(5, 1)), Range::new(6, 0));
        // Whole lines stay as they are, and ranges on one line merge.
        assert_eq!(extend(Range::new(0, 3)), Range::new(0, 3));
        let on_one_line = Selection::new(Range::new(0, 1), [Range::new(1, 2)]);
        assert_eq!(
            on_one_line.extend_to_lines(&text).ranges(),
            [Range::new(0, 3)]
        );
    }

    #[test]
    fn trim_drops_blank_ranges_and_hands_the_primary_on() {
        // ` ab ` 0..4, two line breaks, and ` cd` 6..9.
        let text = Text::from(" ab \n\n cd");
        let blank = Range::new(4, 6);
        let selection = Selection::new(blank, [Range::new(4, 0), Range::new(6, 9)]);
        let trimmed = selection.trim(&text);
        assert_eq!(trimmed.ranges(), [Range::new(3, 1), Range::new(7, 9)]);
        assert_eq!(trimmed.primary(), Range::new(7, 9));
        // With nothing else left, a blank range stays.
        assert_eq!(Selection::single(blank).trim(&text).primary(), blank);
    }

    #[test]
    fn the_primary_range_is_kept_dropped_or_rotated() {
        let ranges = [Range::new(0, 1), Range::new(2, 3), Range::new(4, 5)];
        let with_primary = |index: usize| Selection::new(ranges[index], ranges);
        assert_eq!(with_primary(1).keep_primary().ranges(), [ranges[1]]);
        // The next range takes over from a dropped primary, or the previous one after the last.
        let dropped = with_primary(1).drop_primary();
        assert_eq!(dropped.ranges(), [ranges[0], ranges[2]]);
        assert_eq!(dropped.primary(), ranges[2]);
        assert_eq!(with_primary(2).drop_primary().primary(), ranges[1]);
        let only = Selection::single(ranges[0]);
        assert_eq!(only.clone().drop_primary(), only);
        // Rotating wraps around, either way.
        assert_eq!(with_primary(1).rotate_primary(2, true).primary(), ranges[0]);
        assert_eq!(
            with_primary(1).rotate_primary(5, false).primary(),
            ranges[2]
        );
    }

    #[test]
    fn merge_spans_the_ranges_and_points_as_the_primary_does() {
        let others = [Range::new(0, 1), Range::new(4, 5)];
        let forward = Selection::new(Range::new(2, 3), others);
        assert_eq!(forward.merge().ranges(), [Range::new(0, 5)]);
        let backward = Selection::new(Range::new(3, 2), others);
        assert_eq!(backward.merge().ranges(), [Range::new(5, 0)]);
    }

    #[test]
    fn split_lines_leaves_the_line_breaks_out() {
        // Lines `ab`, `cd`, an empty one, and `ef` start at 0, 3, 6, and 7.
        let text = Text::from("ab\ncd\n\nef");
        let split = Selection::single(Range::new(1, 8)).split_lines(&text);
        let parts = [Range::new(1, 2), Range::new(3, 5), Range::new(7, 8)];
        assert_eq!(split.ranges(), parts);
        // The part with the block cursor is primary: the last one, or the first one backward.
        assert_eq!(split.primary(), parts[2]);
        let split = Selection::single(Range::new(8, 1)).split_lines(&text);
        let parts = [Range::new(2, 1), Range::new(5, 3), Range::new(8, 7)];
        assert_eq!(split.ranges(), parts);
        assert_eq!(split.primary(), parts[0]);
        // A range of line breaks only stays whole.
        let breaks = Selection::single(Range::new(5, 7));
        assert_eq!(breaks.clone().split_lines(&text), breaks);
    }

    #[test]
    fn copy_to_line_keeps_the_columns_and_passes_over_short_lines() {
        // Lines `abcd`, `ab`, an empty one, `abcdef`, and `xy` start at 0, 5, 8, 9, and 16.
        let text = Text::from("abcd\nab\n\nabcdef\nxy");
        let copy = |range, below| {
            let selection = Selection::single(range).copy_to_line(&text, below);
            (selection.ranges().to_vec(), selection.primary())
        };
        // From `c` on the first line, the next two lines are too short. The copy is primary.
        let (on_c, copied) = (Range::new(2, 3), Range::new(11, 12));
        assert_eq!(copy(on_c, true), (vec![on_c, copied], copied));
        assert_eq!(copy(copied, false), (vec![on_c, copied], on_c));
        // An empty range can sit at the end of `ab`.
        assert_eq!(copy(Range::point(2), true).1, Range::point(7));
        // A copy ends with a line too short for its end, and points as the range does.
        assert_eq!(copy(Range::new(14, 10), true).1, Range::new(18, 17));
        // A range two lines tall goes two lines down, or further: here, three.
        assert_eq!(copy(Range::new(1, 6), true).1, Range::new(10, 17));
        // A whole line with its break: its copy is the last line, which has none.
        assert_eq!(copy(Range::new(9, 16), true).1, Range::new(16, 18));
        // A cursor on a line break stays one grapheme wide: on a char of a longer line, or on
        // the break of a line that ends at its column. The last line has no break to hold it.
        assert_eq!(copy(Range::new(7, 8), true).1, Range::new(11, 12));
        assert_eq!(copy(Range::new(7, 8), false).1, Range::new(2, 3));
        assert_eq!(copy(Range::new(15, 16), true).1, Range::new(15, 16));
        let breaks =
            Selection::single(Range::new(2, 3)).copy_to_line(&Text::from("ab\n\nab\n"), true);
        assert_eq!(breaks.primary(), Range::new(6, 7));
        // Without a line to hold it, there is no copy.
        let last = Range::new(16, 17);
        assert_eq!(copy(last, true), (vec![last], last));
        assert_eq!(copy(on_c, false), (vec![on_c], on_c));
    }

    /// A primary range and up to six others over offsets 0..32, pointing either way.
    fn primary_and_others() -> impl Strategy<Value = (Range, Vec<Range>)> {
        let range = (0..32_usize, 0..32_usize).prop_map(|(anchor, head)| Range::new(anchor, head));
        (range.clone(), vec(range, 0..7))
    }

    /// A text, a range with both ends on grapheme boundaries, and a grapheme boundary to extend
    /// it to.
    fn text_range_and_target() -> impl Strategy<Value = (String, Range, usize)> {
        strategies::text().prop_flat_map(|text| {
            let boundary = select(strategies::grapheme_boundaries(&text));
            (Just(text), boundary.clone(), boundary.clone(), boundary)
                .prop_map(|(text, anchor, head, target)| (text, Range::new(anchor, head), target))
        })
    }

    /// What Normal mode makes of `range`, worked out on the grapheme `boundaries` of `text`.
    fn in_normal_mode(text: &str, boundaries: &[usize], range: Range) -> Range {
        // The boundary at or before an offset, and the one at or after it.
        let floor = |offset: usize| boundaries[boundaries.partition_point(|&b| b <= offset) - 1];
        let ceil = |offset: usize| boundaries[boundaries.partition_point(|&b| b < offset)];
        let len = text.len();
        if range.anchor < range.head {
            Range::new(floor(range.anchor), ceil(range.head))
        } else if range.head < range.anchor {
            Range::new(ceil(range.anchor), floor(range.head))
        } else if floor(range.head) < len {
            Range::new(floor(range.head), ceil(range.head + 1))
        } else if text.is_empty() || text.ends_with('\n') {
            Range::point(len)
        } else {
            Range::new(floor(len - 1), len)
        }
    }

    proptest! {
        #[test]
        fn split_lines_gives_the_pieces_of_each_range_between_line_breaks(
            (string, (primary, others)) in strategies::text_and_ranges(),
        ) {
            let text = Text::from(string.as_str());
            // Ranges on grapheme boundaries, as a preset keeps them: none ends inside a `\r\n`.
            let selection = Selection::new(primary, others).for_normal_mode(&text);
            let model = |range: &Range| {
                let mut parts = Vec::new();
                let mut start = range.start();
                for piece in string[range.start()..range.end()].split_inclusive('\n') {
                    let content = match piece.strip_suffix('\n') {
                        Some(content) => content.strip_suffix('\r').unwrap_or(content),
                        None => piece,
                    };
                    if !content.is_empty() {
                        parts.push(range.spanning(start, start + content.len()));
                    }
                    start += piece.len();
                }
                if parts.is_empty() {
                    parts.push(*range);
                }
                parts
            };
            let split = selection.clone().split_lines(&text);
            let parts: Vec<Range> = selection.ranges().iter().flat_map(model).collect();
            prop_assert_eq!(split.ranges(), &parts[..]);
            let primary = selection.primary();
            let of_primary = model(&primary);
            let at_head = if primary.head < primary.anchor { 0 } else { of_primary.len() - 1 };
            prop_assert_eq!(split.primary(), of_primary[at_head]);
        }

        #[test]
        fn extend_to_lines_covers_the_lines_of_each_range(
            (string, (primary, others)) in strategies::text_and_ranges(),
        ) {
            let text = Text::from(string.as_str());
            // An offset inside a `\r\n` is on the line the break ends.
            let starts: Vec<usize> = std::iter::once(0)
                .chain(string.match_indices('\n').map(|(at, _)| at + 1))
                .collect();
            let model = |range: Range| {
                let start = starts.iter().copied().rfind(|&start| start <= range.start()).unwrap();
                let end = if !range.is_empty() && starts.contains(&range.end()) {
                    range.end()
                } else {
                    let next = starts.iter().copied().find(|&start| start > range.end());
                    next.unwrap_or(string.len())
                };
                range.spanning(start, end)
            };
            let selection = Selection::new(primary, others);
            let expected = Selection::new(
                model(selection.primary()),
                selection.ranges().iter().map(|&range| model(range)),
            );
            prop_assert_eq!(selection.extend_to_lines(&text), expected);
        }

        #[test]
        fn trim_keeps_each_range_from_its_first_to_its_last_solid_grapheme(
            (string, (primary, others)) in strategies::text_and_ranges(),
        ) {
            let text = Text::from(string.as_str());
            // Ranges on grapheme boundaries, as a preset keeps them.
            let selection = Selection::new(primary, others).for_normal_mode(&text);
            let model = |range: &Range| {
                let mut solid = string[range.start()..range.end()]
                    .grapheme_indices(true)
                    .filter(|(_, grapheme)| !grapheme.chars().all(char::is_whitespace));
                let (first, grapheme) = solid.next()?;
                let (last, grapheme) = solid.next_back().unwrap_or((first, grapheme));
                let start = range.start();
                Some(range.spanning(start + first, start + last + grapheme.len()))
            };
            let ranges = selection.ranges();
            let kept: Vec<Range> = ranges.iter().filter_map(model).collect();
            let trimmed = selection.clone().trim(&text);
            if kept.is_empty() {
                prop_assert_eq!(trimmed, selection);
            } else {
                prop_assert_eq!(trimmed.ranges(), &kept[..]);
                // The primary range, the next one kept after it, or the last one before it.
                let (before, from_primary) = ranges.split_at(selection.primary_index());
                let primary = from_primary
                    .iter()
                    .find_map(model)
                    .or_else(|| before.iter().rev().find_map(model));
                prop_assert_eq!(Some(trimmed.primary()), primary);
            }
        }

        #[test]
        fn yank_copies_each_range((string, (primary, others)) in strategies::text_and_ranges()) {
            let selection = Selection::new(primary, others);
            let slices: Vec<&str> = selection
                .ranges()
                .iter()
                .map(|range| &string[range.start()..range.end()])
                .collect();
            prop_assert_eq!(selection.yank(&Text::from(string.as_str())), slices);
        }

        #[test]
        fn new_sorts_and_merges((primary, others) in primary_and_others()) {
            let selection = Selection::new(primary, others.iter().copied());
            let merged = selection.ranges();
            for pair in merged.windows(2) {
                prop_assert!(pair[0].start() < pair[1].start() && pair[0].end() <= pair[1].start());
            }
            // Every range lands inside one merged range, and the primary inside the primary.
            let inputs: Vec<Range> = std::iter::once(primary).chain(others).collect();
            for range in &inputs {
                let inside = |merged: &Range| {
                    merged.start() <= range.start() && range.end() <= merged.end()
                };
                prop_assert!(merged.iter().any(inside));
            }
            let primary_range = selection.primary();
            prop_assert!(primary_range.start() <= primary.start());
            prop_assert!(primary.end() <= primary_range.end());
            // Merging covers no byte that no range covered.
            for merged in merged {
                for byte in merged.start()..merged.end() {
                    let covered = inputs.iter().any(|range| (range.start()..range.end()).contains(&byte));
                    prop_assert!(covered, "{byte} in {merged:?} from {inputs:?}");
                }
            }
        }

        #[test]
        fn map_matches_mapping_each_range_alone(
            (text, changes, (primary, others)) in strategies::text_changes_and_ranges(),
        ) {
            let original = Text::from(text.as_str());
            let set = ChangeSet::new(&original, changes.iter().cloned()).unwrap();
            let selection = Selection::new(primary, others);
            let alone = |range: Range| Selection::single(range).map(&set).primary();
            let expected = Selection::new(
                alone(selection.primary()),
                selection.ranges().iter().map(|&range| alone(range)),
            );
            prop_assert_eq!(selection.map(&set), expected);
        }

        #[test]
        fn map_keeps_the_text_of_untouched_ranges(
            (text, changes, (primary, others)) in strategies::text_changes_and_ranges(),
        ) {
            let changed = strategies::apply(&text, &changes);
            for range in std::iter::once(primary).chain(others) {
                let touched = changes.iter().any(|(change, _)| {
                    change.end > range.start() && change.start < range.end()
                });
                if touched {
                    continue;
                }
                let after = mapped(&text, &changes, range);
                prop_assert_eq!(
                    &changed[after.start()..after.end()],
                    &text[range.start()..range.end()],
                );
            }
        }

        #[test]
        fn for_normal_mode_widens_ranges_to_whole_graphemes(
            (string, (primary, others)) in strategies::text_and_ranges(),
        ) {
            let text = Text::from(string.as_str());
            let boundaries = strategies::grapheme_boundaries(&string);
            let empty_line_at_end = string.is_empty() || string.ends_with('\n');
            // The generated ranges, and an empty range at the end, which they rarely include.
            let at_end = Selection::single(Range::point(string.len()));
            for selection in [Selection::new(primary, others), at_end] {
                let normal = selection.clone().for_normal_mode(&text);
                for range in normal.ranges() {
                    prop_assert!(boundaries.contains(&range.anchor));
                    prop_assert!(boundaries.contains(&range.head));
                    prop_assert!(
                        !range.is_empty() || (range.head == string.len() && empty_line_at_end),
                        "{:?} is empty", range
                    );
                }
                let model = |range| in_normal_mode(&string, &boundaries, range);
                let expected = Selection::new(
                    model(selection.primary()),
                    selection.ranges().iter().map(|&range| model(range)),
                );
                prop_assert_eq!(normal, expected);
            }
        }

        #[test]
        fn extend_to_spans_from_the_anchor_grapheme_to_the_target(
            (string, range, target) in text_range_and_target(),
        ) {
            let text = Text::from(string.as_str());
            let boundaries = strategies::grapheme_boundaries(&string);
            let len = string.len();
            let next = |offset| boundaries.iter().copied().find(|&b| b > offset).unwrap_or(len);
            let prev = |offset| boundaries.iter().copied().rev().find(|&b| b < offset).unwrap_or(0);
            // The anchor's grapheme: after the anchor, or before it in a range pointing backward.
            let (anchor_start, anchor_end) = if range.head < range.anchor {
                (prev(range.anchor), range.anchor)
            } else {
                (range.anchor, next(range.anchor))
            };
            let extended = range.extend_to(&text, target);
            // It covers both graphemes and those between, nothing more.
            prop_assert_eq!(extended.start(), anchor_start.min(target));
            prop_assert_eq!(extended.end(), anchor_end.max(next(target)));
            // The block cursor is on the target, or on the last grapheme for the end of the text.
            let cursor = if target == len && !extended.is_empty() { prev(len) } else { target };
            prop_assert_eq!(extended.cursor(&text), cursor);
            // It points backward only toward a target before the anchor's grapheme.
            prop_assert_eq!(extended.head < extended.anchor, target < anchor_start);
        }
    }
}
