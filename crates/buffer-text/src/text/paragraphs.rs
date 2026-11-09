use std::iter;
use std::ops::Range;

use super::objects::{Extent, object_span};
use crate::Text;

impl Text {
    /// Moves `count` paragraphs forward from `offset`, as Vim's `}` does (ADR 0035): past any
    /// blank lines and the paragraph after them, to the start of the blank line that follows it.
    /// A paragraph is a run of lines that aren't blank, and a blank line is empty or all
    /// whitespace, the blank of [`first_non_blank`](Self::first_non_blank). Returns the end of the
    /// text if no blank line follows, and a count that overshoots stops there; a count of 0
    /// returns `offset`. Vim passes its cursor's start.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn next_blank_line(&self, offset: usize, count: usize) -> usize {
        self.paragraph_motion(offset, count, true, true)
    }

    /// Moves `count` paragraphs back from `offset`, as Vim's `{` does: back past any blank lines
    /// and the paragraph before them, to the start of the blank line before it, or to the start
    /// of the text. It never stops on `offset`'s own line, except at the start of the text. See
    /// [`next_blank_line`](Self::next_blank_line).
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn prev_blank_line(&self, offset: usize, count: usize) -> usize {
        self.paragraph_motion(offset, count, false, true)
    }

    /// Moves `count` paragraphs forward from `offset`, as Helix's `]p` does: past the rest of the
    /// paragraph `offset` is in, if any, and the blank lines after it, to the start of the next
    /// paragraph, the first one that starts after `offset`. Returns the end of the text if there
    /// is none. Helix passes its cursor's end: after a `]p`, its cursor is on the line break
    /// before the paragraph it moved to, so the next `]p` moves on. See
    /// [`next_blank_line`](Self::next_blank_line).
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn next_paragraph_start(&self, offset: usize, count: usize) -> usize {
        self.paragraph_motion(offset, count, true, false)
    }

    /// Moves `count` paragraphs back from `offset`, as Helix's `[p` does: to the start of the last
    /// paragraph that starts before `offset`, or to the start of the text. From inside the first
    /// line of a paragraph, that is the line's start. Helix passes its cursor's start. See
    /// [`next_blank_line`](Self::next_blank_line).
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn prev_paragraph_start(&self, offset: usize, count: usize) -> usize {
        let line = self.line_of(offset);
        let mut lines = self.walk_lines(line, false);
        // Only paragraphs that start before `offset` count.
        if offset == self.line_start(line) {
            lines.next();
        }

        let mut target = offset;
        for _ in 0..count {
            // Back past any blank lines, then a paragraph, to the blank line before it, where `{`
            // stops: the paragraph starts on the line after.
            let Some(line) = past_runs(&mut lines, true) else {
                return 0;
            };
            target = self.line_start(line + 1);
        }
        target
    }

    /// The paragraph object at `offset`, as Vim's `ip` and `ap` and Helix's `mip` and `map`
    /// select it (ADR 0038). `Inside` is the run of lines `offset` is on, a paragraph or blank
    /// lines, from its first line's start to the start of the line after it. `Around` adds the
    /// blank lines after it, or before it if none follow; from blank lines, it takes them and the
    /// paragraph after. A count takes that many runs: `Inside` counts runs of blank lines,
    /// `Around` doesn't. A count that overshoots stops at the end of the text; a count of 0
    /// returns `None`. The empty line after a final line break, right after a paragraph, is no
    /// run: the paragraph before it has no blank lines after it, and there is no object on it.
    /// Vim's linewise objects are the Vim preset's.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn paragraph_object(
        &self,
        offset: usize,
        count: usize,
        extent: Extent,
    ) -> Option<Range<usize>> {
        let line = self.line_of(offset);
        let (first, _) = self.run_start(line);
        let mut lines = self.walk_lines(line, true).peekable();
        let mut start = first;
        let units = iter::from_fn(|| {
            let &(_, blank) = lines.peek()?;
            while lines.next_if(|&(_, is_blank)| is_blank == blank).is_some() {}
            let end = lines.peek().map_or(self.line_count(), |&(line, _)| line);
            let unit = self.line_start(start)..self.line_start(end);
            start = end;
            // Only the empty line after a final line break makes an empty run, the last one.
            (!unit.is_empty()).then_some((unit, blank))
        });
        let blanks_before = || {
            let (start, blank) = self.run_start(first.checked_sub(1)?);
            blank.then(|| self.line_start(start))
        };
        object_span(units, blanks_before, count, extent)
    }

    /// The lines of the paragraph `line` is on, or `None` if `line` is blank. Sentence objects and
    /// pairs of chars stay inside it (ADR 0038).
    pub(super) fn paragraph_lines(&self, line: usize) -> Option<Range<usize>> {
        let (first, blank) = self.run_start(line);
        if blank {
            return None;
        }
        let end = self
            .walk_lines(line, true)
            .find(|&(_, blank)| blank)
            .map_or(self.line_count(), |(line, _)| line);
        Some(first..end)
    }

    /// The first line of the run `line` is in, of blank lines or of lines that aren't, and whether
    /// they are blank.
    fn run_start(&self, line: usize) -> (usize, bool) {
        let mut lines = self.walk_lines(line, false).peekable();
        let blank = lines.peek().is_some_and(|&(_, blank)| blank);
        let start = lines
            .find(|&(_, is_blank)| is_blank != blank)
            .map_or(0, |(line, _)| line + 1);
        (start, blank)
    }

    /// Moves `count` steps from `offset`, forward or back. Each step walks past any blank lines,
    /// if `blank_first`, or any lines that aren't, then past lines of the other kind, and stops at
    /// the start of the line after them, or at the end or the start of the text.
    fn paragraph_motion(
        &self,
        offset: usize,
        count: usize,
        forward: bool,
        blank_first: bool,
    ) -> usize {
        let mut lines = self.walk_lines(self.line_of(offset), forward);
        let mut target = offset;
        for _ in 0..count {
            let Some(line) = past_runs(&mut lines, blank_first) else {
                return if forward { self.len() } else { 0 };
            };
            target = self.line_start(line);
        }
        target
    }

    /// The lines from `line` to the last one, or back to the first, each with whether it is
    /// blank. It reads each char once, so a walk costs O(length), however short its lines are.
    fn walk_lines(&self, line: usize, forward: bool) -> impl Iterator<Item = (usize, bool)> {
        // A walk back starts before `line`'s break, which it would read as an empty line.
        let (range, count) = if forward {
            (self.line_start(line)..self.len(), self.line_count() - line)
        } else {
            (0..self.line_end(line), line + 1)
        };
        let mut chars = self.chunks_in(range).flat_map(|(_, chunk)| chunk.chars());
        (0..count).map(move |step| {
            let blank = if forward {
                read_line(chars.by_ref())
            } else {
                read_line(chars.by_ref().rev())
            };
            (if forward { line + step } else { line - step }, blank)
        })
    }
}

