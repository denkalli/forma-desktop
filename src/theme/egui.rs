//! Translates the design tokens into egui's style.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle,
};
use libforma::paint::Color;

use super::icons::FAMILY as ICON_FAMILY;
use super::tokens::{Theme, ThemeMode};

/// Font family for semibold text (headings, emphasis).
pub const SEMIBOLD: &str = "semibold";
/// Font family for medium-weight text.
pub const MEDIUM: &str = "medium";

/// Converts a token color to egui.
pub fn c(color: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

/// Registers Inter and the icon font. Call once at startup.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, data: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(data)));
    };
    add(
        "inter",
        include_bytes!("../../assets/fonts/Inter-Regular.ttf"),
    );
    add(
        "inter-medium",
        include_bytes!("../../assets/fonts/Inter-Medium.ttf"),
    );
    add(
        "inter-semibold",
        include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"),
    );
    add("lucide", lucide_icons::LUCIDE_FONT_BYTES);

    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "inter".to_owned());
    // Icons are drawn with their own family (see `icons::icon`): Inter has glyphs in the
    // same private-use code points, and Lucide maps letters to empty glyphs.
    let fallbacks: Vec<String> = proportional.clone();

    for (family, primary) in [(MEDIUM, "inter-medium"), (SEMIBOLD, "inter-semibold")] {
        let mut list = vec![primary.to_owned()];
        list.extend(fallbacks.iter().skip(1).cloned());
        fonts.families.insert(FontFamily::Name(family.into()), list);
    }
    fonts.families.insert(
        FontFamily::Name(ICON_FAMILY.into()),
        vec!["lucide".to_owned()],
    );
    ctx.set_fonts(fonts);
}

/// Applies the theme to every egui style.
pub fn apply(ctx: &egui::Context, theme: &Theme) {
    let theme = theme.clone();
    ctx.all_styles_mut(move |style| {
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(13.0, FontFamily::Name(SEMIBOLD.into())),
            ),
            (TextStyle::Body, FontId::new(12.5, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(12.5, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(11.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(12.0, FontFamily::Monospace),
            ),
        ]
        .into();

        let spacing = &mut style.spacing;
        spacing.item_spacing = egui::vec2(6.0, 4.0);
        spacing.button_padding = egui::vec2(8.0, 4.0);
        spacing.interact_size = egui::vec2(28.0, 26.0);
        spacing.window_margin = Margin::same(12);
        spacing.menu_margin = Margin::same(6);
        spacing.combo_width = 120.0;
        spacing.slider_width = 120.0;

        let v = &mut style.visuals;
        *v = match theme.mode {
            ThemeMode::Dark => egui::Visuals::dark(),
            ThemeMode::Light => egui::Visuals::light(),
        };
        let small = CornerRadius::same(theme.radius_small);
        let normal = CornerRadius::same(theme.radius);
        v.panel_fill = c(theme.panel);
        v.window_fill = c(theme.panel);
        v.window_stroke = Stroke::new(1.0, c(theme.border));
        v.window_corner_radius = CornerRadius::same(theme.radius_large);
        v.menu_corner_radius = normal;
        v.window_shadow = Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(if theme.mode == ThemeMode::Dark {
                120
            } else {
                40
            }),
        };
        v.popup_shadow = Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(if theme.mode == ThemeMode::Dark {
                100
            } else {
                30
            }),
        };
        v.extreme_bg_color = c(theme.raised);
        v.text_edit_bg_color = Some(c(theme.raised));
        v.faint_bg_color = c(theme.raised);
        v.code_bg_color = c(theme.raised);
        v.hyperlink_color = c(theme.accent);
        v.error_fg_color = c(theme.danger);
        v.selection.bg_fill = c(theme.accent).linear_multiply(0.45);
        v.selection.stroke = Stroke::new(1.0, c(theme.accent));
        v.text_cursor.stroke = Stroke::new(1.5, c(theme.accent));
        v.slider_trailing_fill = true;
        v.indent_has_left_vline = false;

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = c(theme.panel);
        w.noninteractive.weak_bg_fill = c(theme.panel);
        w.noninteractive.bg_stroke = Stroke::new(1.0, c(theme.border));
        w.noninteractive.fg_stroke = Stroke::new(1.0, c(theme.text));
        w.noninteractive.corner_radius = small;

        w.inactive.bg_fill = c(theme.raised);
        w.inactive.weak_bg_fill = c(theme.raised);
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0, c(theme.text));
        w.inactive.corner_radius = small;

        w.hovered.bg_fill = c(theme.raised);
        w.hovered.weak_bg_fill = c(super::tokens::lighten(
            theme.raised,
            if theme.mode == ThemeMode::Dark {
                0.06
            } else {
                -0.04
            },
        ));
        w.hovered.bg_stroke = Stroke::new(1.0, c(theme.border));
        w.hovered.fg_stroke = Stroke::new(1.0, c(theme.text));
        w.hovered.corner_radius = small;
        w.hovered.expansion = 0.0;

        w.active.bg_fill = c(theme.accent_soft);
        w.active.weak_bg_fill = c(theme.accent_soft);
        w.active.bg_stroke = Stroke::new(1.0, c(theme.accent));
        w.active.fg_stroke = Stroke::new(1.0, c(theme.text));
        w.active.corner_radius = small;
        w.active.expansion = 0.0;

        w.open.bg_fill = c(theme.raised);
        w.open.weak_bg_fill = c(theme.raised);
        w.open.bg_stroke = Stroke::new(1.0, c(theme.border));
        w.open.fg_stroke = Stroke::new(1.0, c(theme.text));
        w.open.corner_radius = small;
    });
}
