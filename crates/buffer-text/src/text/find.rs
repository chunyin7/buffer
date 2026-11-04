use crate::{LineEdges, Text};

impl Text {
    /// Finds the `count`-th grapheme after `offset` whose first char `matches`, as `f` does
    /// (ADR 0035), and returns where it starts. With [`LineEdges::Stop`] it searches the rest of
    /// the line's content, as Vim does; with [`LineEdges::Cross`], the rest of the text, as Helix
    /// does, where a line break is a grapheme too. Returns `None` if fewer graphemes match, so a
    /// count that overshoots fails, as in both editors; a count of 0 returns `offset`.
    ///
    /// A grapheme matches by its first char: `'e'` finds a decomposed `é`, and `'👍'` finds `👍🏽`.
    /// The grapheme at `offset` doesn't count, so a search from a match finds the next one. The
    /// presets build `t` and their selections from the match and the grapheme boundaries around
    /// it; Helix's `t` searches from its cursor's end, so that repeating it moves on.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn find_next(
        &self,
        offset: usize,
        count: usize,
        edges: LineEdges,
        matches: impl Fn(char) -> bool,
    ) -> Option<usize> {
        self.find(offset, count, edges, true, matches)
    }

    /// Finds the `count`-th grapheme before `offset` whose first char `matches`, as `F` does, and
    /// returns where it starts. With [`LineEdges::Stop`] it searches back to the start of the
    /// line, and with [`LineEdges::Cross`] to the start of the text. See
    /// [`find_next`](Self::find_next).
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn find_prev(
        &self,
        offset: usize,
        count: usize,
        edges: LineEdges,
        matches: impl Fn(char) -> bool,
    ) -> Option<usize> {
        self.find(offset, count, edges, false, matches)
    }

    fn find(
        &self,
        offset: usize,
        count: usize,
        edges: LineEdges,
        forward: bool,
        matches: impl Fn(char) -> bool,
    ) -> Option<usize> {
        // The line's content with `Stop`, so an offset inside a `\r\n` doesn't find the `\r`.
        let (start, end) = match edges {
            LineEdges::Stop => {
                let line = self.line_of(offset);
                (self.line_start(line), self.line_end(line))
            }
            LineEdges::Cross => {
                // In the other branch, `line_of` checks the offset.
                assert!(
                    self.is_char_boundary(offset),
                    "offset {offset} is inside a char"
                );
                (0, self.len())
            }
        };
        if count == 0 {
            return Some(offset);
        }

        // `offset` can be past `end` when it is inside a `\r\n` and `end` is the line's end.
        let from = offset.min(end);
        let range = if forward { from..end } else { start..from };
        // In each chunk, the chars that match and start a grapheme, other than the one at `from`.
        // Only a char that matches pays for the grapheme check, which its chunk usually answers.
        let mut found = self.chunks_in(range).flat_map(|(chunk_start, chunk)| {
            chunk
                .match_indices(&matches)
                .map(move |(index, _)| chunk_start + index)
                .filter(move |&position| {
                    position != from && self.is_grapheme_boundary_in(chunk, chunk_start, position)
                })
        });
        if forward {
            found.nth(count - 1)
        } else {
            found.nth_back(count - 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::sample::select;
    use unicode_segmentation::UnicodeSegmentation;

    use super::*;
    use crate::LineEdges::{Cross, Stop};
    use crate::strategies;

    /// A predicate for one char.
    fn is(ch: char) -> impl Fn(char) -> bool {
        move |c| c == ch
    }

    #[test]
    fn finds_stay_on_the_line_or_cross_it() {
        let text = Text::from("f(a) (b)\n(c)");
        // From `f`: Vim's `f(` and `2f(`. The third `(` is on the next line.
        assert_eq!(text.find_next(0, 1, Stop, is('(')), Some(1));
        assert_eq!(text.find_next(0, 2, Stop, is('(')), Some(5));
        assert_eq!(text.find_next(0, 3, Stop, is('(')), None);
        // Helix's crosses the line break.
        assert_eq!(text.find_next(0, 3, Cross, is('(')), Some(9));
        // The grapheme at the offset doesn't count, so a search from a match moves on.
        assert_eq!(text.find_next(1, 1, Stop, is('(')), Some(5));
        // Backward from the second line's `(`.
        assert_eq!(text.find_prev(9, 1, Stop, is('(')), None);
        assert_eq!(text.find_prev(9, 2, Cross, is('(')), Some(1));
        // From inside a `\r\n`, `Stop` searches only the line's content, which ends before it.
        let text = Text::from("a\r\nb");
        assert_eq!(text.find_prev(2, 1, Stop, is('\r')), None);
        assert_eq!(text.find_next(2, 1, Stop, is('b')), None);
    }

    #[test]
    fn finds_match_a_grapheme_by_its_first_char() {
        // A decomposed `é` starts with `e`, and its accent starts no grapheme. A thumbs-up with
        // a skin tone starts with the plain thumbs-up.
        let text = Text::from("cafe\u{301} 👍🏽!");
        assert_eq!(text.find_next(0, 1, Stop, is('e')), Some(3));
        assert_eq!(text.find_next(0, 1, Stop, is('\u{301}')), None);
        assert_eq!(text.find_next(0, 1, Stop, is('👍')), Some(7));
        // A bracket after a prepended char is inside that char's grapheme.
        let text = Text::from("a\u{600}(b)");
        assert_eq!(text.find_next(0, 1, Cross, is('(')), None);
        assert_eq!(text.find_prev(6, 1, Cross, is('(')), None);
        // A `\r\n` is one grapheme, and it starts with `\r`.
        let text = Text::from("a\r\nb");
        assert_eq!(text.find_next(0, 1, Cross, is('\n')), None);
        assert_eq!(text.find_next(0, 1, Cross, is('\r')), Some(1));
    }

    /// Chars to look for in `strategies::text`. Some start graphemes, some only continue them:
    /// the accent of `e\u{301}`, the second half of a flag, the `\n` of a `\r\n`, and the joiner
    /// and snowflake of the polar bear.
    const NEEDLES: &[char] = &[
        'a', 'x', ' ', '\n', '\r', 'e', '\u{301}', 'é', 'ß', '日', '🇩', '🇪', '🐻', '\u{200d}', '❄',
    ];

    /// What a find does on `string`, worked out from its graphemes and lines.
    fn model(
        string: &str,
        (offset, count): (usize, usize),
        (forward, edges): (bool, LineEdges),
        needles: &[char],
    ) -> Option<usize> {
        // What the find searches: the line's content, before a `\n` or `\r\n`, or the text.
        let (start, end) = match edges {
            LineEdges::Stop => {
                let start = string[..offset].rfind('\n').map_or(0, |at| at + 1);
                let end = match string[start..].find('\n') {
                    Some(at) if string[..start + at].ends_with('\r') => start + at - 1,
                    Some(at) => start + at,
                    None => string.len(),
                };
                (start, end)
            }
            LineEdges::Cross => (0, string.len()),
        };
        let matching: Vec<usize> = string
            .grapheme_indices(true)
            .filter(|(at, grapheme)| {
                (start..end).contains(at) && needles.contains(&grapheme.chars().next().unwrap())
            })
            .map(|(at, _)| at)
            .collect();
        let mut position = offset;
        for _ in 0..count {
            position = if forward {
                matching.iter().copied().find(|&at| at > position)?
            } else {
                matching.iter().copied().rev().find(|&at| at < position)?
            };
        }
        Some(position)
    }

    proptest! {
        #[test]
        fn finds_match_a_model_of_the_graphemes(
            (string, offset) in strategies::text().prop_flat_map(|text| {
                let offset = select(strategies::char_boundaries(&text));
                (Just(text), offset)
            }),
            count in 0..4_usize,
            forward in any::<bool>(),
            stop in any::<bool>(),
            needles in vec(select(NEEDLES), 1..3),
        ) {
            let text = Text::from(string.as_str());
            let edges = if stop { LineEdges::Stop } else { LineEdges::Cross };
            let matches = |c| needles.contains(&c);
            let found = if forward {
                text.find_next(offset, count, edges, matches)
            } else {
                text.find_prev(offset, count, edges, matches)
            };
            let expected = model(&string, (offset, count), (forward, edges), &needles);
            prop_assert_eq!(found, expected);
        }
    }
}