/// Reads a line from `chars`, and the `\n` that ends it if there is one, and returns whether the
/// line is blank. The `\r` of a `\r\n` is whitespace, and so is a lone `\r`, which ends no line.
#[expect(
    clippy::unnecessary_fold,
    reason = "`all` stops at a char that isn't whitespace, so the next read would start mid-line"
)]
fn read_line(chars: impl Iterator<Item = char>) -> bool {
    chars
        .take_while(|&ch| ch != '\n')
        .fold(true, |blank, ch| blank && ch.is_whitespace())
}

/// Walks `lines` past any blank lines, if `blank`, or any lines that aren't, then past lines of
/// the other kind, and takes and returns the line after them: `None` if the lines run out first.
/// That line is of the first kind, which the next walk starts past anyway.
fn past_runs(lines: &mut impl Iterator<Item = (usize, bool)>, blank: bool) -> Option<usize> {
    lines.find(|&(_, is_blank)| is_blank != blank)?;
    let (line, _) = lines.find(|&(_, is_blank)| is_blank == blank)?;
    Some(line)
}

#[cfg(test)]
mod tests {
    use std::iter;

    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::sample::select;

    use super::*;
    use crate::Extent::{Around, Inside};
    use crate::strategies;
    use crate::text::objects;

    /// Paragraphs `a b`, `c`, and `d`. Lines 0 `a`, 1 `b`, 2 empty, 3 two spaces, 4 `c`, 5 empty,
    /// and 6 `d` start at 0, 2, 4, 5, 8, 10, and 11; the text ends at 12.
    const PARAGRAPHS: &str = "a\nb\n\n  \nc\n\nd";

