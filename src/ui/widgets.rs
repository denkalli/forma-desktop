//! Small reusable widgets in the Forma style.

use eframe::egui::{self, Color32, Response, RichText, Sense, Ui, Vec2};
use libforma::paint::Color;

use crate::theme::Theme;
use crate::theme::egui::{SEMIBOLD, c};
use crate::theme::icons::Icon;

/// A square icon button. Selected buttons are filled with the accent color.
pub fn icon_button(ui: &mut Ui, theme: &Theme, i: Icon, tooltip: &str, selected: bool) -> Response {
    icon_button_sized(ui, theme, i, tooltip, selected, Vec2::splat(30.0))
}

/// An icon button of a given size.
pub fn icon_button_sized(
    ui: &mut Ui,
    theme: &Theme,
    i: Icon,
    tooltip: &str,
    selected: bool,
    size: Vec2,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (bg, fg) = if selected {
        (c(theme.accent), c(theme.on_accent))
    } else if response.hovered() {
        (c(theme.raised), c(theme.text))
    } else {
        (Color32::TRANSPARENT, c(theme.text))
    };
    ui.painter().rect_filled(rect, theme.radius as f32, bg);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        char::from(i),
        egui::FontId::new(
            16.0,
            egui::FontFamily::Name(crate::theme::icons::FAMILY.into()),
        ),
        fg,
    );
    response.on_hover_text(tooltip)
}

/// A small icon-only button without a frame, for panel headers and layer rows.
pub fn small_icon_button(ui: &mut Ui, theme: &Theme, i: Icon, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, theme.radius_small as f32, c(theme.raised));
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        char::from(i),
        egui::FontId::new(
            13.0,
            egui::FontFamily::Name(crate::theme::icons::FAMILY.into()),
        ),
        c(if response.hovered() {
            theme.text
        } else {
            theme.text_muted
        }),
    );
    response.on_hover_text(tooltip)
}

/// A button filled with the accent color.
pub fn accent_button<'a>(
    ui: &mut Ui,
    theme: &Theme,
    content: impl egui::IntoAtoms<'a>,
) -> Response {
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        for (state, fill) in [
            (&mut w.inactive, theme.accent),
            (&mut w.hovered, theme.accent_hover),
            (&mut w.active, theme.accent_active),
        ] {
            state.weak_bg_fill = c(fill);
            state.bg_fill = c(fill);
            state.bg_stroke = egui::Stroke::NONE;
            state.fg_stroke = egui::Stroke::new(1.0, c(theme.on_accent));
            state.corner_radius = egui::CornerRadius::same(theme.radius);
        }
        ui.visuals_mut().override_text_color = Some(c(theme.on_accent));
        ui.add(egui::Button::new(content))
    })
    .inner
}

/// Section title of a panel, with optional buttons aligned to the right edge.
pub fn section_header(ui: &mut Ui, title: &str, add_right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(24.0);
        ui.label(
            RichText::new(title)
                .family(egui::FontFamily::Name(SEMIBOLD.into()))
                .size(11.5),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add_right);
    });
    ui.add_space(4.0);
}

/// A horizontal separator in the border color, spanning the panel.
pub fn separator(ui: &mut Ui, theme: &Theme) {
    let rect = ui.available_rect_before_wrap();
    let y = rect.top() + 4.0;
    let full = ui.max_rect();
    ui.painter().hline(
        full.x_range().expand(16.0),
        y,
        egui::Stroke::new(1.0, c(theme.border)),
    );
    ui.add_space(9.0);
}

/// Height of fields in the properties panel.
pub const FIELD_HEIGHT: f32 = 26.0;

/// Lays out `N` widgets side by side in equal columns spanning the available width, like
/// justified text. Each closure gets a column to fill.
pub fn field_row<const N: usize>(ui: &mut Ui, add: impl FnOnce(&mut [Ui; N])) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.columns_const(|cols: &mut [Ui; N]| add(cols));
    });
    ui.add_space(2.0);
}

/// The frame of a field: a filled box spanning the column, with a short label in front.
fn field_frame<R>(ui: &mut Ui, theme: &Theme, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::NONE
        .fill(c(theme.raised))
        .corner_radius(theme.radius_small)
        .inner_margin(egui::Margin::symmetric(7, 0))
        .show(ui, |ui| {
            row(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if !label.is_empty() {
                    ui.label(RichText::new(label).color(c(theme.text_faint)).size(11.0));
                }
                // The value takes the rest of the field.
                ui.spacing_mut().interact_size.x = ui.available_width();
                add(ui)
            })
        })
        .inner
}

/// A row of the field height spanning the available width, contents centered vertically.
fn row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let size = Vec2::new(ui.available_width(), FIELD_HEIGHT);
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(size);
            add(ui)
        },
    )
    .inner
}

/// A number field with a short label in front, dragged or typed ("X", "W", "°"). It spans
/// the width it is given.
pub fn number_field(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    value: &mut f64,
    speed: f64,
    suffix: &str,
) -> Response {
    field_frame(ui, theme, label, |ui| {
        ui.add(
            egui::DragValue::new(value)
                .speed(speed)
                .max_decimals(2)
                .suffix(suffix),
        )
    })
}

