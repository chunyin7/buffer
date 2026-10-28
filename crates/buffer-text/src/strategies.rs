//! Proptest strategies shared by the unit tests.

use std::ops;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

use crate::Range;

/// A replacement: the byte range it replaces and the text it inserts.
pub type Change = (ops::Range<usize>, String);

/// Pieces of text: multi-byte chars, a combining mark, emoji sequences, and line breaks.
const PIECES: &[&str] = &[
    "a",
    "b",
    "xyz",
    " ",
    "\t",
    "\n",
    "\r\n",
    "\r",
    "é",
    "e\u{301}",
    "ß",
    "日本",
    "🇩🇪",
    "🐻\u{200d}❄\u{fe0f}",
];

/// A short text, or now and then one long enough to span several rope chunks.
pub fn text() -> impl Strategy<Value = String> {
    let pieces = || vec(select(PIECES), 0..64).prop_map(|pieces| pieces.concat());
    prop_oneof![
        9 => pieces(),
        1 => pieces().prop_map(|text| text.repeat(80)),
    ]
}

/// Text to insert: up to three pieces.
pub fn insert() -> impl Strategy<Value = String> {
    vec(select(PIECES), 0..4).prop_map(|pieces| pieces.concat())
}

/// The char boundaries of `text`, including its end.
pub fn char_boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(offset, _)| offset)
        .chain([text.len()])
        .collect()
}

/// Up to five changes to `text`: sorted, non-overlapping, and on char boundaries.
pub fn changes(text: &str) -> impl Strategy<Value = Vec<Change>> + use<> {
    let boundaries = char_boundaries(text);
    let change = (select(boundaries.clone()), select(boundaries), insert());
    vec(change, 0..6).prop_map(|changes| {
        let mut offsets: Vec<usize> = changes.iter().flat_map(|(a, b, _)| [*a, *b]).collect();
        offsets.sort_unstable();
        offsets
            .chunks(2)
            .zip(changes)
            .map(|(range, (_, _, insert))| (range[0]..range[1], insert))
            .collect()
    })
}

/// A text and changes to it.
pub fn text_and_changes() -> impl Strategy<Value = (String, Vec<Change>)> {
    text().prop_flat_map(|text| {
        let changes = changes(&text);
        (Just(text), changes)
    })
}

/// A text, changes to it, and changes to the result.
pub fn text_and_two_changes() -> impl Strategy<Value = (String, Vec<Change>, Vec<Change>)> {
    text_and_changes().prop_flat_map(|(text, first)| {
        let second = changes(&apply(&text, &first));
        (Just(text), Just(first), second)
    })
}

/// A primary range and up to four others, on char boundaries of `text`, pointing either way.
pub fn ranges(text: &str) -> impl Strategy<Value = (Range, Vec<Range>)> + use<> {
    let boundaries = char_boundaries(text);
    let range = (select(boundaries.clone()), select(boundaries))
        .prop_map(|(anchor, head)| Range::new(anchor, head));
    (range.clone(), vec(range, 0..5))
}

/// A text, changes to it, and a primary range and others in it.
pub fn text_changes_and_ranges() -> impl Strategy<Value = (String, Vec<Change>, (Range, Vec<Range>))>
{
    text().prop_flat_map(|text| {
        let changes = changes(&text);
        let ranges = ranges(&text);
        (Just(text), changes, ranges)
    })
}

/// Applies `changes` to `text` the obvious way: the model change sets must match.
pub fn apply(text: &str, changes: &[Change]) -> String {
    let mut result = String::new();
    let mut position = 0;
    for (range, insert) in changes {
        result.push_str(&text[position..range.start]);
        result.push_str(insert);
        position = range.end;
    }
    result.push_str(&text[position..]);
    result
}