    #[test]
    fn vim_stops_on_the_blank_line_past_a_paragraph() {
        let text = Text::from(PARAGRAPHS);
        // `}` from `a` stops on the empty line, then past the line of spaces and `c`, on the line
        // after `c`. Past `d`, at the end of the text.
        assert_eq!(text.next_blank_line(0, 1), 4);
        assert_eq!(text.next_blank_line(4, 1), 10);
        assert_eq!(text.next_blank_line(10, 1), 12);
        assert_eq!(text.next_blank_line(0, 2), 10);
        assert_eq!(text.next_blank_line(0, 5), 12);
        // `{` from `d` stops on the line after `c`, then on the line of spaces, the blank line
        // right before `c`, then at the start of the text.
        assert_eq!(text.prev_blank_line(11, 1), 10);
        assert_eq!(text.prev_blank_line(10, 1), 5);
        assert_eq!(text.prev_blank_line(5, 1), 0);
        assert_eq!(text.prev_blank_line(12, 2), 5);
        // From inside the line of spaces, `{` doesn't stop at the line's own start.
        assert_eq!(text.prev_blank_line(6, 1), 0);
    }

    #[test]
    fn helix_stops_on_paragraph_starts() {
        let text = Text::from(PARAGRAPHS);
        // `]p` from `a`, and from the blank lines after it, goes to `c`, then to `d`, then to the
        // end of the text.
        assert_eq!(text.next_paragraph_start(0, 1), 8);
        assert_eq!(text.next_paragraph_start(5, 1), 8);
        assert_eq!(text.next_paragraph_start(8, 1), 11);
        assert_eq!(text.next_paragraph_start(11, 1), 12);
        assert_eq!(text.next_paragraph_start(0, 9), 12);
        // `[p` from inside `c`'s line goes to its start, and from there to `a`.
        assert_eq!(text.prev_paragraph_start(9, 1), 8);
        assert_eq!(text.prev_paragraph_start(8, 1), 0);
        assert_eq!(text.prev_paragraph_start(12, 2), 8);
        // From inside `b`, the second line of its paragraph, to `a`.
        assert_eq!(text.prev_paragraph_start(3, 1), 0);
    }

    #[test]
    fn blank_lines_are_empty_or_all_whitespace() {
        // A tab, a no-break space, and the ideographic space are blank; a space with a combining
        // accent isn't, so `b` and `c` are one paragraph. Lines start at 0, 2, 9, 11, and 15.
        let text = Text::from("a\n\t\u{a0}\u{3000}\nb\n \u{301}\nc");
        assert_eq!(text.next_blank_line(0, 1), 2);
        assert_eq!(text.next_blank_line(2, 1), 16);
        assert_eq!(text.next_paragraph_start(0, 1), 9);
        assert_eq!(text.next_paragraph_start(9, 1), 16);
        // `\r\n` breaks lines, and a final line break leaves an empty line, which is blank. Lines
        // start at 0, 3, 5, and 8.
        let text = Text::from("a\r\n\r\nb\r\n");
        assert_eq!(text.next_blank_line(3, 1), 8);
        assert_eq!(text.prev_blank_line(8, 1), 3);
        assert_eq!(text.prev_paragraph_start(8, 1), 5);
        // From inside the `\r\n` after `b`, the offset is on `b`'s line.
        assert_eq!(text.prev_paragraph_start(7, 1), 5);
        assert_eq!(text.next_blank_line(7, 1), 8);
    }

