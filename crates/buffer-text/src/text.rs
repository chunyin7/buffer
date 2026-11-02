use std::fmt;
use std::ops::Range;

use crop::Rope;
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete, UnicodeSegmentation};

/// UTF-8 text in a rope, addressed by byte offsets.
///
/// Clones are cheap and share structure (copy-on-write), so background work can read a snapshot
/// while the UI thread keeps editing. Outside this crate, only a [`Document`](crate::Document)
/// changes a `Text`, so every edit lands in its history.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Text {
    rope: Rope,
}

/// Where a grapheme motion stops: at the edges of the line it starts on, as Vim's `h` and `l`
/// do, or only at the ends of the text, as Helix's do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEdges {
    /// Stop at the start of the line and at the end of its content, before its line break.
    Stop,
    /// Cross line breaks; a line break is one grapheme.
    Cross,
}

impl Text {
    /// An empty text.
    pub fn new() -> Self {
        Self::default()
    }

    /// The length in bytes.
    pub fn len(&self) -> usize {
        self.rope.byte_len()
    }

    pub fn is_empty(&self) -> bool {
        self.rope.is_empty()
    }

    /// Whether `offset` is a char boundary. The start and the end of the text are boundaries.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text.
    pub fn is_char_boundary(&self, offset: usize) -> bool {
        self.rope.is_char_boundary(offset)
    }

    /// The number of lines: one more than the number of line breaks (ADR 0028). `\n` and `\r\n`
    /// break lines; a lone `\r` doesn't. `"foo\n"` has two lines, the second one empty, and an
    /// empty text has one.
    pub fn line_count(&self) -> usize {
        // Not crop's `line_len`, which leaves out the empty line after a final break.
        self.rope.line_of_byte(self.len()) + 1
    }

