//! Rope wrapper, transactions, selections, revision-tree history, motions, and text objects.
//!
//! Pure: no egui, typst, or platform APIs.

mod change;
mod selection;
#[cfg(test)]
mod strategies;
mod text;
mod transaction;

pub use change::{Assoc, ChangeSet, InvalidChange};
pub use selection::{Range, Selection};
pub use text::Text;
pub use transaction::Transaction;
