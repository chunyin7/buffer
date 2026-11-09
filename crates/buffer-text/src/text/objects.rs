use std::ops::Range;

use crate::Text;

/// How much of a text object to take (ADR 0038).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extent {
    /// The object alone: a word, sentence, or paragraph without the blanks around it, or what
    /// lies between brackets, quotes, or a pair of chars.
    Inside,
    /// A word, sentence, or paragraph with the blanks after it, or before it if none follow;
    /// brackets, quotes, and pairs of chars with the delimiters.
    Around,
}

impl Text {
    /// The span of a bracket, quote, or pair object whose delimiters start at `open` and
    /// `close`: `Inside` lies between them, `Around` includes them.
    pub(super) fn delimited(&self, open: usize, close: usize, extent: Extent) -> Range<usize> {
        match extent {
            Extent::Inside => self.grapheme_end(open)..close,
            Extent::Around => open..self.grapheme_end(close),
        }
    }
}

/// The span of a word, sentence, or paragraph object (ADR 0038). `units` are the units of the
/// text from the one at the offset on, each a span and whether it is blank, and `blanks_before`
/// gives where the blank unit right before the first starts, if there is one.
///
/// `Inside` takes `count` units, blank or not. `Around` takes `count` units that aren't blank and
/// the blank ones between them. From a unit that isn't blank, it adds the blank one after the
/// last, or the one before the first if none follows; from a blank one, it adds nothing more. A
/// count that overshoots stops at the last unit. `None` if there is no unit, or the count is 0.
pub(super) fn object_span(
    mut units: impl Iterator<Item = (Range<usize>, bool)>,
    blanks_before: impl FnOnce() -> Option<usize>,
    count: usize,
    extent: Extent,
) -> Option<Range<usize>> {
    let (first, blank) = units.next()?;
    if count == 0 {
        return None;
    }
    if extent == Extent::Inside {
        let end = units
            .take(count - 1)
            .last()
            .map_or(first.end, |(unit, _)| unit.end);
        return Some(first.start..end);
    }

    // The units that aren't blank still to take, where the last one taken ends, and where the
    // blank unit right after it ends, if the units went on to one.
    let mut left = if blank { count } else { count - 1 };
    let mut end = first.end;
    let mut blanks_after = None;
    for (unit, is_blank) in units {
        if is_blank {
            blanks_after = Some(unit.end);
            if left == 0 {
                break;
            }
        } else if left == 0 {
            break;
        } else {
            left -= 1;
            end = unit.end;
            blanks_after = None;
        }
    }
    if blank {
        return Some(first.start..end);
    }
    Some(match blanks_after {
        Some(after) => first.start..after,
        None => blanks_before().unwrap_or(first.start)..end,
    })
}

/// What [`object_span`] returns, worked out from all the units on the line or in the text and
/// the index of the one at the offset.
#[cfg(test)]
pub(super) fn model(
    units: &[(Range<usize>, bool)],
    at: usize,
    count: usize,
    extent: Extent,
) -> Option<Range<usize>> {
    let start = units.get(at)?.0.start;
    if count == 0 {
        return None;
    }
    let end = |index: usize| units[index].0.end;
    let blank = |index: usize| units[index].1;
    if extent == Extent::Inside {
        return Some(start..end((at + count - 1).min(units.len() - 1)));
    }
    let last = (at..units.len())
        .filter(|&index| !blank(index))
        .take(count)
        .last()
        .unwrap_or(at);
    Some(if blank(at) {
        start..end(last)
    } else if last + 1 < units.len() && blank(last + 1) {
        start..end(last + 1)
    } else if at > 0 && blank(at - 1) {
        units[at - 1].0.start..end(last)
    } else {
        start..end(last)
    })
}
