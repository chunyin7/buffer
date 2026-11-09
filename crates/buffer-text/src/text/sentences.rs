use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::objects::{Extent, object_span};
use super::words::is_grapheme_boundary;
use crate::Text;

impl Text {
    /// The sentence object at `offset`, as Vim's `is` and `as` select it (ADR 0038). Sentences
    /// follow Unicode's sentence boundaries (UAX #29) inside the paragraph `offset` is on, with
    /// its line breaks read as spaces, so a hard-wrapped sentence stays whole. `Inside` is the
    /// sentence at `offset` without the blanks after it, or a run of blanks between sentences.
    /// `Around` adds the blanks after it, or before it if none follow; from blanks, it takes them
    /// and the sentence after. A count takes that many sentences, as
    /// [`word_object`](Self::word_object) takes segments, and stops at the paragraph's end.
    ///
    /// Returns `None` on a blank line, at the end of the paragraph's last line, or with a count
    /// of 0. The whole paragraph is segmented, so a call costs O(paragraph).
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn sentence_object(
        &self,
        offset: usize,
        count: usize,
        extent: Extent,
    ) -> Option<Range<usize>> {
        let lines = self.paragraph_lines(self.line_of(offset))?;
        let start = self.line_start(lines.start);
        let units = sentence_units(&self.to_string_in(start..self.line_end(lines.end - 1)));
        let at = units
            .iter()
            .position(|(unit, _)| start + unit.end > offset)?;
        let blanks_before = || {
            let (unit, blank) = units.get(at.checked_sub(1)?)?;
            blank.then_some(start + unit.start)
        };
        let units = units[at..]
            .iter()
            .map(|(unit, blank)| (start + unit.start..start + unit.end, *blank));
        object_span(units, blanks_before, count, extent)
    }
}

/// The sentences of `paragraph` and the runs of blanks around them, in order and covering all of
/// it, each with whether it is blank. Their ends are grapheme boundaries.
fn sentence_units(paragraph: &str) -> Vec<(Range<usize>, bool)> {
    let mut units: Vec<(Range<usize>, bool)> = Vec::new();
    let mut push = |unit: Range<usize>, blank: bool| match units.last_mut() {
        _ if unit.is_empty() => {}
        Some((last, true)) if blank => last.end = unit.end,
        _ => units.push((unit, blank)),
    };

    // Line breaks as spaces of the same length, so a sentence goes on across them.
    let spaced = paragraph.replace("\r\n", "  ").replace('\n', " ");
    let mut start = 0;
    for (at, sentence) in spaced.split_sentence_bound_indices() {
        let end = at + sentence.len();
        // UAX #29 can end a sentence inside a grapheme; the sentence goes on to its end.
        if !is_grapheme_boundary(paragraph, end) {
            continue;
        }
        // The sentence's text, without the blank graphemes before and after it.
        let mut text = paragraph[start..end]
            .grapheme_indices(true)
            .filter(|(_, grapheme)| !grapheme.chars().all(char::is_whitespace));
        match text.next() {
            Some((first, grapheme)) => {
                let last = text.next_back().unwrap_or((first, grapheme));
                let (first, last) = (start + first, start + last.0 + last.1.len());
                push(start..first, true);
                push(first..last, false);
                push(last..end, true);
            }
            None => push(start..end, true),
        }
        start = end;
    }
    units
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
    fn sentences_follow_uax_29_across_hard_wraps() {
        // `See e.g. the docs.` 0..18, `Then go.` 19..27, a line break, `Wrapped here`, another,
        // `and on.` up to 48, two spaces, `Last one` 50..58. After a blank line, three
        // sentences without spaces: 60..78, 78..96, and 96..102.
        let text = Text::from(
            "See e.g. the docs. Then go.\nWrapped here\nand on.  Last one\n\n\
             日本語です。次の文です！最後",
        );
        assert_eq!(text.sentence_object(0, 1, Inside), Some(0..18));
        assert_eq!(text.sentence_object(0, 1, Around), Some(0..19));
        // A hard-wrapped sentence stays whole, line breaks and all.
        assert_eq!(text.sentence_object(30, 1, Inside), Some(28..48));
        assert_eq!(text.sentence_object(81, 1, Inside), Some(78..96));
        // Nothing follows the paragraph's last sentence, so `Around` takes the blanks before.
        assert_eq!(text.sentence_object(50, 1, Around), Some(48..58));
        // Blanks between sentences: alone, or with the sentence after.
        assert_eq!(text.sentence_object(18, 1, Inside), Some(18..19));
        assert_eq!(text.sentence_object(18, 1, Around), Some(18..27));
        // Counts stop at the paragraph's end.
        assert_eq!(text.sentence_object(0, 2, Inside), Some(0..19));
        assert_eq!(text.sentence_object(0, 2, Around), Some(0..28));
        // Nothing follows the last sentence, so `Around` takes the blanks before the first.
        assert_eq!(text.sentence_object(19, 9, Around), Some(18..58));
        // No sentence on a blank line, past the paragraph's last char, or with a count of 0.
        assert_eq!(text.sentence_object(59, 1, Inside), None);
        assert_eq!(text.sentence_object(58, 1, Inside), None);
        assert_eq!(text.sentence_object(0, 0, Inside), None);
    }

    /// Sentences and the blanks around them, with terminators, closing quotes and brackets,
    /// line breaks, CJK, and chars that join the grapheme before or after them.
    const PIECES: &[&str] = &[
        "Hi", "e.g", "3", ".", "!", "?", "。", "！", "\"", ")", " ", "\u{a0}", "\n", "\r\n", "\r",
        "日本", "\u{301}", "\u{600}", "\u{903}", "\u{200d}", "🇩🇪",
    ];

    proptest! {
        #[test]
        fn sentence_units_cover_the_paragraph_on_grapheme_boundaries(
            pieces in vec(select(PIECES), 0..40),
        ) {
            let paragraph = pieces.concat();
            let units = sentence_units(&paragraph);
            let graphemes = strategies::grapheme_boundaries(&paragraph);
            let is_blank = |text: &str| text.chars().all(char::is_whitespace);
            let mut start = 0;
            for (index, (unit, blank)) in units.iter().enumerate() {
                prop_assert_eq!(unit.start, start);
                prop_assert!(unit.start < unit.end);
                prop_assert!(graphemes.contains(&unit.end));
                let text = &paragraph[unit.clone()];
                prop_assert_eq!(*blank, is_blank(text), "{:?}", text);
                // A sentence starts and ends with graphemes that aren't blank, and blanks merge.
                if !blank {
                    let mut graphemes = text.graphemes(true);
                    prop_assert!(!graphemes.next().is_some_and(is_blank));
                    prop_assert!(!graphemes.next_back().is_some_and(is_blank));
                } else if let Some((_, next)) = units.get(index + 1) {
                    prop_assert!(!next);
                }
                start = unit.end;
            }
            prop_assert_eq!(start, paragraph.len());
        }
    }
}