    #[test]
    fn paragraph_objects_take_runs_of_lines() {
        // `a b` 0..4, blank lines 4..8, `c` 8..10, an empty line 10..11, `d` 11..12.
        let text = Text::from(PARAGRAPHS);
        // `Inside` is the run of lines at the offset, of either kind; a count counts both.
        assert_eq!(text.paragraph_object(1, 1, Inside), Some(0..4));
        assert_eq!(text.paragraph_object(6, 1, Inside), Some(4..8));
        assert_eq!(text.paragraph_object(0, 2, Inside), Some(0..8));
        assert_eq!(text.paragraph_object(0, 9, Inside), Some(0..12));
        // `Around` adds the blank lines after, or before `d`, the last paragraph.
        assert_eq!(text.paragraph_object(0, 1, Around), Some(0..8));
        assert_eq!(text.paragraph_object(11, 1, Around), Some(10..12));
        // The empty line after a final line break isn't blank lines after `b`, and holds no
        // object. After blank lines, it is one of them.
        let text = Text::from("a\n\nb\n");
        assert_eq!(text.paragraph_object(3, 1, Around), Some(2..5));
        assert_eq!(text.paragraph_object(5, 1, Inside), None);
        assert_eq!(
            Text::from("a\n\n").paragraph_object(3, 1, Inside),
            Some(2..3)
        );
        let text = Text::from(PARAGRAPHS);
        // From blank lines, it takes them and the paragraph after; a count skips blank runs.
        assert_eq!(text.paragraph_object(4, 1, Around), Some(4..10));
        assert_eq!(text.paragraph_object(0, 2, Around), Some(0..11));
        assert_eq!(text.paragraph_object(0, 0, Inside), None);
    }

    /// Pieces of lines: Unicode whitespace, a lone `\r`, which breaks no line, and chars that
    /// aren't blank, one of them a combining accent on a space.
    const PIECES: &[&str] = &[" ", "\t", "\u{a0}", "\u{3000}", "\r", "a", "日", " \u{301}"];

    /// Lines of blanks and of chars that aren't, now and then repeated to span rope chunks, and
    /// an offset in them: a char boundary, most often one where a line starts, where `[p` skips
    /// the paragraph that starts there.
    fn lines_and_offset() -> impl Strategy<Value = (String, usize)> {
        let lines = || {
            let piece = prop_oneof![3 => select(PIECES), 2 => select(&["\n", "\r\n"][..])];
            vec(piece, 0..32).prop_map(|pieces| pieces.concat())
        };
        prop_oneof![
            9 => lines(),
            1 => lines().prop_map(|lines| lines.repeat(80)),
        ]
        .prop_flat_map(|lines| {
            let starts = iter::once(0).chain(lines.match_indices('\n').map(|(at, _)| at + 1));
            let offset = prop_oneof![
                2 => select(starts.collect::<Vec<_>>()),
                1 => select(strategies::char_boundaries(&lines)),
            ];
            (Just(lines), offset)
        })
    }

    /// `string`'s lines: where each starts, and whether it is blank. A `\r` before a `\n` is
    /// whitespace, so it doesn't change whether its line is blank.
    fn lines(string: &str) -> Vec<(usize, bool)> {
        let mut lines = Vec::new();
        let mut start = 0;
        for piece in string.split('\n') {
            lines.push((start, piece.chars().all(char::is_whitespace)));
            start += piece.len() + 1;
        }
        lines
    }

