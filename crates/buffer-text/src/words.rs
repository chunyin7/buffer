use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use crate::Text;

/// What a word segment holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordKind {
    /// Letters or digits, with what Unicode joins to them: `foo`, `3.14`, `don't`, `foo_bar`, `日`.
    Word,
    /// Anything else that isn't blank: `**`, `[[`, `...`, emoji.
    Punctuation,
    /// Whitespace.
    Whitespace,
}

/// Splits a line into segments for the word motions (ADR 0032). The motions stop where a segment
/// that isn't blank starts or ends, so another segmenter, such as Vim's char classes or a
/// dictionary for CJK, changes the stops without changing the motions.
pub trait Words {
    /// The segments of `line`, a line's content without its line break, in order and covering all
    /// of it: where each ends, and its kind. An empty line has none. Each end is a grapheme
    /// boundary, so a motion never stops inside a grapheme.
    fn segments(&self, line: &str) -> Vec<(usize, WordKind)>;
}

/// Unicode's word boundaries (UAX #29), with adjacent punctuation merged into one segment, so
/// `**`, `[[`, `...`, and `->` are one stop each. Adjacent whitespace merges too. A segment with a
/// letter or digit is a word, one of whitespace only is blank, and any other is punctuation.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnicodeWords;

impl Words for UnicodeWords {
    fn segments(&self, line: &str) -> Vec<(usize, WordKind)> {
        let mut segments: Vec<(usize, WordKind)> = Vec::new();
        for (start, piece) in line.split_word_bound_indices() {
            let end = start + piece.len();
            let kind = kind_of(piece);
            match segments.last_mut() {
                // UAX #29 can break inside a grapheme, after a prepended char such as U+0600
                // ARABIC NUMBER SIGN; the grapheme stays whole.
                Some(last) if !is_grapheme_boundary(line, start) => {
                    *last = (end, joined(last.1, kind))
                }
                Some(last) if last.1 == kind && kind != WordKind::Word => last.0 = end,
                _ => segments.push((end, kind)),
            }
        }
        segments
    }
}

/// WORDs, which Vim's `W`, `B`, and `E` move by: runs of graphemes that aren't blank, and runs of
/// graphemes that are.
#[derive(Clone, Copy, Debug, Default)]
pub struct BigWords;

impl Words for BigWords {
    fn segments(&self, line: &str) -> Vec<(usize, WordKind)> {
        let mut segments: Vec<(usize, WordKind)> = Vec::new();
        for (start, grapheme) in line.grapheme_indices(true) {
            let end = start + grapheme.len();
            let kind = if kind_of(grapheme) == WordKind::Whitespace {
                WordKind::Whitespace
            } else {
                WordKind::Word
            };
            match segments.last_mut() {
                Some(last) if last.1 == kind => last.0 = end,
                _ => segments.push((end, kind)),
            }
        }
        segments
    }
}

/// The kind of a segment: a word if it has a letter or digit, blank if it is all whitespace.
fn kind_of(segment: &str) -> WordKind {
    if segment.chars().all(char::is_whitespace) {
        WordKind::Whitespace
    } else if segment.chars().any(char::is_alphanumeric) {
        WordKind::Word
    } else {
        WordKind::Punctuation
    }
}

/// The kind of two segments joined into one.
fn joined(a: WordKind, b: WordKind) -> WordKind {
    if a == WordKind::Word || b == WordKind::Word {
        WordKind::Word
    } else if a == WordKind::Punctuation || b == WordKind::Punctuation {
        WordKind::Punctuation
    } else {
        WordKind::Whitespace
    }
}

fn is_grapheme_boundary(line: &str, offset: usize) -> bool {
    // The whole line is one chunk, so the cursor never asks for more.
    matches!(
        GraphemeCursor::new(offset, line.len(), true).is_boundary(line, 0),
        Ok(true)
    )
}

/// Which boundaries a word motion stops at.
#[derive(Clone, Copy)]
enum Edge {
    /// Where a segment that isn't blank starts: `w`, `b`.
    Start,
    /// Where one ends: `e`, `ge`.
    End,
}