    /// The line `offset` is on, counting from 0. Every offset from 0 to the length is on exactly
    /// one line; a line break, and an offset inside a `\r\n`, belong to the line the break ends.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn line_of(&self, offset: usize) -> usize {
        self.rope.line_of_byte(offset)
    }

    /// Where `line` starts: at 0, or right after the line break before it. The line after the
    /// last one starts at the end of the text, so lines `a..b`, with their breaks, cover
    /// `line_start(a)..line_start(b)`.
    ///
    /// # Panics
    ///
    /// Panics if `line` is past [`line_count`](Self::line_count).
    pub fn line_start(&self, line: usize) -> usize {
        let count = self.line_count();
        assert!(
            line <= count,
            "line {line} is past the end of the text ({count} lines)",
        );
        if line == count {
            self.len()
        } else {
            self.rope.byte_of_line(line)
        }
    }

    /// Where the content of `line` ends: before its line break, or at the end of the text on the
    /// last line.
    ///
    /// # Panics
    ///
    /// Panics if `line` is not a line of the text: [`line_count`](Self::line_count) or more.
    pub fn line_end(&self, line: usize) -> usize {
        let count = self.line_count();
        assert!(
            line < count,
            "line {line} is past the last line ({count} lines)",
        );
        if line + 1 == count {
            return self.len();
        }
        // The next line starts right after this line's break, a `\n` or a `\r\n`. The byte before
        // this line, if any, is a `\n`, so a `\r` found here is this line's.
        let next = self.rope.byte_of_line(line + 1);
        if next >= 2 && self.rope.byte(next - 2) == b'\r' {
            next - 2
        } else {
            next - 1
        }
    }

    /// Whether `offset` is a grapheme boundary, where a cursor may stop. A grapheme is what a
    /// reader sees as one character, Unicode's extended grapheme cluster: an `e` with a combining
    /// accent, a flag, an emoji sequence, or a `\r\n`. The start and the end of the text are
    /// boundaries; an offset inside a char isn't.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text.
    pub fn is_grapheme_boundary(&self, offset: usize) -> bool {
        if !self.is_char_boundary(offset) {
            return false;
        }
        // At the end of the text there is no chunk, and the cursor needs none.
        let chunk = self
            .rope
            .byte_slice(offset..)
            .chunks()
            .next()
            .unwrap_or_default();
        let mut cursor = GraphemeCursor::new(offset, self.len(), true);
        loop {
            match cursor.is_boundary(chunk, offset) {
                Ok(is_boundary) => return is_boundary,
                Err(GraphemeIncomplete::PreContext(end)) => self.provide_context(&mut cursor, end),
                Err(error) => unreachable!("grapheme cursor in its own chunk: {error:?}"),
            }
        }
    }

    /// The first grapheme boundary after `offset`, or the end of the text if `offset` is there.
    /// From inside a grapheme, that is where the grapheme ends.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    #[expect(
        clippy::expect_used,
        reason = "the cursor asks for the next chunk only before the end of the text"
    )]
    pub fn next_grapheme_boundary(&self, offset: usize) -> usize {
        let len = self.len();
        let mut chunks = self.rope.byte_slice(offset..).chunks();
        let mut chunk = chunks.next().unwrap_or_default();
        let mut chunk_start = offset;
        let mut cursor = GraphemeCursor::new(offset, len, true);
        loop {
            match cursor.next_boundary(chunk, chunk_start) {
                Ok(boundary) => return boundary.unwrap_or(len),
                Err(GraphemeIncomplete::NextChunk) => {
                    // The cursor needs the next chunk to decide this chunk's end. One that gets a
                    // second chunk can count a regional indicator twice (unicode-segmentation
                    // 1.13.3) and split a flag, so a fresh cursor decides, and a new one goes on.
                    chunk_start += chunk.len();
                    if self.is_grapheme_boundary(chunk_start) {
                        return chunk_start;
                    }
                    chunk = chunks.next().expect("the text goes on after this chunk");
                    cursor = GraphemeCursor::new(chunk_start, len, true);
                }
                Err(GraphemeIncomplete::PreContext(end)) => self.provide_context(&mut cursor, end),
                Err(error) => unreachable!("grapheme cursor going forward: {error:?}"),
            }
        }
    }

    /// The last grapheme boundary before `offset`, or 0 if `offset` is 0. From inside a grapheme,
    /// that is where the grapheme starts.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    #[expect(
        clippy::expect_used,
        reason = "the cursor asks for the previous chunk only after the start of the text"
    )]
    pub fn prev_grapheme_boundary(&self, offset: usize) -> usize {
        let mut chunks = self.rope.byte_slice(..offset).chunks();
        let mut chunk = chunks.next_back().unwrap_or_default();
        let mut chunk_start = offset - chunk.len();
        let mut cursor = GraphemeCursor::new(offset, self.len(), true);
        loop {
            match cursor.prev_boundary(chunk, chunk_start) {
                Ok(boundary) => return boundary.unwrap_or(0),
                Err(GraphemeIncomplete::PrevChunk) => {
                    chunk = chunks
                        .next_back()
                        .expect("the text goes on before this chunk");
                    chunk_start -= chunk.len();
                }
                Err(GraphemeIncomplete::PreContext(end)) => self.provide_context(&mut cursor, end),
                Err(error) => unreachable!("grapheme cursor going backward: {error:?}"),
            }
        }
    }

    /// Gives `cursor` the chunk that ends at `end`. It asks for one when a rule needs the chars
    /// before the chunks it has, such as the regional indicators before a flag.
    #[expect(
        clippy::expect_used,
        reason = "the cursor asks for context only before an offset past the start of the text"
    )]
    fn provide_context(&self, cursor: &mut GraphemeCursor, end: usize) {
        let chunk = self
            .rope
            .byte_slice(..end)
            .chunks()
            .next_back()
            .expect("there is text before `end`");
        cursor.provide_context(chunk, end - chunk.len());
    }

    /// Moves `count` graphemes forward from `offset`, as `l` does. With [`LineEdges::Stop`] it
    /// stops at the end of the line's content, before its line break; with
    /// [`LineEdges::Cross`], at the end of the text. Returns `offset` if it can't move.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn next_grapheme(&self, offset: usize, count: usize, edges: LineEdges) -> usize {
        let end = match edges {
            LineEdges::Stop => self.line_end(self.line_of(offset)),
            LineEdges::Cross => {
                // In the other branch, `line_of` checks the offset.
                assert!(
                    self.is_char_boundary(offset),
                    "offset {offset} is inside a char"
                );
                self.len()
            }
        };
        let mut target = offset;
        for _ in 0..count {
            if target >= end {
                break;
            }
            target = self.next_grapheme_boundary(target);
        }
        target
    }

    /// Moves `count` graphemes back from `offset`, as `h` does. With [`LineEdges::Stop`] it
    /// stops at the start of the line; with [`LineEdges::Cross`], at the start of the text.
    /// Returns `offset` if it can't move.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn prev_grapheme(&self, offset: usize, count: usize, edges: LineEdges) -> usize {
        let start = match edges {
            LineEdges::Stop => self.line_start(self.line_of(offset)),
            LineEdges::Cross => {
                // In the other branch, `line_of` checks the offset.
                assert!(
                    self.is_char_boundary(offset),
                    "offset {offset} is inside a char"
                );
                0
            }
        };
        let mut target = offset;
        for _ in 0..count {
            if target <= start {
                break;
            }
            target = self.prev_grapheme_boundary(target);
        }
        target
    }

    /// Where the first grapheme of `line` that isn't blank starts, as `^` finds it, or the end of
    /// the line's content if all of it is blank. Blank is Unicode whitespace: spaces, tabs,
    /// no-break spaces, the ideographic space. A grapheme with any other char, such as a space
    /// with a combining accent, isn't blank.
    ///
    /// # Panics
    ///
    /// Panics if `line` is not a line of the text: [`line_count`](Self::line_count) or more.
    pub fn first_non_blank(&self, line: usize) -> usize {
        let end = self.line_end(line);
        let start = self.line_start(line);
        let mut offset = start;
        for ch in self.rope.byte_slice(start..end).chars() {
            if !ch.is_whitespace() {
                // Blanks before it can share its grapheme, as a space does with an accent.
                return if self.is_grapheme_boundary(offset) {
                    offset
                } else {
                    self.prev_grapheme_boundary(offset)
                };
            }
            offset += ch.len_utf8();
        }
        end
    }

    /// Moves `count` lines down from `offset`, as `j` does: to the grapheme `goal` graphemes from
    /// the start of the line, or to the end of the line's content if the line is shorter. Without
    /// a goal, the goal is `offset`'s column: the graphemes from its line's start to it.
    ///
    /// Returns the target and the goal. The caller keeps the goal while vertical moves repeat
    /// (ADR 0032), so a move through a short line comes back to the column it started from; the
    /// goal `usize::MAX` keeps to the ends of lines, as Vim does after `$`. A count past the last
    /// line stops there; on the last line, the target is `offset`.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn line_down(&self, offset: usize, count: usize, goal: Option<usize>) -> (usize, usize) {
        let line = self.line_of(offset);
        let last = self.line_count() - 1;
        self.move_to_line(offset, line, line.saturating_add(count).min(last), goal)
    }

    /// Moves `count` lines up from `offset`, as `k` does; see [`line_down`](Self::line_down). A
    /// count past the first line stops there; on the first line, the target is `offset`.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is past the end of the text or inside a char.
    pub fn line_up(&self, offset: usize, count: usize, goal: Option<usize>) -> (usize, usize) {
        let line = self.line_of(offset);
        self.move_to_line(offset, line, line.saturating_sub(count), goal)
    }

    /// Moves from `offset`, on line `from`, to the grapheme at `goal` on line `to`; see
    /// [`line_down`](Self::line_down).
    fn move_to_line(
        &self,
        offset: usize,
        from: usize,
        to: usize,
        goal: Option<usize>,
    ) -> (usize, usize) {
        // Segmenting a copy of the line costs about a seventh of stepping through it with
        // `next_grapheme_boundary`: 17 ns per grapheme against 120 to 185.
        let goal = goal.unwrap_or_else(|| {
            let start = self.line_start(from);
            self.to_string_in(start..offset).graphemes(true).count()
        });
        if to == from {
            return (offset, goal);
        }
        let (start, end) = (self.line_start(to), self.line_end(to));
        let target = self
            .to_string_in(start..end)
            .grapheme_indices(true)
            .nth(goal)
            .map_or(end, |(index, _)| start + index);
        (target, goal)
    }

    /// Copies the bytes in `range` into a new string.
    pub(crate) fn to_string_in(&self, range: Range<usize>) -> String {
        let mut string = String::with_capacity(range.len());
        for chunk in self.rope.byte_slice(range).chunks() {
            string.push_str(chunk);
        }
        string
    }

    /// Replaces the bytes in `range` with `text`.
    pub(crate) fn replace(&mut self, range: Range<usize>, text: &str) {
        self.rope.replace(range, text);
    }
}

