use crate::Text;

/// The brackets [`Text::matching_bracket`] pairs, each opening one with its closing one
/// (ADR 0035). Not `<>` or curly quotes: in prose, `>` also starts quotes and arrows, and `’` is
/// also an apostrophe.
const PAIRS: [(char, char); 12] = [
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('（', '）'),
    ('［', '］'),
    ('｛', '｝'),
    ('「', '」'),
    ('『', '』'),
    ('【', '】'),
    ('〈', '〉'),
    ('《', '》'),
    ('〔', '〕'),
];

/// Whether `ch` is one of the brackets [`Text::matching_bracket`] pairs. The Vim preset passes it
/// to [`Text::find_next`] to find the bracket on the line that `%` jumps from.
pub fn is_bracket(ch: char) -> bool {
    partner(ch).is_some()
}

/// The bracket that pairs with `ch`, and whether `ch` opens the pair: `(')', true)` for `'('`.
fn partner(ch: char) -> Option<(char, bool)> {
    PAIRS.iter().find_map(|&(open, close)| {
        if ch == open {
            Some((close, true))
        } else if ch == close {
            Some((open, false))
        } else {
            None
        }
    })
}

impl Text {
    /// Finds the partner of the bracket at `offset`, as Vim's `%` and Helix's `mm` do
    /// (ADR 0035), and returns where it starts. A bracket is a grapheme whose first char is one of
    /// `()`, `[]`, `{}`, or the full-width and CJK `（）`, `［］`, `｛｝`, `「」`, `『』`, `【】`,
    /// `〈〉`, `《》`, and `〔〕`. An opening bracket's partner is the first closing bracket of its
    /// kind after it that closes no pair opened in between, on any line; a closing bracket's is
    /// the opening one found the same way back. Brackets of other kinds don't count, and quotes
    /// hide none.
    ///
    /// Returns `None` if no bracket starts at `offset`, or if it has no partner. A bracket char
    /// inside a grapheme starts no bracket, such as a `(` after a prepended char like U+0600
    /// ARABIC NUMBER SIGN. Vim's `%` jumps from the first bracket on the line at or after its
    /// cursor, which [`find_next`](Self::find_next) with [`is_bracket`] finds.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn matching_bracket(&self, offset: usize) -> Option<usize> {
        // `is_char_boundary` panics past the end of the text.
        assert!(
            self.is_char_boundary(offset),
            "offset {offset} is inside a char"
        );
        let bracket = self.rope.byte_slice(offset..).chars().next()?;
        let (partner, opens) = partner(bracket)?;
        if !self.is_grapheme_boundary(offset) {
            return None;
        }

        // The brackets of this kind that start graphemes, after `offset` or before it, each with
        // whether it is the same bracket as the one at `offset`, which opens a nested pair. As in
        // a find, only a bracket char pays for the grapheme check, which its chunk usually
        // answers.
        let range = if opens {
            offset + bracket.len_utf8()..self.len()
        } else {
            0..offset
        };
        let brackets = self.chunks_in(range).flat_map(|(chunk_start, chunk)| {
            chunk
                .match_indices([bracket, partner])
                .map(move |(index, found)| (chunk_start + index, found.starts_with(bracket)))
                .filter(move |&(position, _)| {
                    // A bracket starts a grapheme unless a prepended char such as U+0600 comes
                    // right before it (UAX #29, GB9b), and none is ASCII: after an ASCII byte,
                    // the check can be skipped. A test pins this against the Unicode tables.
                    let index = position - chunk_start;
                    (index > 0 && chunk.as_bytes()[index - 1].is_ascii())
                        || self.is_grapheme_boundary_in(chunk, chunk_start, position)
                })
        });
        if opens {
            unnested(brackets)
        } else {
            unnested(brackets.rev())
        }
    }
}

