use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::objects::Extent;
use super::words::is_grapheme_boundary;
use crate::Text;

/// The quotes [`Text::quote_object`] pairs, each opening one with its closing one (ADR 0038).
const QUOTES: [(char, char); 4] = [('"', '"'), ('\'', '\''), ('`', '`'), ('‘', '’')];

impl Text {
    /// The string `offset` is in, as Vim's `i"` and `a"` and Helix's `mi"` and `ma"` select it
    /// (ADR 0038). `quote` is `"`, `'`, `` ` ``, `‘`, or `’`; the last two pick `‘’`. Quotes
    /// pair on `offset`'s line from its start, the first with the next and so on, as in Vim. A
    /// quote is a grapheme whose first char is one, and a `'` or `’` with a letter or digit on
    /// both sides is an apostrophe, which doesn't count. A quote at `offset` belongs to its
    /// string.
    ///
    /// `Inside` lies between the quotes and may be empty; `Around` includes them, and the blanks
    /// Vim's `a"` adds are the Vim preset's. Returns `None` if `quote` is none of these or
    /// `offset` is in no string; Vim's fallback to the next string on the line is the Vim
    /// preset's. A call reads `offset`'s line up to the string.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn quote_object(&self, offset: usize, quote: char, extent: Extent) -> Option<Range<usize>> {
        let line = self.line_of(offset);
        let (open, close) = QUOTES
            .into_iter()
            .find(|&(open, close)| quote == open || quote == close)?;
        let start = self.line_start(line);
        let content = self.to_string_in(start..self.line_end(line));
        let offset = offset - start;

        let mut quotes = content
            .match_indices([open, close])
            .map(|(at, _)| at)
            .filter(|&at| is_grapheme_boundary(&content, at) && !is_apostrophe(&content, at));
        let grapheme_end =
            |at: usize| at + content[at..].graphemes(true).next().map_or(0, str::len);
        loop {
            let opening = quotes.find(|&at| content[at..].starts_with(open))?;
            if opening > offset {
                return None;
            }
            let closing = quotes.find(|&at| content[at..].starts_with(close))?;
            if offset < grapheme_end(closing) {
                return Some(self.delimited(start + opening, start + closing, extent));
            }
        }
    }

    /// The pair of `delimiter`s around `offset`, as Helix's `mi*` and `ma*` select it (ADR
    /// 0038): the `count`-th one before `offset` and the `count`-th one after it, inside the
    /// paragraph `offset` is on. A delimiter is a grapheme whose first char is `delimiter`.
    /// `Inside` lies between them; `Around` includes them. It is meant for a char that is no
    /// bracket or quote, and doesn't check: a preset sends those to
    /// [`enclosing_bracket_object`](Self::enclosing_bracket_object) and
    /// [`quote_object`](Self::quote_object), and here they pair as any char does.
    ///
    /// Returns `None` if `offset` is on a delimiter or a blank line, if either side runs out, or
    /// with a count of 0. A call reads the paragraph out to the delimiters.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn pair_object(
        &self,
        offset: usize,
        count: usize,
        delimiter: char,
        extent: Extent,
    ) -> Option<Range<usize>> {
        let lines = self.paragraph_lines(self.line_of(offset))?;
        let on_delimiter = self.rope.byte_slice(offset..).chars().next() == Some(delimiter)
            && self.is_grapheme_boundary(offset);
        if count == 0 || on_delimiter {
            return None;
        }
        let bounds = self.line_start(lines.start)..self.line_end(lines.end - 1);
        let is_delimiter = |ch| ch == delimiter;
        let before = self.find_in(bounds.clone(), offset, count, false, is_delimiter)?;
        let after = self.find_in(bounds, offset, count, true, is_delimiter)?;
        Some(self.delimited(before, after, extent))
    }
}

