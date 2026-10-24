//! Visual theme. Colors and sizes are defined as toolkit-independent tokens in [`tokens`];
//! [`egui`] is the only module that translates them for the GUI toolkit.

pub mod egui;
pub mod icons;
pub mod tokens;

pub use tokens::{Theme, ThemeMode};