/// The first of `brackets` that closes no pair opened among them: where it starts. Each comes
/// with whether it opens a pair, which the next bracket that doesn't open one closes.
fn unnested(brackets: impl Iterator<Item = (usize, bool)>) -> Option<usize> {
    let mut depth = 0_usize;
    for (position, opens) in brackets {
        if opens {
            depth += 1;
        } else if depth == 0 {
            return Some(position);
        } else {
            depth -= 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::sample::select;
    use unicode_segmentation::UnicodeSegmentation;

    use super::*;
    use crate::strategies;

    #[test]
    fn partners_nest_by_kind_across_lines() {
        // `(` at 0, 3; `)` at 5, 9, 13; `[` at 7; `]` at 10. The line break is at 6.
        let text = Text::from("(a (b)\n[c)] d)");
        // The `(` at 3 nests in the one at 0, which the `)` on the next line closes: the `[`
        // around that `)` is another kind, so it doesn't count.
        assert_eq!(text.matching_bracket(0), Some(9));
        assert_eq!(text.matching_bracket(3), Some(5));
        assert_eq!(text.matching_bracket(7), Some(10));
        // Back from a closing bracket.
        assert_eq!(text.matching_bracket(9), Some(0));
        assert_eq!(text.matching_bracket(5), Some(3));
        assert_eq!(text.matching_bracket(10), Some(7));
        // The last `)` closes nothing; `a` and the end of the text are no brackets.
        assert_eq!(text.matching_bracket(13), None);
        assert_eq!(text.matching_bracket(1), None);
        assert_eq!(text.matching_bracket(14), None);
    }

    #[test]
    fn cjk_brackets_pair_and_angle_brackets_and_curly_quotes_dont() {
        // `「` at 0, `『` at 4, `』` at 8, `（` at 11, `）` at 15, and `」` at 18: three bytes each.
        let text = Text::from("「a『b』（c）」");
        assert_eq!(text.matching_bracket(0), Some(18));
        assert_eq!(text.matching_bracket(18), Some(0));
        assert_eq!(text.matching_bracket(4), Some(8));
        assert_eq!(text.matching_bracket(15), Some(11));
        let text = Text::from("<a> “b”");
        assert_eq!(text.matching_bracket(0), None);
        assert_eq!(text.matching_bracket(4), None);
    }

    #[test]
    fn a_bracket_is_a_grapheme_that_starts_with_one() {
        // A `(` with a combining accent is still a bracket.
        let text = Text::from("(\u{301}a)");
        assert_eq!(text.matching_bracket(0), Some(4));
        assert_eq!(text.matching_bracket(4), Some(0));
        // A `)` after a prepended char is inside that char's grapheme, so the `(` pairs with the
        // `)` after it, and the one inside is no bracket.
        let text = Text::from("(a\u{600})b)");
        assert_eq!(text.matching_bracket(0), Some(6));
        assert_eq!(text.matching_bracket(6), Some(0));
        assert_eq!(text.matching_bracket(4), None);
    }

    /// `matching_bracket` skips the grapheme check for a bracket after an ASCII char. This fails
    /// if a Unicode update ever joins an ASCII char to the bracket after it.
    #[test]
    fn every_bracket_after_an_ascii_char_starts_a_grapheme() {
        for ascii in (0..=0x7f_u8).map(char::from) {
            for bracket in PAIRS.iter().flat_map(|&(open, close)| [open, close]) {
                let text = Text::from(format!("{ascii}{bracket}"));
                assert!(text.is_grapheme_boundary(1), "{ascii:?} before {bracket:?}");
            }
        }
    }

    /// Pieces of text with brackets of three kinds: one with a combining accent, which is still
    /// a bracket, and two after a prepended char, which are inside its grapheme.
    const PIECES: &[&str] = &[
        "(", ")", "[", "]", "「", "」", "(\u{301}", "\u{600}(", "\u{600})", "a", " ", "\n", "\r\n",
        "日", "🇩🇪",
    ];

    /// Brackets among other text, now and then repeated to span rope chunks, and an offset in
    /// it: a char boundary, most often one where a bracket char is.
    fn text_and_offset() -> impl Strategy<Value = (String, usize)> {
        let text = || vec(select(PIECES), 0..32).prop_map(|pieces| pieces.concat());
        prop_oneof![
            9 => text(),
            1 => text().prop_map(|text| text.repeat(80)),
        ]
        .prop_flat_map(|text| {
            let brackets = text
                .char_indices()
                .filter(|&(_, ch)| is_bracket(ch))
                .map(|(at, _)| at)
                .chain([text.len()]);
            let offset = prop_oneof![
                3 => select(brackets.collect::<Vec<_>>()),
                1 => select(strategies::char_boundaries(&text)),
            ];
            (Just(text), offset)
        })
    }

    /// What `matching_bracket` returns on `string`, worked out by pairing its brackets from the
    /// start of the text: each closing bracket closes the last one of its kind still open.
    fn model(string: &str, offset: usize) -> Option<usize> {
        let mut still_open: Vec<Vec<usize>> = vec![Vec::new(); PAIRS.len()];
        let mut partners = HashMap::new();
        for (at, grapheme) in string.grapheme_indices(true) {
            let first = grapheme.chars().next().unwrap();
            for (open, &(opening, closing)) in still_open.iter_mut().zip(&PAIRS) {
                if first == opening {
                    open.push(at);
                } else if first == closing
                    && let Some(start) = open.pop()
                {
                    partners.insert(start, at);
                    partners.insert(at, start);
                }
            }
        }
        partners.get(&offset).copied()
    }

    proptest! {
        #[test]
        fn partners_match_a_model_that_pairs_every_bracket(
            (string, offset) in text_and_offset(),
        ) {
            let text = Text::from(string.as_str());
            prop_assert_eq!(text.matching_bracket(offset), model(&string, offset));
        }
    }
}
