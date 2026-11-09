use std::collections::VecDeque;
use std::ops::Range;

use super::objects::Extent;
use crate::Text;

/// The brackets [`Text::matching_bracket`] and [`Text::enclosing_bracket_object`] pair, each
/// opening one with its closing one (ADR 0035, 0038). Not `<>` or `‘’`: in prose, `>` also
/// starts quotes and arrows, and `’` is also an apostrophe.
const PAIRS: [(char, char); 13] = [
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('“', '”'),
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

/// The ASCII brackets as a set of bits, so that most chars, which are ASCII, skip the search of
/// [`PAIRS`].
const ASCII_BRACKETS: u128 = {
    let mut set = 0_u128;
    let mut index = 0;
    while index < PAIRS.len() {
        let (open, close) = PAIRS[index];
        if open.is_ascii() {
            set |= 1 << open as u32;
        }
        if close.is_ascii() {
            set |= 1 << close as u32;
        }
        index += 1;
    }
    set
};

/// Whether `ch` is one of the brackets [`Text::matching_bracket`] pairs. The Vim preset passes it
/// to [`Text::find_next`] to find the bracket on the line that `%` jumps from.
pub fn is_bracket(ch: char) -> bool {
    if ch.is_ascii() {
        ASCII_BRACKETS & (1 << ch as u32) != 0
    } else {
        pair(ch).is_some()
    }
}

/// The pair of brackets that `ch` opens or closes.
fn pair(ch: char) -> Option<(char, char)> {
    PAIRS
        .into_iter()
        .find(|&(open, close)| ch == open || ch == close)
}

impl Text {
    /// Finds the partner of the bracket at `offset`, as Vim's `%` and Helix's `mm` do
    /// (ADR 0035), and returns where it starts. A bracket is a grapheme whose first char is one of
    /// `()`, `[]`, `{}`, `“”`, or the full-width and CJK `（）`, `［］`, `｛｝`, `「」`, `『』`,
    /// `【】`, `〈〉`, `《》`, and `〔〕`. An opening bracket's partner is the first closing bracket
    /// of its kind after it that closes no pair opened in between, on any line; a closing
    /// bracket's is the opening one found the same way back. Brackets of other kinds don't count,
    /// and quotes hide none.
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
        let (open, close) = pair(bracket)?;
        if !self.is_grapheme_boundary(offset) {
            return None;
        }

        // The brackets of this kind that start graphemes, after `offset` or before it, each with
        // whether it is the same bracket as the one at `offset`. As in a find, only a bracket
        // char pays for the grapheme check, which its chunk usually answers. Not through
        // `brackets_in`: with the `[open, close]` pattern, this scan takes 0.6 ms across 1 MB
        // of prose, and through a predicate 0.75 to 1 ms.
        let opens = bracket == open;
        let range = if opens {
            offset + bracket.len_utf8()..self.len()
        } else {
            0..offset
        };
        let mut brackets = self.chunks_in(range).flat_map(|(chunk_start, chunk)| {
            chunk
                .match_indices([open, close])
                .filter(move |&(index, _)| {
                    // A bracket starts a grapheme unless a prepended char such as U+0600 comes
                    // right before it (UAX #29, GB9b), and none is ASCII: after an ASCII byte,
                    // the check can be skipped. A test pins this against the Unicode tables.
                    (index > 0 && chunk.as_bytes()[index - 1].is_ascii())
                        || self.is_grapheme_boundary_in(chunk, chunk_start, chunk_start + index)
                })
                .map(move |(index, found)| (chunk_start + index, found.starts_with(bracket)))
        });
        // The partner is the first bracket that closes no pair opened in between: one like the
        // bracket at `offset` opens a pair, and any other closes one.
        let mut depth = 0_isize;
        let is_partner = |&(_, nests): &(usize, bool)| {
            depth += if nests { 1 } else { -1 };
            depth < 0
        };
        let partner = if opens {
            brackets.find(is_partner)
        } else {
            brackets.rfind(is_partner)
        };
        partner.map(|(position, _)| position)
    }

    /// The `count`-th pair of brackets that encloses `span`, as Vim's `i(` and `a(` and Helix's
    /// `mi(`, `ma(`, `mim`, and `mam` select it (ADR 0038). `kind` is either bracket of a pair,
    /// for that pair only, or `None` for any of them, the pairs of
    /// [`matching_bracket`](Self::matching_bracket). Pairs count outward, by where they close,
    /// on any line and without a limit. A bracket at the span's edge belongs to the pair it opens
    /// or closes.
    ///
    /// `Inside` lies between the brackets and may be empty; `Around` includes them. A pair whose
    /// object lies within `span` is skipped for the next one out, so a repeat grows the
    /// selection; a span of one grapheme or none is a cursor and skips none, so `i(` on the `x`
    /// of `(x)` gives `x`. Returns `None` if `kind` is no bracket, fewer pairs enclose `span`, or
    /// the count is 0. Vim's fallback to the next pair on the line is the Vim preset's.
    ///
    /// # Panics
    ///
    /// Panics if `span` ends past the end of the text, starts after its end, or either end is
    /// inside a char.
    pub fn enclosing_bracket_object(
        &self,
        span: Range<usize>,
        count: usize,
        kind: Option<char>,
        extent: Extent,
    ) -> Option<Range<usize>> {
        // `is_char_boundary` panics past the end of the text.
        assert!(
            span.start <= span.end
                && self.is_char_boundary(span.end)
                && self.is_char_boundary(span.start),
            "span {span:?} is reversed or splits a char"
        );
        let kind = match kind {
            Some(kind) => Some(pair(kind)?),
            None => None,
        };
        if count == 0 {
            return None;
        }
        let selected = move |ch| match kind {
            Some((open, close)) => ch == open || ch == close,
            None => is_bracket(ch),
        };

        // A span that starts on an opening bracket is inside the pair it opens. Scanning forward
        // from the bracket would count it as nesting and pass over its partner, so the scans split
        // past it instead, where the backward one finds it. Splitting past a char that is no
        // bracket of `kind`, or no bracket because it is inside a grapheme, changes nothing:
        // neither scan picks it up.
        let pivot = match self.rope.byte_slice(span.start..span.end).chars().next() {
            Some(ch) if pair(ch).is_some_and(|(open, _)| ch == open) => {
                self.grapheme_end(span.start)
            }
            _ => span.start,
        };
        // The brackets after `pivot` that close a pair it is in, and the ones before it that open
        // one, pair up by level: the n-th of a kind on one side with the n-th on the other.
        let closes = unclosed(self.brackets_in(pivot..self.len(), selected), true);
        let mut opens = unclosed(self.brackets_in(0..pivot, selected).rev(), false).fuse();
        // Opening brackets passed over while looking for one of another kind, by kind, nearest
        // first.
        let mut passed = vec![VecDeque::new(); PAIRS.len()];
        // The opening bracket of the next pair out of a kind: its n-th call for a kind returns
        // the n-th opening bracket of that kind before `pivot` that no bracket closes there.
        let mut next_open = |index: usize| {
            if let Some(open) = passed[index].pop_front() {
                return Some(open);
            }
            for (open, found) in opens.by_ref() {
                if found == index {
                    return Some(open);
                }
                passed[found].push_back(open);
            }
            None
        };
        let is_cursor = self.next_grapheme_boundary(span.start) >= span.end;

        closes
            .filter_map(|(close, index)| {
                let open = next_open(index)?;
                // A pair that closes inside the span doesn't enclose it.
                (self.grapheme_end(close) >= span.end).then(|| self.delimited(open, close, extent))
            })
            .filter(|object| is_cursor || !(span.start <= object.start && object.end <= span.end))
            .nth(count - 1)
    }

    /// The brackets in `range` that `selected` picks and that start graphemes, each with where it
    /// starts and its text, as [`matching_bracket`](Self::matching_bracket) finds them.
    fn brackets_in(
        &self,
        range: Range<usize>,
        selected: impl Fn(char) -> bool + Copy,
    ) -> impl DoubleEndedIterator<Item = (usize, &str)> {
        self.chunks_in(range).flat_map(move |(chunk_start, chunk)| {
            chunk
                .match_indices(selected)
                .filter(move |&(index, _)| {
                    // See `matching_bracket`.
                    (index > 0 && chunk.as_bytes()[index - 1].is_ascii())
                        || self.is_grapheme_boundary_in(chunk, chunk_start, chunk_start + index)
                })
                .map(move |(index, found)| (chunk_start + index, found))
        })
    }
}

