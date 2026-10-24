//! Interface icons, drawn with the Lucide icon font.

use eframe::egui::{FontFamily, RichText};
pub use lucide_icons::Icon;

/// Name of the egui font family holding the icon font.
pub const FAMILY: &str = "icons";

/// The icon as text, ready to put in a label or button.
pub fn icon(icon: Icon) -> RichText {
    RichText::new(char::from(icon).to_string()).family(FontFamily::Name(FAMILY.into()))
}
