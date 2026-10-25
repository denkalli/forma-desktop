//! Design tokens: the colors and sizes of the interface, independent of any GUI toolkit.

use libforma::paint::Color;
use serde::{Deserialize, Serialize};

/// Light or dark variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

/// Default accent color: Forma violet.
pub const DEFAULT_ACCENT: Color = Color::rgb(0x7c, 0x5c, 0xff);

/// Accent presets offered in the settings, besides the free color picker.
pub const ACCENT_PRESETS: [Color; 8] = [
    DEFAULT_ACCENT,
    Color::rgb(0x0e, 0x8a, 0xff),
    Color::rgb(0x14, 0xb8, 0xa6),
    Color::rgb(0x22, 0xc5, 0x5e),
    Color::rgb(0xf5, 0x9e, 0x0b),
    Color::rgb(0xf9, 0x73, 0x16),
    Color::rgb(0xef, 0x44, 0x6f),
    Color::rgb(0xd9, 0x46, 0xef),
];

/// All the colors and sizes of the interface.
#[derive(Clone, PartialEq, Debug)]
pub struct Theme {
    pub mode: ThemeMode,
    /// Panels, rail and toolbars.
    pub panel: Color,
    /// Hovered rows and buttons, input fields.
    pub raised: Color,
    /// Background of the infinite canvas.
    pub canvas: Color,
    /// Default color for new text nodes on the canvas.
    pub canvas_text: Color,
    /// Dots of the canvas grid.
    pub canvas_grid: Color,
    /// Separators and outlines.
    pub border: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_faint: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_active: Color,
    /// Text drawn on top of the accent color.
    pub on_accent: Color,
    /// Accent blended into the panel color, for selected rows.
    pub accent_soft: Color,
    pub danger: Color,
    /// Corner radii.
    pub radius_small: u8,
    pub radius: u8,
    pub radius_large: u8,
}

impl Theme {
    pub fn new(mode: ThemeMode, accent: Color) -> Self {
        let base = match mode {
            ThemeMode::Dark => Self {
                mode,
                // Panels a step lighter than the canvas, with quiet borders.
                panel: Color::rgb(0x2a, 0x2a, 0x2c),
                raised: Color::rgb(0x38, 0x38, 0x3b),
                canvas: Color::rgb(0x1b, 0x1b, 0x1d),
                canvas_text: Color::WHITE,
                canvas_grid: Color::rgb(0x4e, 0x4e, 0x55),
                border: Color::rgb(0x3b, 0x3b, 0x3f),
                text: Color::rgb(0xee, 0xee, 0xf0),
                text_muted: Color::rgb(0xa3, 0xa3, 0xab),
                text_faint: Color::rgb(0x70, 0x70, 0x78),
                danger: Color::rgb(0xf0, 0x5a, 0x5a),
                ..Self::placeholder(mode)
            },
            ThemeMode::Light => Self {
                mode,
                panel: Color::rgb(0xff, 0xff, 0xff),
                raised: Color::rgb(0xf1, 0xf1, 0xf4),
                canvas: Color::rgb(0xe9, 0xe9, 0xed),
                canvas_text: Color::BLACK,
                canvas_grid: Color::rgb(0xb6, 0xb6, 0xc0),
                border: Color::rgb(0xe1, 0xe1, 0xe6),
                text: Color::rgb(0x1c, 0x1c, 0x20),
                text_muted: Color::rgb(0x66, 0x66, 0x70),
                text_faint: Color::rgb(0x9c, 0x9c, 0xa6),
                danger: Color::rgb(0xd9, 0x2d, 0x2d),
                ..Self::placeholder(mode)
            },
        };
        base.with_accent(accent)
    }

    fn placeholder(mode: ThemeMode) -> Self {
        Self {
            mode,
            panel: Color::BLACK,
            raised: Color::BLACK,
            canvas: Color::BLACK,
            canvas_text: Color::WHITE,
            canvas_grid: Color::BLACK,
            border: Color::BLACK,
            text: Color::WHITE,
            text_muted: Color::WHITE,
            text_faint: Color::WHITE,
            accent: DEFAULT_ACCENT,
            accent_hover: DEFAULT_ACCENT,
            accent_active: DEFAULT_ACCENT,
            on_accent: Color::WHITE,
            accent_soft: DEFAULT_ACCENT,
            danger: Color::BLACK,
            radius_small: 4,
            radius: 6,
            radius_large: 12,
        }
    }

    /// Sets the accent color and derives its variants.
    pub fn with_accent(mut self, accent: Color) -> Self {
        let accent = accent.with_alpha(255);
        self.accent = accent;
        let (lighter, darker) = (lighten(accent, 0.12), lighten(accent, -0.12));
        let (hover, active) = match self.mode {
            ThemeMode::Dark => (lighter, darker),
            ThemeMode::Light => (darker, lighten(accent, -0.22)),
        };
        self.accent_hover = hover;
        self.accent_active = active;
        // Whichever of white or near-black reads better on the accent.
        let dark = Color::rgb(0x12, 0x12, 0x14);
        self.on_accent = if contrast(accent, Color::WHITE) >= contrast(accent, dark) {
            Color::WHITE
        } else {
            dark
        };
        let soft_amount = match self.mode {
            ThemeMode::Dark => 0.28,
            ThemeMode::Light => 0.16,
        };
        self.accent_soft = mix(self.panel, accent, soft_amount);
        self
    }
}

/// Mixes `a` towards `b` by `t` (0 = a, 1 = b), in sRGB.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let lerp = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color::rgba(
        lerp(a.r, b.r),
        lerp(a.g, b.g),
        lerp(a.b, b.b),
        lerp(a.a, b.a),
    )
}

/// Moves a color towards white (`amount` > 0) or black (`amount` < 0).
pub fn lighten(c: Color, amount: f32) -> Color {
    if amount >= 0.0 {
        mix(c, Color::WHITE.with_alpha(c.a), amount)
    } else {
        mix(c, Color::BLACK.with_alpha(c.a), -amount)
    }
}

/// WCAG contrast ratio between two colors, 1 to 21.
pub fn contrast(a: Color, b: Color) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// WCAG relative luminance, 0 (black) to 1 (white).
pub fn relative_luminance(c: Color) -> f32 {
    let channel = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_text_stays_readable() {
        for accent in
            ACCENT_PRESETS
                .into_iter()
                .chain([Color::WHITE, Color::BLACK, Color::rgb(255, 255, 0)])
        {
            for mode in [ThemeMode::Dark, ThemeMode::Light] {
                let theme = Theme::new(mode, accent);
                let (l1, l2) = (
                    relative_luminance(theme.accent),
                    relative_luminance(theme.on_accent),
                );
                let contrast = (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05);
                assert!(contrast >= 3.0, "{accent} in {mode:?}: contrast {contrast}");
            }
        }
    }

    #[test]
    fn accent_alpha_is_ignored() {
        let theme = Theme::new(ThemeMode::Dark, Color::rgba(10, 20, 30, 0));
        assert_eq!(theme.accent.a, 255);
    }
}