    /// What a paragraph motion does on `string`, worked out from where its lines start and
    /// whether each is blank: motion 0 is `}`, 1 `{`, 2 `]p`, and 3 `[p`.
    fn model(string: &str, (offset, count): (usize, usize), motion: usize) -> usize {
        let lines = lines(string);
        let line = lines
            .iter()
            .rposition(|&(start, _)| start <= offset)
            .unwrap();
        let blank = |line: usize| lines[line].1;
        let first = |line: usize| !blank(line) && (line == 0 || blank(line - 1));
        let (stops, edge): (Vec<usize>, usize) = match motion {
            // Blank lines right after a line that isn't, after `offset`'s line.
            0 => (
                (line + 1..lines.len())
                    .filter(|&l| blank(l) && !blank(l - 1))
                    .collect(),
                string.len(),
            ),
            // Blank lines right before a line that isn't, before `offset`'s line, nearest first.
            1 => (
                (0..line)
                    .rev()
                    .filter(|&l| blank(l) && !blank(l + 1))
                    .collect(),
                0,
            ),
            // First lines of paragraphs after `offset`'s line.
            2 => (
                (line + 1..lines.len()).filter(|&l| first(l)).collect(),
                string.len(),
            ),
            // First lines of paragraphs that start before `offset`, nearest first.
            _ => (
                (0..lines.len())
                    .rev()
                    .filter(|&l| lines[l].0 < offset && first(l))
                    .collect(),
                0,
            ),
        };
        match count {
            0 => offset,
            _ => stops.get(count - 1).map_or(edge, |&l| lines[l].0),
        }
    }

    /// What `paragraph_object` returns on `string`, worked out from all its runs of lines.
    fn object_model(
        string: &str,
        (offset, count): (usize, usize),
        extent: Extent,
    ) -> Option<Range<usize>> {
        let lines = lines(string);
        let mut units: Vec<(Range<usize>, bool)> = Vec::new();
        for (index, &(start, blank)) in lines.iter().enumerate() {
            let end = lines.get(index + 1).map_or(string.len(), |&(next, _)| next);
            match units.last_mut() {
                Some((unit, last)) if *last == blank => unit.end = end,
                _ => units.push((start..end, blank)),
            }
        }
        // The empty line after a final line break, right after a paragraph, is no run.
        units.retain(|(unit, _)| !unit.is_empty());
        // An offset inside a `\r\n` is on the line the break ends.
        let line_start = lines
            .iter()
            .rfind(|&&(start, _)| start <= offset)
            .unwrap()
            .0;
        // After blank lines, the empty line after a final line break is in their run.
        let at = match units.iter().position(|(unit, _)| unit.end > line_start) {
            Some(at) => at,
            None if units.last()?.1 => units.len() - 1,
            None => return None,
        };
        objects::model(&units, at, count, extent)
    }

    proptest! {
        #[test]
        fn paragraph_motions_match_a_model_of_the_lines(
            (string, offset) in lines_and_offset(),
            // Mostly one step: a count past the last paragraph hides where a step stopped.
            count in prop_oneof![Just(1), 0..6_usize],
            motion in 0..4_usize,
        ) {
            let text = Text::from(string.as_str());
            let moved = match motion {
                0 => text.next_blank_line(offset, count),
                1 => text.prev_blank_line(offset, count),
                2 => text.next_paragraph_start(offset, count),
                _ => text.prev_paragraph_start(offset, count),
            };
            prop_assert_eq!(moved, model(&string, (offset, count), motion));
        }

        #[test]
        fn paragraph_objects_match_a_model_of_the_lines(
            (string, offset) in lines_and_offset(),
            count in prop_oneof![Just(1), 0..6_usize],
            around in any::<bool>(),
        ) {
            let text = Text::from(string.as_str());
            let extent = if around { Around } else { Inside };
            prop_assert_eq!(
                text.paragraph_object(offset, count, extent),
                object_model(&string, (offset, count), extent)
            );
        }
    }
}