impl From<&str> for Text {
    fn from(text: &str) -> Self {
        Self {
            rope: Rope::from(text),
        }
    }
}

impl From<String> for Text {
    fn from(text: String) -> Self {
        Self {
            rope: Rope::from(text),
        }
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.rope, f)
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

    proptest! {
        #[test]
        fn lines_match_splitting_at_line_feeds(string in strategies::text()) {
            let text = Text::from(string.as_str());
            // Every `\n` ends a line, together with a `\r` right before it.
            let lines: Vec<&str> = string.split('\n').collect();
            prop_assert_eq!(text.line_count(), lines.len());
            let last = lines.len() - 1;
            let mut start = 0;
            for (line, piece) in lines.iter().enumerate() {
                let content = if line == last {
                    piece
                } else {
                    piece.strip_suffix('\r').unwrap_or(piece)
                };
                prop_assert_eq!(text.line_start(line), start);
                prop_assert_eq!(text.line_end(line), start + content.len());
                // The line's offsets run up to its `\n`, or to the end of the text.
                for offset in start..=start + piece.len() {
                    if string.is_char_boundary(offset) {
                        prop_assert_eq!(text.line_of(offset), line);
                    }
                }
                start += piece.len() + 1;
            }
            prop_assert_eq!(text.line_start(lines.len()), string.len());
        }

        #[test]
        fn graphemes_match_segmenting_a_string(
            string in prop_oneof![strategies::text(), runs()],
        ) {
            let text = Text::from(string.as_str());
            let boundaries: Vec<usize> = string
                .grapheme_indices(true)
                .map(|(offset, _)| offset)
                .chain([string.len()])
                .collect();
            // Every offset of a short text. A long one checks the offsets near its chunk edges,
            // where a grapheme, or the chars that decide it, start to cross chunks: checking every
            // offset in a long run would take quadratic time.
            let edges = chunk_edges(&text);
            let near_an_edge = |offset: usize| edges.iter().any(|edge| edge.abs_diff(offset) <= 16);
            let checked =
                (0..=string.len()).filter(|&offset| string.len() <= 1024 || near_an_edge(offset));
            for offset in checked {
                if !string.is_char_boundary(offset) {
                    prop_assert!(!text.is_grapheme_boundary(offset));
                    continue;
                }
                // The boundaries after `offset` start at `later`; those before it end at `earlier`.
                let later = boundaries.partition_point(|&boundary| boundary <= offset);
                let earlier = boundaries.partition_point(|&boundary| boundary < offset);
                let next = boundaries.get(later).copied().unwrap_or(string.len());
                let prev = earlier.checked_sub(1).map_or(0, |index| boundaries[index]);
                prop_assert_eq!(
                    text.is_grapheme_boundary(offset),
                    boundaries.binary_search(&offset).is_ok(),
                    "at {}",
                    offset
                );
                prop_assert_eq!(text.next_grapheme_boundary(offset), next, "after {}", offset);
                prop_assert_eq!(text.prev_grapheme_boundary(offset), prev, "before {}", offset);
            }
        }

        #[test]
        fn grapheme_motions_step_through_boundaries(
            (string, offset) in strategies::text_and_grapheme_boundary(),
            count in 0..6_usize,
        ) {
            let text = Text::from(string.as_str());
            let boundaries = strategies::grapheme_boundaries(&string);
            let index = boundaries.binary_search(&offset).unwrap();
            let forward = boundaries[(index + count).min(boundaries.len() - 1)];
            let backward = boundaries[index.saturating_sub(count)];
            // The edges of the line: after the `\n` before `offset`, and before the break after it.
            let line_start = string[..offset].rfind('\n').map_or(0, |at| at + 1);
            let line_end = string[offset..].find('\n').map_or(string.len(), |at| {
                let at = offset + at;
                if string[..at].ends_with('\r') { at - 1 } else { at }
            });
            let (stop, cross) = (LineEdges::Stop, LineEdges::Cross);
            prop_assert_eq!(text.next_grapheme(offset, count, cross), forward);
            prop_assert_eq!(text.next_grapheme(offset, count, stop), forward.min(line_end));
            prop_assert_eq!(text.prev_grapheme(offset, count, cross), backward);
            prop_assert_eq!(text.prev_grapheme(offset, count, stop), backward.max(line_start));
        }

        #[test]
        fn first_non_blank_skips_blank_graphemes(string in blanks()) {
            let text = Text::from(string.as_str());
            let lines: Vec<&str> = string.split('\n').collect();
            let last = lines.len() - 1;
            let mut start = 0;
            for (line, piece) in lines.iter().enumerate() {
                let content = if line == last {
                    piece
                } else {
                    piece.strip_suffix('\r').unwrap_or(piece)
                };
                let first = content
                    .grapheme_indices(true)
                    .find(|(_, grapheme)| !grapheme.chars().all(char::is_whitespace))
                    .map_or(content.len(), |(at, _)| at);
                prop_assert_eq!(text.first_non_blank(line), start + first, "line {}", line);
                start += piece.len() + 1;
            }
        }

        #[test]
        fn vertical_motions_keep_the_goal_column(
            (string, offset) in strategies::text_and_grapheme_boundary(),
            count in 0..4_usize,
            goal in prop_oneof![Just(None), (0..12_usize).prop_map(Some), Just(Some(usize::MAX))],
            down in any::<bool>(),
        ) {
            let text = Text::from(string.as_str());
            let pieces: Vec<&str> = string.split('\n').collect();
            let starts: Vec<usize> = pieces
                .iter()
                .scan(0, |start, piece| {
                    let this = *start;
                    *start += piece.len() + 1;
                    Some(this)
                })
                .collect();
            // A line's content: a `\r` before its `\n` belongs to the break.
            let content = |line: usize| {
                let piece = pieces[line];
                if line + 1 == pieces.len() {
                    piece
                } else {
                    piece.strip_suffix('\r').unwrap_or(piece)
                }
            };
            // Where the grapheme at `goal` starts on `line`, or where its content ends.
            let at_goal = |line: usize, goal: usize| {
                let content = content(line);
                let graphemes = content.grapheme_indices(true);
                starts[line] + graphemes.map(|(at, _)| at).nth(goal).unwrap_or(content.len())
            };
            let line = starts.partition_point(|&start| start <= offset) - 1;
            let column = string[starts[line]..offset].graphemes(true).count();
            let kept = goal.unwrap_or(column);
            let to = if down {
                (line + count).min(pieces.len() - 1)
            } else {
                line.saturating_sub(count)
            };
            let expected = if to == line { offset } else { at_goal(to, kept) };
            let moved = if down {
                text.line_down(offset, count, goal)
            } else {
                text.line_up(offset, count, goal)
            };
            prop_assert_eq!(moved, (expected, kept));
            // Moving back with the goal kept returns to `offset` if the line it moved to is long
            // enough to reach its column.
            if goal.is_none() && to != line && content(to).graphemes(true).count() >= column {
                let back = if down {
                    text.line_up(moved.0, to - line, Some(kept))
                } else {
                    text.line_down(moved.0, line - to, Some(kept))
                };
                prop_assert_eq!(back, (offset, kept));
            }
        }
    }

