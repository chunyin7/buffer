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
    use crate::strategies;

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

    /// What a paragraph motion does on `string`, worked out from where its lines start and
    /// whether each is blank: motion 0 is `}`, 1 `{`, 2 `]p`, and 3 `[p`.
    fn model(string: &str, (offset, count): (usize, usize), motion: usize) -> usize {
        // A `\r` before a `\n` is whitespace, so it doesn't change whether its line is blank.
        let mut lines = Vec::new();
        let mut start = 0;
        for piece in string.split('\n') {
            lines.push((start, piece.chars().all(char::is_whitespace)));
            start += piece.len() + 1;
        }
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
    }
}
