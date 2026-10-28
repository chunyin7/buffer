use smallvec::{SmallVec, smallvec};

use crate::ChangeSet;
use crate::change::{Assoc, Mapper};

/// A span of text between an anchor and a head, as byte offsets.
///
/// It covers the bytes from [`start`](Self::start) up to, not including, [`end`](Self::end).
/// Extending it moves the head; the anchor stays put.
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

    /// Whether the ranges share a byte or start at the same offset.
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
        // A range inside replaced bytes maps past its own end; it then covers the new text.
        let (start, end) = (start.min(end), start.max(end));
        if self.anchor < self.head {
            Self::new(start, end)
        } else {
            Self::new(end, start)
        }
    }
}

/// One or more ranges, one of them primary.
///
/// The ranges are sorted and never overlap: ranges that share a byte or start at the same offset
/// merge into one.
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

    proptest! {
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
    }
}
