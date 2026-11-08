use std::ops::Range;

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

/// Splits a line into segments for the word motions (ADR 0032, 0036). The motions stop where a
/// segment that isn't blank starts or ends, so another segmenter, such as Vim's char classes or a
/// dictionary for CJK, changes the stops without changing the motions.
pub trait Words {
    /// The segments of `piece`, in order and covering all of it: where each ends, and its kind.
    /// `piece` is part of a line's content, without its line break, that starts at the line's
    /// start or at a restart point and ends at the line's end or at a restart point. An empty
    /// piece has none. Each end is a grapheme boundary, so a motion never stops inside a
    /// grapheme.
    fn segments(&self, piece: &str) -> Vec<(usize, WordKind)>;

    /// Whether there is a restart point between `before` and `after`, where `earlier`,
    /// `before`, and `after` are three chars next to each other on a line: splitting a line at
    /// its restart points and segmenting each piece gives the same segments as segmenting the
    /// whole line. Word motions segment the piece of a line around the cursor, between two
    /// restart points, so a long line costs no more than a short one, unless it has none.
    fn restarts_between(&self, earlier: char, before: char, after: char) -> bool;
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

    fn restarts_between(&self, earlier: char, before: char, after: char) -> bool {
        word_starts_after_blank(earlier, before, after)
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

    fn restarts_between(&self, earlier: char, before: char, after: char) -> bool {
        word_starts_after_blank(earlier, before, after)
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

fn is_grapheme_boundary(text: &str, offset: usize) -> bool {
    // The whole text is one chunk, so the cursor never asks for more.
    matches!(
        GraphemeCursor::new(offset, text.len(), true).is_boundary(text, 0),
        Ok(true)
    )
}

/// Whether a word starts between `before`, a blank grapheme of its own, and `after`, a letter or
/// digit: a restart point for both segmenters. A blank ends the segments before it, and a letter
/// or digit starts a word, which nothing before it joins. No word or grapheme rule decides a
/// boundary by looking past a blank, back or ahead, so each side segments the same alone.
///
/// Graphemes can reach across the chars on either side: a prepended char such as U+0600 ARABIC
/// NUMBER SIGN joins the blank after it, and a combining mark joins the char before it. Each
/// would make the blank part of a segment that isn't blank, so the boundaries around `before`
/// must be grapheme boundaries, and the one before `after` a word boundary.
fn word_starts_after_blank(earlier: char, before: char, after: char) -> bool {
    if !before.is_whitespace() || !after.is_alphanumeric() {
        return false;
    }
    let mut buffer = [0; 12];
    let mut len = 0;
    for char in [earlier, before, after] {
        len += char.encode_utf8(&mut buffer[len..]).len();
    }
    let (blank, word) = (earlier.len_utf8(), earlier.len_utf8() + before.len_utf8());
    str::from_utf8(&buffer[..len]).is_ok_and(|chars| {
        is_grapheme_boundary(chars, blank)
            && is_grapheme_boundary(chars, word)
            && chars.split_word_bound_indices().any(|(at, _)| at == word)
    })
}

/// Which boundaries a word motion stops at.
#[derive(Clone, Copy)]
enum Edge {
    /// Where a segment that isn't blank starts: `w`, `b`.
    Start,
    /// Where one ends: `e`, `ge`.
    End,
}

/// How many bytes a word motion segments past a step's start, going forward, or before it, going
/// backward, at least: the window goes on to the next restart point. Segmenting costs about 14 ns
/// a byte, and setting up a window about 0.5 µs, so 256 bytes take about 4 µs.
const WINDOW: usize = 256;

/// The segments of a piece of a line, from its start or a restart point to its end or a restart
/// point, and the stops among them.
struct Window {
    range: Range<usize>,
    /// Where each segment ends, and its kind.
    segments: Vec<(usize, WordKind)>,
    /// The boundaries inside `range` where a segment that isn't blank starts, or ends.
    stops: Vec<usize>,
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
        // The piece of the line the last step segmented. A step on the same line starts at the
        // last one's target, a stop inside it, so it reuses it.
        let mut window: Option<Window> = None;

        for _ in 0..count {
            let mut start = position;
            let (mut near, mut far) = edges(line);

            // A step that starts at its line's far edge has nothing left to cross on that line: it
            // is at the end of the line's content going forward, or at its start going backward. It
            // hops to the next line instead and starts at that line's near edge, so its span never
            // holds a line break. In `"foo\n\nbar"`, `w` from the end of `foo` hops over the empty
            // line to `b`, and its span is `bar`.
            while start == far {
                // The line after going forward, the one before going backward.
                let next = if forward {
                    (line < last).then(|| line + 1)
                } else {
                    line.checked_sub(1)
                };

                // No such line: the step is at the end of the text going forward, or at its start
                // going backward. The hops leave `from` and `position` alone, so the motion returns
                // what the last step set, or `(None, offset)` if this is the first step.
                let Some(next) = next else {
                    return (from, position);
                };

                // An empty line's near edge is also its far edge, so the loop runs again: empty
                // lines are never stops.
                line = next;
                (near, far) = edges(line);
                start = near;
                window = None;
            }

            // The window holds the segments on both sides of the start, so that its stops tell
            // whether the start is one, unless the start is the near edge.
            let content = near.min(far)..near.max(far);
            let window = window
                .get_or_insert_with(|| self.word_window(words, &content, start, forward, edge));

            // The first stop after the start, or the last one before it. A window without one
            // grows toward the far edge until it reaches it.
            let target = loop {
                let stops = &window.stops;
                let target = if forward {
                    stops[stops.partition_point(|&stop| stop <= start)..].first()
                } else {
                    stops[..stops.partition_point(|&stop| stop < start)].last()
                };
                if target.is_some() || window.range.start == far || window.range.end == far {
                    break target.copied();
                }
                self.grow_word_window(window, words, &content, forward, edge);
            };

            // The span starts at the step's start if that is a stop or the near edge, and at the
            // cursor if it is inside a segment.
            from = (start == near || window.stops.binary_search(&start).is_ok()).then_some(start);
            position = target.unwrap_or(far);
        }

        (from, position)
    }

    /// The window a step from `start`, on a line with its content in `content`, segments first:
    /// from the last restart point before `start` to the first one at least [`WINDOW`] bytes
    /// after it going forward, and from the last one at least [`WINDOW`] bytes before it to the
    /// first one after it going backward.
    fn word_window(
        &self,
        words: &(impl Words + ?Sized),
        content: &Range<usize>,
        start: usize,
        forward: bool,
        edge: Edge,
    ) -> Window {
        let (behind, ahead) = if forward { (1, WINDOW) } else { (WINDOW, 1) };
        let range = self.restart_behind(words, start, content.start, behind)
            ..self.restart_ahead(words, start, content.end, ahead);
        let segments = self.segments_in(words, range.clone());
        let stops = stops(&segments, edge);
        Window {
            range,
            segments,
            stops,
        }
    }

    /// Adds the next piece of at least [`WINDOW`] bytes to `window`: after it going forward,
    /// before it going backward.
    fn grow_word_window(
        &self,
        window: &mut Window,
        words: &(impl Words + ?Sized),
        content: &Range<usize>,
        forward: bool,
        edge: Edge,
    ) {
        if forward {
            let end = self.restart_ahead(words, window.range.end, content.end, WINDOW);
            let after = self.segments_in(words, window.range.end..end);
            window.segments.extend(after);
            window.range.end = end;
        } else {
            let start = self.restart_behind(words, window.range.start, content.start, WINDOW);
            let before = self.segments_in(words, start..window.range.start);
            window.segments.splice(0..0, before);
            window.range.start = start;
        }
        // The segments on both sides of where the pieces meet make a stop there.
        window.stops = stops(&window.segments, edge);
    }

    /// The first restart point at least `min` bytes after `from`, or `to`, the end of the line's
    /// content, if there is none. It reads the line from `from` on, so it misses a restart point
    /// one char after `from`, which needs the char before `from`; a window can end at a later one.
    fn restart_ahead(
        &self,
        words: &(impl Words + ?Sized),
        from: usize,
        to: usize,
        min: usize,
    ) -> usize {
        let mut chars = self.rope.byte_slice(from..to).chars();
        let (Some(mut earlier), Some(mut before)) = (chars.next(), chars.next()) else {
            return to;
        };
        // Where `after` starts.
        let mut at = from + earlier.len_utf8() + before.len_utf8();
        for after in chars {
            if at >= from + min && words.restarts_between(earlier, before, after) {
                return at;
            }
            at += after.len_utf8();
            (earlier, before) = (before, after);
        }
        to
    }

    /// The last restart point at least `min` bytes before `from`, or `to`, the start of the
    /// line's content, if there is none.
    fn restart_behind(
        &self,
        words: &(impl Words + ?Sized),
        from: usize,
        to: usize,
        min: usize,
    ) -> usize {
        let mut chars = self.rope.byte_slice(to..from).chars().rev();
        let (Some(mut after), Some(mut before)) = (chars.next(), chars.next()) else {
            return to;
        };
        // Where `after` starts.
        let mut at = from - after.len_utf8();
        for earlier in chars {
            if at + min <= from && words.restarts_between(earlier, before, after) {
                return at;
            }
            at -= before.len_utf8();
            (after, before) = (before, earlier);
        }
        to
    }

    /// The segments of the piece of a line in `range`, which starts at the line's start or a
    /// restart point and ends at its end or a restart point: where each ends, and its kind.
    fn segments_in(
        &self,
        words: &(impl Words + ?Sized),
        range: Range<usize>,
    ) -> Vec<(usize, WordKind)> {
        let start = range.start;
        let mut segments = words.segments(&self.to_string_in(range));
        for (end, _) in &mut segments {
            *end += start;
        }
        segments
    }
}

/// The stops among `segments`: the boundaries between two where one that isn't blank starts, or
/// ends.
fn stops(segments: &[(usize, WordKind)], edge: Edge) -> Vec<usize> {
    segments
        .windows(2)
        .filter(|pair| match edge {
            Edge::Start => pair[1].1 != WordKind::Whitespace,
            Edge::End => pair[0].1 != WordKind::Whitespace,
        })
        .map(|pair| pair[0].0)
        .collect()
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

    /// A line's content: pieces, each up to three times, so runs of blanks and of punctuation are
    /// common.
    fn line() -> impl Strategy<Value = String> {
        let run = (select(PIECES), 1..4_usize).prop_map(|(piece, count)| piece.repeat(count));
        vec(run, 0..24).prop_map(|runs| runs.concat())
    }

    /// Lines of words, empty lines among them, and a grapheme boundary to move from. Now and then
    /// the lines are long runs of pieces, so a motion segments a window of each: a long run of
    /// blanks, or a long word, makes it grow the window, and a line without blanks has no restart
    /// points.
    fn text_and_offset() -> impl Strategy<Value = (String, usize)> {
        let piece = prop_oneof![4 => select(PIECES), 1 => select(&["\n", "\r\n", "\r"][..])];
        let short = vec(piece, 0..40).prop_map(|pieces| pieces.concat());
        let run = (select(PIECES), 1..200_usize).prop_map(|(piece, count)| piece.repeat(count));
        let long_line = vec(run, 1..12).prop_map(|runs| runs.concat());
        let long = vec((long_line, select(&["\n", "\r\n"][..])), 1..4)
            .prop_map(|lines| lines.into_iter().map(|(line, end)| line + end).collect());
        prop_oneof![short, long].prop_flat_map(|text: String| {
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
        // The rule fails only on rare sequences, such as a prepended char before a run of
        // blanks, so this cheap test runs more cases than the default 256.
        #![proptest_config(ProptestConfig::with_cases(4096))]

        #[test]
        fn restart_points_split_a_line_into_its_segments(line in line(), big in any::<bool>()) {
            let words: &dyn Words = if big { &BigWords } else { &UnicodeWords };
            let chars: Vec<(usize, char)> = line.char_indices().collect();
            let restarts = chars
                .windows(3)
                .filter(|three| words.restarts_between(three[0].1, three[1].1, three[2].1))
                .map(|three| three[2].0);
            let cuts: Vec<usize> = [0].into_iter().chain(restarts).chain([line.len()]).collect();
            let pieces: Vec<(usize, WordKind)> = cuts
                .windows(2)
                .flat_map(|cut| {
                    let segments = words.segments(&line[cut[0]..cut[1]]);
                    segments.into_iter().map(move |(end, kind)| (cut[0] + end, kind))
                })
                .collect();
            prop_assert_eq!(pieces, words.segments(&line));
        }
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
