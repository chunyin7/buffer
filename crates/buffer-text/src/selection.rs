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