    /// The offsets where the chunks of `text` start and end.
    fn chunk_edges(text: &Text) -> Vec<usize> {
        let mut edges = vec![0];
        for chunk in text.rope.chunks() {
            edges.push(edges[edges.len() - 1] + chunk.len());
        }
        edges
    }

    /// Lines of blanks and of chars that aren't: Unicode spaces, a combining accent that joins
    /// the char before it, and line breaks.
    fn blanks() -> impl Strategy<Value = String> {
        let piece = select(
            &[
                " ", "\t", "\u{a0}", "\u{3000}", "\u{301}", "x", "日", "\n", "\r\n", "\r",
            ][..],
        );
        vec(piece, 0..24).prop_map(|pieces| pieces.concat())
    }

    /// A text around a run of a char that joins the one before it: a regional indicator, which
    /// pairs into flags, a combining accent, or an emoji after a zero-width joiner. A rope chunk
    /// holds 2 KiB at most, so a run of 2 to 6 KiB spans chunks, and so does a grapheme in it, or
    /// the chars that decide where it ends. A run of flags, each after a prepended char (U+0600
    /// ARABIC NUMBER SIGN), puts chunk edges inside graphemes that start with another char.
    fn runs() -> impl Strategy<Value = String> {
        let run = (
            select(&["\u{1f1e9}", "\u{301}", "\u{200d}🐻", "\u{600}🇩🇪"][..]),
            2048..6144_usize,
        )
            .prop_map(|(piece, bytes)| piece.repeat(bytes / piece.len()));
        (strategies::text(), run, strategies::text())
            .prop_map(|(before, run, after)| before + &run + &after)
    }
}