/// The brackets in `brackets`, met going forward or back, that close a pair or open one where
/// `brackets` start: closing ones going forward, opening ones going back. Each comes with the
/// index of its pair in [`PAIRS`]; per pair, they come out level by level, nearest first.
fn unclosed<'a>(
    brackets: impl Iterator<Item = (usize, &'a str)>,
    forward: bool,
) -> impl Iterator<Item = (usize, usize)> {
    let mut depths = [0_usize; PAIRS.len()];
    brackets.filter_map(move |(position, found)| {
        let ch = found.chars().next()?;
        let index = PAIRS
            .iter()
            .position(|&(open, close)| ch == open || ch == close)?;
        // Going forward, opening brackets nest and closing ones unwind; going back, the reverse.
        let depth = &mut depths[index];
        if (ch == PAIRS[index].0) == forward {
            *depth += 1;
            None
        } else if *depth > 0 {
            *depth -= 1;
            None
        } else {
            Some((position, index))
        }
    })
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::sample::select;
    use unicode_segmentation::UnicodeSegmentation;

    use super::*;
    use crate::Extent::{Around, Inside};
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
    fn cjk_brackets_and_curly_quotes_pair_and_angle_brackets_dont() {
        // `「` at 0, `『` at 4, `』` at 8, `（` at 11, `）` at 15, and `」` at 18: three bytes each.
        let text = Text::from("「a『b』（c）」");
        assert_eq!(text.matching_bracket(0), Some(18));
        assert_eq!(text.matching_bracket(18), Some(0));
        assert_eq!(text.matching_bracket(4), Some(8));
        assert_eq!(text.matching_bracket(15), Some(11));
        let text = Text::from("<a> “b”");
        assert_eq!(text.matching_bracket(0), None);
        assert_eq!(text.matching_bracket(4), Some(8));
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

    #[test]
    fn enclosing_bracket_objects_take_the_pair_around_a_cursor() {
        // `(` at 1 and 5, `)` at 9 and 15, `[` at 12, `]` at 14.
        let text = Text::from("f(a, (b c), [d])");
        // From `b`, the inner pair, then the outer one.
        assert_eq!(
            text.enclosing_bracket_object(6..7, 1, Some('('), Inside),
            Some(6..9)
        );
        assert_eq!(
            text.enclosing_bracket_object(6..7, 1, Some(')'), Around),
            Some(5..10)
        );
        assert_eq!(
            text.enclosing_bracket_object(6..7, 2, Some('('), Inside),
            Some(2..15)
        );
        assert_eq!(
            text.enclosing_bracket_object(6..7, 3, Some('('), Inside),
            None
        );
        // A bracket at the cursor belongs to the pair it opens or closes.
        assert_eq!(
            text.enclosing_bracket_object(5..6, 1, Some('('), Inside),
            Some(6..9)
        );
        assert_eq!(
            text.enclosing_bracket_object(9..10, 1, Some('('), Inside),
            Some(6..9)
        );
        // Without a kind, the nearest pair of any kind; with another kind, none.
        assert_eq!(
            text.enclosing_bracket_object(13..14, 1, None, Inside),
            Some(13..14)
        );
        assert_eq!(
            text.enclosing_bracket_object(13..14, 2, None, Inside),
            Some(2..15)
        );
        assert_eq!(
            text.enclosing_bracket_object(6..7, 1, Some('['), Inside),
            None
        );
        assert_eq!(
            text.enclosing_bracket_object(6..7, 1, Some('x'), Inside),
            None
        );
        // A span across a pair's edge is in the pair around both.
        assert_eq!(
            text.enclosing_bracket_object(4..13, 1, None, Inside),
            Some(2..15)
        );
        // The cursor's own grapheme can be the whole inside, or the pair empty.
        let text = Text::from("(x) ()");
        assert_eq!(
            text.enclosing_bracket_object(1..2, 1, None, Inside),
            Some(1..2)
        );
        assert_eq!(
            text.enclosing_bracket_object(4..5, 1, None, Inside),
            Some(5..5)
        );
    }

    #[test]
    fn a_repeat_grows_the_selection() {
        let text = Text::from("[((x))]");
        // `Inside`: `x`, then `(x)`, the outer pair's inside, then `((x))`.
        assert_eq!(
            text.enclosing_bracket_object(3..4, 1, None, Inside),
            Some(3..4)
        );
        assert_eq!(
            text.enclosing_bracket_object(3..4, 1, Some('('), Inside),
            Some(3..4)
        );
        assert_eq!(
            text.enclosing_bracket_object(2..5, 1, None, Inside),
            Some(1..6)
        );
        assert_eq!(
            text.enclosing_bracket_object(2..5, 1, Some('('), Inside),
            None
        );
        // `Around`: `(x)`, then `((x))`. From the inside, the same pair's whole.
        assert_eq!(
            text.enclosing_bracket_object(2..5, 1, Some('('), Around),
            Some(1..6)
        );
        assert_eq!(
            text.enclosing_bracket_object(1..6, 1, None, Around),
            Some(0..7)
        );
        assert_eq!(
            text.enclosing_bracket_object(1..6, 1, Some('('), Around),
            None
        );
        assert_eq!(
            text.enclosing_bracket_object(1..6, 1, Some('('), Inside),
            None
        );
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
        "(", ")", "[", "]", "「", "」", "“", "”", "(\u{301}", "\u{600}(", "\u{600})", "a", " ",
        "\n", "\r\n", "日", "🇩🇪",
    ];

    /// Brackets among other text, now and then repeated to span rope chunks.
    fn text() -> impl Strategy<Value = String> {
        let text = || vec(select(PIECES), 0..32).prop_map(|pieces| pieces.concat());
        prop_oneof![
            9 => text(),
            1 => text().prop_map(|text| text.repeat(80)),
        ]
    }

    /// Brackets among other text and an offset in it: a char boundary, most often one where a
    /// bracket char is.
    fn text_and_offset() -> impl Strategy<Value = (String, usize)> {
        text().prop_flat_map(|text| {
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

    /// Brackets among other text and a span in it: most often one grapheme, or a pair's inside
    /// or whole, which a bracket object skips, and otherwise any span between char boundaries.
    fn text_and_span() -> impl Strategy<Value = (String, Range<usize>)> {
        text().prop_flat_map(|text| {
            let graphemes = strategies::grapheme_boundaries(&text);
            let mut spans: Vec<Range<usize>> = graphemes.windows(2).map(|w| w[0]..w[1]).collect();
            spans.push(text.len()..text.len());
            for (open, close, _) in pairs(&text) {
                spans.push(grapheme_end(&text, open)..close);
                spans.push(open..grapheme_end(&text, close));
            }
            let chars = strategies::char_boundaries(&text);
            let any = (select(chars.clone()), select(chars)).prop_map(|(a, b)| a.min(b)..a.max(b));
            (Just(text), prop_oneof![3 => select(spans), 1 => any])
        })
    }

    fn grapheme_end(string: &str, at: usize) -> usize {
        at + string[at..].graphemes(true).next().map_or(0, str::len)
    }

    /// The pairs of brackets in `string`, found from its start: each closing bracket closes the
    /// last one of its kind still open. Each is where it opens, where it closes, and its kind.
    fn pairs(string: &str) -> Vec<(usize, usize, usize)> {
        let mut still_open: Vec<Vec<usize>> = vec![Vec::new(); PAIRS.len()];
        let mut pairs = Vec::new();
        for (at, grapheme) in string.grapheme_indices(true) {
            let first = grapheme.chars().next().unwrap();
            for (index, &(opening, closing)) in PAIRS.iter().enumerate() {
                if first == opening {
                    still_open[index].push(at);
                } else if first == closing
                    && let Some(start) = still_open[index].pop()
                {
                    pairs.push((start, at, index));
                }
            }
        }
        pairs
    }

    /// What `matching_bracket` returns on `string`, worked out from all its pairs.
    fn model(string: &str, offset: usize) -> Option<usize> {
        pairs(string).into_iter().find_map(|(open, close, _)| {
            (open == offset)
                .then_some(close)
                .or((close == offset).then_some(open))
        })
    }

    /// What `enclosing_bracket_object` returns on `string`, worked out from all its pairs: those
    /// of the kind that hold the span, or start with it, in the order they close.
    fn object_model(
        string: &str,
        span: Range<usize>,
        (count, kind): (usize, Option<char>),
        extent: Extent,
    ) -> Option<Range<usize>> {
        let kind = match kind {
            Some(kind) => Some(PAIRS.iter().position(|&(o, c)| kind == o || kind == c)?),
            None => None,
        };
        if count == 0 {
            return None;
        }
        let is_cursor = string[span.clone()].graphemes(true).count() <= 1;
        let mut enclosing: Vec<(usize, usize, usize)> = pairs(string)
            .into_iter()
            .filter(|&(open, close, index)| {
                let holds_start = (open < span.start && span.start <= close)
                    || (open == span.start && !span.is_empty());
                kind.is_none_or(|kind| kind == index)
                    && holds_start
                    && grapheme_end(string, close) >= span.end
            })
            .collect();
        enclosing.sort_by_key(|&(_, close, _)| close);
        enclosing
            .into_iter()
            .map(|(open, close, _)| match extent {
                Inside => grapheme_end(string, open)..close,
                Around => open..grapheme_end(string, close),
            })
            .filter(|object| is_cursor || span.start > object.start || object.end > span.end)
            .nth(count - 1)
    }

    proptest! {
        #[test]
        fn partners_match_a_model_that_pairs_every_bracket(
            (string, offset) in text_and_offset(),
        ) {
            let text = Text::from(string.as_str());
            prop_assert_eq!(text.matching_bracket(offset), model(&string, offset));
        }

        #[test]
        fn enclosing_bracket_objects_match_a_model_that_pairs_every_bracket(
            (string, span) in text_and_span(),
            count in prop_oneof![Just(1), 0..4_usize],
            kind in select(&[None, Some('('), Some(']'), Some('「'), Some('”'), Some('x')][..]),
            around in any::<bool>(),
        ) {
            let text = Text::from(string.as_str());
            let extent = if around { Around } else { Inside };
            prop_assert_eq!(
                text.enclosing_bracket_object(span.clone(), count, kind, extent),
                object_model(&string, span, (count, kind), extent)
            );
        }
    }
}
