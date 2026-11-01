use std::fmt;
use std::ops::Range;

use crop::Rope;

/// UTF-8 text in a rope, addressed by byte offsets.
///
/// Clones are cheap and share structure (copy-on-write), so background work can read a snapshot
/// while the UI thread keeps editing. Outside this crate, only a [`Document`](crate::Document)
/// changes a `Text`, so every edit lands in its history.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Text {
    rope: Rope,
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
    use proptest::prelude::*;

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
    }
}
