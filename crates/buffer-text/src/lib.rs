//! Rope wrapper, transactions, selections, revision-tree history, motions, and text objects.
//!
//! Pure: no egui, typst, or platform APIs.

mod change;
#[cfg(test)]
mod strategies;
mod text;

pub use change::{Assoc, ChangeSet};
pub use text::Text;