/// Whether the quote at `at`, a grapheme boundary of `line`, is an apostrophe: a `'` or `’` with
/// a letter or digit on both sides, the first chars of the graphemes before and after it.
fn is_apostrophe(line: &str, at: usize) -> bool {
    let first_char = |grapheme: Option<&str>| grapheme.and_then(|grapheme| grapheme.chars().next());
    let mut after = line[at..].graphemes(true);
    let quote = first_char(after.next());
    let before = first_char(line[..at].graphemes(true).next_back());
    matches!(quote, Some('\'' | '’'))
        && before.is_some_and(char::is_alphanumeric)
        && first_char(after.next()).is_some_and(char::is_alphanumeric)
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::sample::select;

    use super::*;
    use crate::Extent::{Around, Inside};
    use crate::strategies;

    #[test]
    fn quotes_pair_from_the_line_start() {
        // `"` at 4, 7, 13, and 17.
        let text = Text::from("say \"hi\" and \"bye\"\n\"x");
        assert_eq!(text.quote_object(5, '"', Inside), Some(5..7));
        assert_eq!(text.quote_object(5, '"', Around), Some(4..8));
        // A quote at the offset belongs to its string.
        assert_eq!(text.quote_object(4, '"', Inside), Some(5..7));
        assert_eq!(text.quote_object(7, '"', Inside), Some(5..7));
        assert_eq!(text.quote_object(15, '"', Inside), Some(14..17));
        // Between strings and before the first, there is none, and a string doesn't go on to the
        // next line. Another kind of quote finds none either.
        assert_eq!(text.quote_object(9, '"', Inside), None);
        assert_eq!(text.quote_object(0, '"', Inside), None);
        assert_eq!(text.quote_object(20, '"', Inside), None);
        assert_eq!(text.quote_object(5, '\'', Inside), None);
        assert_eq!(text.quote_object(5, 'x', Inside), None);
    }

    #[test]
    fn apostrophes_between_letters_dont_count() {
        // The `'` of `don't` is an apostrophe; `'hi'` pairs.
        let text = Text::from("don't say 'hi'");
        assert_eq!(text.quote_object(11, '\'', Inside), Some(11..13));
        // The second `'` of `'n'` has a blank after it.
        let text = Text::from("rock 'n' roll");
        assert_eq!(text.quote_object(6, '\'', Inside), Some(6..7));
        // `‘` at 0, `’` at 6 and 10, three bytes each: the one in `don’t` is an apostrophe.
        let text = Text::from("‘don’t’");
        assert_eq!(text.quote_object(3, '’', Inside), Some(3..10));
        assert_eq!(text.quote_object(3, '‘', Around), Some(0..13));
    }

    #[test]
    fn pairs_of_chars_are_the_nearest_on_each_side_in_the_paragraph() {
        // `*` at 2, 4, 8, 10, a blank line, and `*` at 13 and 15.
        let text = Text::from("a *b* c *d*\n\n*e*");
        assert_eq!(text.pair_object(3, 1, '*', Inside), Some(3..4));
        assert_eq!(text.pair_object(3, 1, '*', Around), Some(2..5));
        // Between two pairs, the nearest on each side; a count goes out on both.
        assert_eq!(text.pair_object(6, 1, '*', Inside), Some(5..8));
        assert_eq!(text.pair_object(6, 2, '*', Inside), Some(3..10));
        assert_eq!(text.pair_object(3, 2, '*', Inside), None);
        // None on a delimiter or a blank line, and none across a blank line.
        assert_eq!(text.pair_object(4, 1, '*', Inside), None);
        assert_eq!(text.pair_object(12, 1, '*', Inside), None);
        assert_eq!(text.pair_object(14, 1, '*', Inside), Some(14..15));
        assert_eq!(text.pair_object(9, 2, '*', Inside), None);
    }

    /// Quotes and `*`, letters and digits for apostrophes, blanks, line breaks, and chars that
    /// join the grapheme before or after them, so that a quote can be inside one.
    const PIECES: &[&str] = &[
        "\"", "'", "‘", "’", "*", "a", "1", "日", " ", "\n", "\r\n", "\u{301}", "\u{600}",
    ];

    /// Lines of [`PIECES`] and a char boundary in them.
    fn lines_and_offset() -> impl Strategy<Value = (String, usize)> {
        vec(select(PIECES), 0..40)
            .prop_map(|pieces| pieces.concat())
            .prop_flat_map(|lines| {
                let offset = select(strategies::char_boundaries(&lines));
                (Just(lines), offset)
            })
    }

    /// The graphemes of `string[bounds]`, as spans of `string`.
    fn graphemes_in(string: &str, bounds: Range<usize>) -> Vec<Range<usize>> {
        string[bounds.clone()]
            .grapheme_indices(true)
            .map(|(at, grapheme)| bounds.start + at..bounds.start + at + grapheme.len())
            .collect()
    }

    /// The string around `offset` on `line`, a span of `string`, walking the line's graphemes
    /// from its start: a quote opens a string, the next one closes it, and so on.
    fn quote_model(
        string: &str,
        line: Range<usize>,
        offset: usize,
        (open, close): (char, char),
        extent: Extent,
    ) -> Option<Range<usize>> {
        let graphemes = graphemes_in(string, line);
        let first_char = |index: usize| string[graphemes[index].clone()].chars().next();
        let is_alphanumeric = |index: Option<usize>| {
            let at = index.filter(|&index| index < graphemes.len());
            at.and_then(first_char).is_some_and(char::is_alphanumeric)
        };
        let mut opening: Option<&Range<usize>> = None;
        for (index, grapheme) in graphemes.iter().enumerate() {
            let quote = first_char(index);
            let apostrophe = matches!(quote, Some('\'' | '’'))
                && is_alphanumeric(index.checked_sub(1))
                && is_alphanumeric(Some(index + 1));
            match opening {
                _ if apostrophe => {}
                None if quote == Some(open) => opening = Some(grapheme),
                Some(opened) if quote == Some(close) => {
                    if (opened.start..grapheme.end).contains(&offset) {
                        return Some(match extent {
                            Inside => opened.end..grapheme.start,
                            Around => opened.start..grapheme.end,
                        });
                    }
                    opening = None;
                }
                _ => {}
            }
        }
        None
    }

    proptest! {
        #[test]
        fn quote_objects_match_a_model_of_the_lines_graphemes(
            (string, offset) in lines_and_offset(),
            quote in select(&['"', '\'', '‘', '’'][..]),
            around in any::<bool>(),
        ) {
            let text = Text::from(string.as_str());
            let extent = if around { Around } else { Inside };
            let line = text.line_of(offset);
            let pair = if quote == '‘' || quote == '’' { ('‘', '’') } else { (quote, quote) };
            prop_assert_eq!(
                text.quote_object(offset, quote, extent),
                quote_model(&string, text.line_start(line)..text.line_end(line), offset, pair, extent)
            );
        }

        #[test]
        fn pair_objects_match_a_model_of_the_paragraphs_graphemes(
            (string, offset) in lines_and_offset(),
            count in prop_oneof![Just(1), 0..4_usize],
            around in any::<bool>(),
        ) {
            let text = Text::from(string.as_str());
            let extent = if around { Around } else { Inside };
            // The `count`-th `*` before the offset and after it, in its paragraph.
            let model = || {
                let lines = text.paragraph_lines(text.line_of(offset))?;
                let bounds = text.line_start(lines.start)..text.line_end(lines.end - 1);
                let mut stars = graphemes_in(&string, bounds);
                stars.retain(|grapheme| string[grapheme.clone()].starts_with('*'));
                let nth = count.checked_sub(1)?;
                if stars.iter().any(|star| star.start == offset) {
                    return None;
                }
                let before = stars.iter().rev().filter(|star| star.start < offset).nth(nth)?;
                let after = stars.iter().filter(|star| star.start > offset).nth(nth)?;
                Some(match extent {
                    Inside => before.end..after.start,
                    Around => before.start..after.end,
                })
            };
            prop_assert_eq!(text.pair_object(offset, count, '*', extent), model());
        }
    }
}
