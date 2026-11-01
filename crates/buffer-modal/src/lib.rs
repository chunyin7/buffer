//! Modes, command registry, keymap engine, and the Vim and Helix presets.
//!
//! Pure: no egui, typst, or platform APIs.

mod key;
mod keymap;

pub use key::{Key, KeyCode, Modifiers};
pub use keymap::{Binding, Keymap, Prefix};
