//! The list of pages in the left panel.

use eframe::egui::{self, Color32, Sense, Ui, Vec2};
use libforma::PageId;

use crate::editor::Editor;
use crate::i18n::{tr, trf};
use crate::theme::Theme;
use crate::theme::egui::c;
use crate::theme::icons::Icon;
use crate::ui::widgets::{section_header, small_icon_button};

#[derive(Default)]
pub struct PagesState {
    renaming: Option<(PageId, String)>,
}

pub fn show(ui: &mut Ui, editor: &mut Editor, theme: &Theme, state: &mut PagesState) {
    section_header(ui, tr("Pages"), |ui| {
        if small_icon_button(ui, theme, Icon::Plus, tr("Add page")).clicked() {
            editor.add_page();
        }
    });
    ui.add_space(2.0);

    let pages: Vec<(PageId, String)> = editor
        .doc
        .pages
        .iter()
        .map(|p| (p.id, p.name.clone()))
        .collect();
    let count = pages.len();
    for (i, (id, name)) in pages.into_iter().enumerate() {
        let selected = id == editor.page;
        if let Some((rename_id, buffer)) = &mut state.renaming
            && *rename_id == id
        {
            let response = ui.add(egui::TextEdit::singleline(buffer).desired_width(f32::INFINITY));
            response.request_focus();
            if response.lost_focus() {
                let name = buffer.trim().to_owned();
                if !name.is_empty() {
                    editor.update_page(id, None, |p| p.name = name);
                }
                state.renaming = None;
            }
            continue;
        }

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
        let bg = if selected {
            c(theme.raised)
        } else if response.hovered() {
            c(theme.raised).gamma_multiply(0.6)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, theme.radius_small as f32, bg);
        let label = if name.is_empty() {
            trf("Page {n}", &[("n", &(i + 1).to_string())])
        } else {
            name.clone()
        };
        ui.painter().text(
            rect.left_center() + egui::vec2(8.0, 0.0),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.5),
            c(if selected {
                theme.text
            } else {
                theme.text_muted
            }),
        );
        if response.clicked() {
            editor.set_page(id);
        }
        if response.double_clicked() {
            state.renaming = Some((id, name.clone()));
        }
        response.context_menu(|ui| {
            if ui.button(tr("Rename")).clicked() {
                state.renaming = Some((id, name.clone()));
                ui.close();
            }
            if ui.button(tr("Duplicate")).clicked() {
                editor.duplicate_page(id);
                ui.close();
            }
            if ui
                .add_enabled(i > 0, egui::Button::new(tr("Move up")))
                .clicked()
            {
                editor.move_page(id, true);
                ui.close();
            }
            if ui
                .add_enabled(i + 1 < count, egui::Button::new(tr("Move down")))
                .clicked()
            {
                editor.move_page(id, false);
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(count > 1, egui::Button::new(tr("Delete page")))
                .clicked()
            {
                editor.delete_page(id);
                ui.close();
            }
        });
    }
}
