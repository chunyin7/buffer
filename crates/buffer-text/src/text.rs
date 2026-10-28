use std::fmt;
use std::ops::Range;

use crop::Rope;

/// UTF-8 text in a rope, addressed by byte offsets.
///
/// Clones are cheap and share structure (copy-on-write), so background work can read a snapshot
/// while the UI thread keeps editing. The only way to change a `Text` is
/// [`ChangeSet::apply`](crate::ChangeSet::apply), so every edit can be recorded and inverted.
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