impl Text {
    /// Moves `count` word starts forward from `offset`, as `w` does (ADR 0032, 0034). Each step
    /// stops where the next segment on the line that isn't blank starts, or at the line's end. A
    /// step that starts at a line's end first skips the line break and any empty lines, so the
    /// span a step crosses never holds a line break. A count past the last target stops there;
    /// at the end of the text, the target is `offset`.
    ///
    /// Returns where the last step's span starts, and the target. The span starts at the step's
    /// start if that is a stop of its own or a line's start, otherwise at the cursor: `None`.
    /// Helix passes its cursor's end as `offset` and selects
    /// `Range::new(from.unwrap_or(cursor), target)`; Vim passes its cursor's start and moves to
    /// the target.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn next_word_start(
        &self,
        offset: usize,
        count: usize,
        words: &(impl Words + ?Sized),
    ) -> (Option<usize>, usize) {
        self.word_motion(offset, count, words, true, Edge::Start)
    }

    /// Moves `count` word ends forward from `offset`, as `e` does: each step stops where the next
    /// segment on the line that isn't blank ends, or at the line's end. See
    /// [`next_word_start`](Self::next_word_start).
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn next_word_end(
        &self,
        offset: usize,
        count: usize,
        words: &(impl Words + ?Sized),
    ) -> (Option<usize>, usize) {
        self.word_motion(offset, count, words, true, Edge::End)
    }

    /// Moves `count` word starts back from `offset`, as `b` does: each step stops where the
    /// previous segment on the line that isn't blank starts, or at the line's start, and one that
    /// starts at a line's start first skips back over the line break and any empty lines. The
    /// span starts at the step's start if that is a stop or a line's end, otherwise at the
    /// cursor's end: Helix passes its cursor's start as `offset`. See
    /// [`next_word_start`](Self::next_word_start).
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn prev_word_start(
        &self,
        offset: usize,
        count: usize,
        words: &(impl Words + ?Sized),
    ) -> (Option<usize>, usize) {
        self.word_motion(offset, count, words, false, Edge::Start)
    }

    /// Moves `count` word ends back from `offset`, as `ge` does: each step stops where the
    /// previous segment on the line that isn't blank ends, or at the line's start. See
    /// [`prev_word_start`](Self::prev_word_start).
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn prev_word_end(
        &self,
        offset: usize,
        count: usize,
        words: &(impl Words + ?Sized),
    ) -> (Option<usize>, usize) {
        self.word_motion(offset, count, words, false, Edge::End)
    }

    fn word_motion(
        &self,
        offset: usize,
        count: usize,
        words: &(impl Words + ?Sized),
        forward: bool,
        edge: Edge,
    ) -> (Option<usize>, usize) {
        let last = self.line_count() - 1;
        // The edge of `line` a step comes from, and the one it runs to.
        let edges = |line| {
            let (start, end) = (self.line_start(line), self.line_end(line));
            if forward { (start, end) } else { (end, start) }
        };

        let mut line = self.line_of(offset);
        let (mut position, mut from) = (offset, None);
        // The stops of `stops_line`: steps on the same line reuse them.
        let (mut stops, mut stops_line) = (Vec::new(), None);

        for _ in 0..count {
            let mut start = position;
            let (mut near, mut far) = edges(line);

            // A step from the far edge starts at the next line's near edge, past the line break
            // and any empty lines. At the end of the text there is no next line: the motion ends.
            while start == far {
                let next = if forward {
                    (line < last).then(|| line + 1)
                } else {
                    line.checked_sub(1)
                };
                let Some(next) = next else {
                    return (from, position);
                };

                line = next;
                (near, far) = edges(line);
                start = near;
            }

            if stops_line != Some(line) {
                stops = self.word_stops(line, words, edge);
                stops_line = Some(line);
            }

            // The span starts at the step's start if that is a stop or the near edge, and at the
            // cursor if it is inside a segment.
            from = (start == near || stops.binary_search(&start).is_ok()).then_some(start);

            // The first stop after the start, or the last one before it; else the far edge.
            let target = if forward {
                stops[stops.partition_point(|&stop| stop <= start)..].first()
            } else {
                stops[..stops.partition_point(|&stop| stop < start)].last()
            };
            position = target.copied().unwrap_or(far);
        }

        (from, position)
    }

    /// The stops on `line`: the boundaries inside it where a segment that isn't blank starts, or
    /// ends.
    fn word_stops(&self, line: usize, words: &(impl Words + ?Sized), edge: Edge) -> Vec<usize> {
        let line_start = self.line_start(line);
        let segments = words.segments(&self.to_string_in(line_start..self.line_end(line)));

        segments
            .windows(2)
            .filter(|pair| match edge {
                Edge::Start => pair[1].1 != WordKind::Whitespace,
                Edge::End => pair[0].1 != WordKind::Whitespace,
            })
            .map(|pair| line_start + pair[0].0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::sample::select;
    use unicode_segmentation::UnicodeSegmentation;

    use super::*;
    use crate::strategies;

    /// `line`'s segments: words in brackets, punctuation in angle brackets, blanks as `_`.
    fn render(words: &impl Words, line: &str) -> String {
        let mut start = 0;
        let mut rendered = String::new();
        for (end, kind) in words.segments(line) {
            let text = &line[start..end];
            match kind {
                WordKind::Word => rendered.push_str(&format!("[{text}]")),
                WordKind::Punctuation => rendered.push_str(&format!("<{text}>")),
                WordKind::Whitespace => rendered.push('_'),
            }
            start = end;
        }
        rendered
    }

    #[test]
    fn unicode_words_merge_adjacent_punctuation() {
        let cases = [
            (
                "**bold** and [[Note|alias]] -> done...",
                "<**>[bold]<**>_[and]_<[[>[Note]<|>[alias]<]]>_<->>_[done]<...>",
            ),
            (
                "e.g. don't 3.14 foo_bar __init__",
                "[e.g]<.>_[don't]_[3.14]_[foo_bar]_[__init__]",
            ),
            ("https://example.com/a", "[https]<://>[example.com]</>[a]"),
            ("日本語のテキスト。", "[日][本][語][の][テキスト]<。>"),
            ("emoji 👍🏽👍 «quote»", "[emoji]_<👍🏽👍>_<«>[quote]<»>"),
            (" \t\u{a0}indented", "_[indented]"),
            ("", ""),
        ];
        for (line, expected) in cases {
            assert_eq!(render(&UnicodeWords, line), expected, "{line:?}");
        }
    }

    #[test]
    fn big_words_split_at_blank_graphemes_only() {
        assert_eq!(
            render(&BigWords, "**bold** and [[Note|alias]]"),
            "[**bold**]_[and]_[[[Note|alias]]]"
        );
        // A space with a combining accent isn't blank.
        assert_eq!(render(&BigWords, "a \u{301}b  c"), "[a \u{301}b]_[c]");
    }

    #[test]
    fn word_motions_select_as_helix_does() {
        let text = Text::from("foo bar baz");
        let words = &UnicodeWords;
        // From `f`, Helix passes its cursor's end: `w` selects `foo `, then `bar `.
        assert_eq!(text.next_word_start(1, 1, words), (None, 4));
        assert_eq!(text.next_word_start(4, 1, words), (Some(4), 8));
        // With a count, the span is the last step's.
        assert_eq!(text.next_word_start(1, 2, words), (Some(4), 8));
        // `e` from `f` selects `foo`; from its last `o`, ` bar`.
        assert_eq!(text.next_word_end(1, 1, words), (None, 3));
        assert_eq!(text.next_word_end(3, 1, words), (Some(3), 7));
        // `b` passes the cursor's start: from the `r` of `bar` it selects `bar`; from its `b`,
        // `foo `.
        assert_eq!(text.prev_word_start(6, 1, words), (None, 4));
        assert_eq!(text.prev_word_start(4, 1, words), (Some(4), 0));
        // At the end of the text, `w` stays.
        assert_eq!(text.next_word_start(11, 1, words), (None, 11));
        // WORDs: `foo.bar` is one.
        let text = Text::from("foo.bar baz");
        assert_eq!(text.next_word_start(1, 1, &BigWords), (None, 8));
    }

    #[test]
    fn a_word_span_never_holds_a_line_break() {
        let text = Text::from("foo bar\n\n  baz qux");
        let words = &UnicodeWords;
        // In the last word of a line, `w` stops at the line's end.
        assert_eq!(text.next_word_start(6, 1, words), (None, 7));
        // From there it skips the break and the empty line; the indent is the next span.
        assert_eq!(text.next_word_start(7, 1, words), (Some(9), 11));
        // `b` from `baz` stops at its line's start, then skips back to `bar`.
        assert_eq!(text.prev_word_start(11, 1, words), (Some(11), 9));
        assert_eq!(text.prev_word_start(9, 1, words), (Some(7), 4));
    }

    /// Pieces of prose and Markdown: words, numbers, punctuation that UAX #29 keeps in a word or
    /// splits off, blanks, CJK, emoji, a space with a combining accent, and a prepended char.
    const PIECES: &[&str] = &[
        "foo", "Bar", "3", "14", ".", "'", "_", "**", "[[", "]]", "->", "#", " ", "\t", "\u{a0}",
        "日", "本", "カタ", "e\u{301}", " \u{301}", "\u{600}", "🇩🇪", "👍🏽",
    ];

    /// A line's content.
    fn line() -> impl Strategy<Value = String> {
        vec(select(PIECES), 0..24).prop_map(|pieces| pieces.concat())
    }

    /// Lines of words, empty lines among them, and a grapheme boundary to move from.
    fn text_and_offset() -> impl Strategy<Value = (String, usize)> {
        let piece = prop_oneof![4 => select(PIECES), 1 => select(&["\n", "\r\n", "\r"][..])];
        vec(piece, 0..40)
            .prop_map(|pieces| pieces.concat())
            .prop_flat_map(|text| {
                let offset = select(strategies::grapheme_boundaries(&text));
                (Just(text), offset)
            })
    }

    /// What a word motion does on `string`, worked out from its lines and their segments.
    fn model(
        string: &str,
        (offset, count): (usize, usize),
        words: &dyn Words,
        (forward, starts): (bool, bool),
    ) -> (Option<usize>, usize) {
        // Each line: where it starts, and where its content ends, before a `\n` or `\r\n`.
        let pieces: Vec<&str> = string.split('\n').collect();
        let mut lines = Vec::new();
        let mut start = 0;
        for (index, piece) in pieces.iter().enumerate() {
            let content = if index + 1 < pieces.len() {
                piece.strip_suffix('\r').unwrap_or(piece)
            } else {
                piece
            };
            lines.push((start, start + content.len()));
            start += piece.len() + 1;
        }
        // The boundaries inside a line where a segment that isn't blank starts, or ends.
        let stops = |line: usize| -> Vec<usize> {
            let (line_start, line_end) = lines[line];
            let segments = words.segments(&string[line_start..line_end]);
            let mut stops = Vec::new();
            for pair in segments.windows(2) {
                let side = if starts { pair[1].1 } else { pair[0].1 };
                if side != WordKind::Whitespace {
                    stops.push(line_start + pair[0].0);
                }
            }
            stops
        };
        let mut line = lines
            .iter()
            .rposition(|&(start, _)| start <= offset)
            .unwrap();
        let (mut position, mut from) = (offset, None);
        for _ in 0..count {
            let mut start = position;
            if forward {
                while line + 1 < lines.len() && start == lines[line].1 {
                    line += 1;
                    start = lines[line].0;
                }
            } else {
                while line > 0 && start == lines[line].0 {
                    line -= 1;
                    start = lines[line].1;
                }
            }
            let (line_start, line_end) = lines[line];
            let (near, far) = if forward {
                (line_start, line_end)
            } else {
                (line_end, line_start)
            };
            if start == far {
                break;
            }
            let stops = stops(line);
            let target = if forward {
                stops.iter().copied().find(|&stop| stop > start)
            } else {
                stops.iter().copied().rev().find(|&stop| stop < start)
            };
            from = (start == near || stops.contains(&start)).then_some(start);
            position = target.unwrap_or(far);
        }
        (from, position)
    }

    proptest! {
        #[test]
        fn unicode_words_keep_uax_29_boundaries_and_merge_punctuation(line in line()) {
            let segments = UnicodeWords.segments(&line);
            let graphemes = strategies::grapheme_boundaries(&line);
            let uax_29: Vec<usize> = line.split_word_bound_indices().map(|(at, _)| at).collect();
            let mut start = 0;
            for (index, &(end, kind)) in segments.iter().enumerate() {
                prop_assert!(start < end);
                prop_assert!(graphemes.contains(&end));
                prop_assert!(end == line.len() || uax_29.contains(&end));
                let text = &line[start..end];
                let expected = if text.chars().all(char::is_whitespace) {
                    WordKind::Whitespace
                } else if text.chars().any(char::is_alphanumeric) {
                    WordKind::Word
                } else {
                    WordKind::Punctuation
                };
                prop_assert_eq!(kind, expected, "{:?}", text);
                // Only words sit next to their own kind: punctuation and blanks merge.
                if let Some(&(_, next)) = segments.get(index + 1) {
                    prop_assert!(kind != next || kind == WordKind::Word);
                }
                start = end;
            }
            prop_assert_eq!(start, line.len());
        }

        #[test]
        fn big_words_alternate_blank_and_not(line in line()) {
            let segments = BigWords.segments(&line);
            let graphemes = strategies::grapheme_boundaries(&line);
            let mut start = 0;
            for (index, &(end, kind)) in segments.iter().enumerate() {
                prop_assert!(start < end);
                prop_assert!(graphemes.contains(&end));
                let blank = line[start..end].chars().all(char::is_whitespace);
                prop_assert_eq!(kind, if blank { WordKind::Whitespace } else { WordKind::Word });
                if let Some(&(_, next)) = segments.get(index + 1) {
                    prop_assert_ne!(kind, next);
                }
                start = end;
            }
            prop_assert_eq!(start, line.len());
        }

        #[test]
        fn word_motions_match_a_model_of_the_lines(
            (string, offset) in text_and_offset(),
            count in 0..4_usize,
            motion in 0..4_usize,
            big in any::<bool>(),
        ) {
            let text = Text::from(string.as_str());
            let words: &dyn Words = if big { &BigWords } else { &UnicodeWords };
            let (moved, kind) = match motion {
                0 => (text.next_word_start(offset, count, words), (true, true)),
                1 => (text.next_word_end(offset, count, words), (true, false)),
                2 => (text.prev_word_start(offset, count, words), (false, true)),
                _ => (text.prev_word_end(offset, count, words), (false, false)),
            };
            prop_assert_eq!(moved, model(&string, (offset, count), words, kind));
        }
    }
}
