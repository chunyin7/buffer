//! Rope wrapper, transactions, selections, revision-tree history, motions, and text objects.
//!
//! Pure: no egui, typst, or platform APIs.

mod change;
mod document;
mod find;
mod history;
mod selection;
#[cfg(test)]
mod strategies;
mod text;
mod transaction;
mod words;

pub use change::{Assoc, ChangeSet, InvalidChange};
pub use document::Document;
pub use selection::{Range, Selection};
pub use text::{LineEdges, Text};
pub use words::{BigWords, UnicodeWords, WordKind, Words};