/// A whole-number field (sides, points), shown without decimals.
pub fn int_field(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
) -> Response {
    field_frame(ui, theme, label, |ui| {
        ui.add(egui::DragValue::new(value).speed(0.1).range(range))
    })
}

/// A drop-down list spanning the width it is given. Returns `true` when the value changed.
pub fn select<T: PartialEq + Copy>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    value: &mut T,
    options: &[(T, &str)],
) -> bool {
    let before = *value;
    let current = options
        .iter()
        .find(|(v, _)| v == value)
        .map_or("", |(_, label)| label);
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(current)
        .width(ui.available_width())
        .height(FIELD_HEIGHT * 12.0)
        .show_ui(ui, |ui| {
            for (option, label) in options {
                ui.selectable_value(value, *option, *label);
            }
        });
    *value != before
}

/// Icon buttons sharing the available width equally, like a segmented control. `items` are
/// (icon, tooltip, selected). Returns the index of the clicked button.
pub fn icon_strip(ui: &mut Ui, theme: &Theme, items: &[(Icon, &str, bool)]) -> Option<usize> {
    let width = ui.available_width() / items.len().max(1) as f32;
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, (icon, tip, selected)) in items.iter().enumerate() {
            if icon_button_sized(ui, theme, *icon, tip, *selected, Vec2::new(width, 28.0)).clicked()
            {
                clicked = Some(i);
            }
        }
    });
    clicked
}

/// Converts a token color to a color picker value and back.
fn to_picker(color: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

fn from_picker(color: Color32) -> Color {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Color::rgba(r, g, b, a)
}

/// A color field: swatch (opens a picker), hex value and opacity, like "■ 1E1E1E  100 %".
///
/// Returns `(response, changed)`; the response covers the whole field so callers can detect
/// when an edit starts and stops.
pub fn color_field(
    ui: &mut Ui,
    theme: &Theme,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    color: &mut Color,
) -> (Response, bool) {
    let mut changed = false;
    let id = ui.make_persistent_id(id_salt);
    let inner = egui::Frame::NONE
        .fill(c(theme.raised))
        .corner_radius(theme.radius_small)
        .inner_margin(egui::Margin::symmetric(6, 0))
        .show(ui, |ui| {
            row(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (swatch_rect, swatch) =
                    ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
                checkerboard(ui, swatch_rect);
                ui.painter()
                    .rect_filled(swatch_rect, 2.0, to_picker(*color));
                ui.painter().rect_stroke(
                    swatch_rect,
                    2.0,
                    egui::Stroke::new(1.0, c(theme.border)),
                    egui::StrokeKind::Inside,
                );
                egui::Popup::menu(&swatch)
                    .id(id.with("picker"))
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        let mut picked = to_picker(*color);
                        if egui::color_picker::color_picker_color32(
                            ui,
                            &mut picked,
                            egui::color_picker::Alpha::OnlyBlend,
                        ) {
                            *color = from_picker(picked);
                            changed = true;
                        }
                    });

                // The opacity sits at the right edge and the hex code takes what is left, so
                // the field never asks for more than its width.
                let (hex_response, alpha_response) = ui
                    .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut alpha = (f64::from(color.a) / 255.0 * 100.0).round();
                        let alpha_response = ui.add(
                            egui::DragValue::new(&mut alpha)
                                .range(0.0..=100.0)
                                .speed(0.5)
                                .max_decimals(0)
                                .suffix(" %"),
                        );
                        if alpha_response.changed() {
                            color.a = (alpha / 100.0 * 255.0).round() as u8;
                            changed = true;
                        }
                        let mut hex = format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b);
                        let hex_response = ui.add(
                            egui::TextEdit::singleline(&mut hex)
                                .desired_width(ui.available_width())
                                .frame(egui::Frame::NONE)
                                .char_limit(7),
                        );
                        if hex_response.changed()
                            && let Some(parsed) = Color::from_hex(hex.trim())
                        {
                            *color = parsed.with_alpha(color.a);
                            changed = true;
                        }
                        (hex_response, alpha_response)
                    })
                    .inner;
                swatch.union(hex_response).union(alpha_response)
            })
        });
    (inner.inner, changed)
}

fn checkerboard(ui: &Ui, rect: egui::Rect) {
    let half = rect.width() / 2.0;
    ui.painter().rect_filled(rect, 2.0, Color32::from_gray(230));
    for (dx, dy) in [(0.0, 0.0), (half, half)] {
        let r = egui::Rect::from_min_size(rect.min + egui::vec2(dx, dy), Vec2::splat(half));
        ui.painter().rect_filled(r, 0.0, Color32::from_gray(160));
    }
}

/// Text in the semibold font.
pub fn strong(text: impl Into<String>) -> RichText {
    RichText::new(text).family(egui::FontFamily::Name(SEMIBOLD.into()))
}
