//! The narrow icon rail on the far left.

use eframe::egui::{self, Color32, RichText, Sense, Ui, Vec2};

use crate::app::{Action, RailTab};
use crate::i18n::tr;
use crate::theme::Theme;
use crate::theme::egui::c;
use crate::theme::icons::{FAMILY, Icon};

pub fn show(ui: &mut Ui, theme: &Theme, tab: &mut RailTab, actions: &mut Vec<Action>) {
    ui.add_space(10.0);
    // Home: back to the workspace overview.
    ui.vertical_centered(|ui| {
        let logo = ui.add(
            egui::Label::new(RichText::new("◆").size(20.0).color(c(theme.accent)))
                .sense(Sense::click()),
        );
        if logo.on_hover_text(tr("Home")).clicked() {
            actions.push(Action::GoHome);
        }
    });
    ui.add_space(12.0);
    for (value, icon, label) in [
        (RailTab::Files, Icon::Folder, tr("Files")),
        (RailTab::Layers, Icon::Layers, tr("Layers")),
        (RailTab::Templates, Icon::LayoutTemplate, tr("Templates")),
        (RailTab::Elements, Icon::Shapes, tr("Elements")),
    ] {
        if rail_item(ui, theme, icon, label, *tab == value).clicked() {
            *tab = value;
        }
        ui.add_space(6.0);
    }
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
        ui.add_space(10.0);
        if rail_item(ui, theme, Icon::Settings, tr("Settings"), false).clicked() {
            actions.push(Action::OpenSettings);
        }
    });
}

fn rail_item(
    ui: &mut Ui,
    theme: &Theme,
    icon: Icon,
    label: &str,
    selected: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 50.0), Sense::click());
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 17.0),
        Vec2::new(34.0, 30.0),
    );
    let bg = if selected {
        c(theme.accent_soft)
    } else if response.hovered() {
        c(theme.raised)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(icon_rect, theme.radius as f32, bg);
    let fg = if selected {
        c(theme.text)
    } else {
        c(theme.text_muted)
    };
    ui.painter().text(
        icon_rect.center(),
        egui::Align2::CENTER_CENTER,
        char::from(icon),
        egui::FontId::new(17.0, egui::FontFamily::Name(FAMILY.into())),
        if selected { c(theme.accent) } else { fg },
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 6.0),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(10.0),
        fg,
    );
    response
}
