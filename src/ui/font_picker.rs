//! The font list of the text section: search, a filter, a preview in the font itself and a
//! button to add font files.

use std::collections::HashSet;

use eframe::egui::{self, RichText, Sense, Ui, Vec2};
use eframe::epaint::text::{FontInsert, FontPriority, InsertFontFamily};

use crate::app::Action;
use crate::editor::Editor;
use crate::i18n::tr;
use crate::theme::Theme;
use crate::theme::egui::c;
use crate::theme::icons::{FAMILY, Icon, icon};
use crate::ui::widgets::{FIELD_HEIGHT, select, small_icon_button, strong};

const ROW_HEIGHT: f32 = 26.0;
/// Text shown in the preview line.
const SAMPLE: &str = "Forma Aa Gg 123";

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Filter {
    #[default]
    All,
    System,
    Added,
}

#[derive(Default)]
pub struct FontPickerState {
    search: String,
    filter: Filter,
    /// Families of the editor's text engine, read when the list is first opened.
    families: Vec<String>,
    /// Fonts given to egui for previews, ready to draw from the next frame on.
    requested: HashSet<String>,
    ready: HashSet<String>,
    /// Families whose data could not be read.
    failed: HashSet<String>,
    hovered: Option<String>,
    focus_search: bool,
    /// Open the list on the next frame without a click (used by the screenshot aid).
    pub open_request: bool,
}

impl FontPickerState {
    /// Reads the font list again (after fonts were added).
    pub fn invalidate(&mut self) {
        self.families.clear();
    }
}

/// Shows the font button of the text section. Returns the family the user picked.
pub fn show(
    ui: &mut Ui,
    theme: &Theme,
    editor: &mut Editor,
    state: &mut FontPickerState,
    current: &str,
    actions: &mut Vec<Action>,
) -> Option<String> {
    // Fonts requested in an earlier frame can now be used.
    state.ready.extend(state.requested.drain());

    let (rect, button) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), FIELD_HEIGHT),
        Sense::click(),
    );
    let fill = if button.hovered() {
        theme.border
    } else {
        theme.raised
    };
    ui.painter()
        .rect_filled(rect, theme.radius_small as f32, c(fill));
    ui.painter().text(
        rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        current,
        egui::FontId::proportional(12.5),
        c(theme.text),
    );
    ui.painter().text(
        rect.right_center() - egui::vec2(10.0, 0.0),
        egui::Align2::CENTER_CENTER,
        char::from(Icon::ChevronDown),
        egui::FontId::new(12.0, egui::FontFamily::Name(FAMILY.into())),
        c(theme.text_muted),
    );
    let requested = std::mem::take(&mut state.open_request);
    if button.clicked() || requested {
        state.focus_search = true;
        state.hovered = None;
        if state.families.is_empty() {
            state.families = editor.font_families();
        }
    }

    let mut picked = None;
    let mut popup = egui::Popup::menu(&button);
    if requested {
        popup = popup.open_memory(egui::SetOpenCommand::Bool(true));
    }
    popup
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(290.0)
        .show(|ui| {
            ui.set_width(290.0);
            ui.horizontal(|ui| {
                ui.label(strong(tr("Fonts")).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if small_icon_button(ui, theme, Icon::X, tr("Close")).clicked() {
                        ui.close();
                    }
                    if small_icon_button(ui, theme, Icon::Plus, tr("Add fonts…")).clicked() {
                        actions.push(Action::AddFonts);
                        ui.close();
                    }
                });
            });
            ui.add_space(4.0);
            let search = ui.add(
                egui::TextEdit::singleline(&mut state.search)
                    .hint_text(tr("Search fonts"))
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut state.focus_search) {
                search.request_focus();
            }
            ui.add_space(4.0);
            select(
                ui,
                "font-filter",
                &mut state.filter,
                &[
                    (Filter::All, tr("All fonts")),
                    (Filter::System, tr("System fonts")),
                    (Filter::Added, tr("Added by you")),
                ],
            );
            ui.add_space(6.0);

            // A sample in the font under the pointer, or the current one.
            let preview = state.hovered.clone().unwrap_or_else(|| current.to_owned());
            let family = preview_family(ui.ctx(), editor, state, &preview);
            let sample = match family {
                Some(family) => RichText::new(SAMPLE).font(egui::FontId::new(22.0, family)),
                None => RichText::new(SAMPLE).size(22.0),
            };
            ui.add(egui::Label::new(sample.color(c(theme.text))).truncate());
            ui.add_space(4.0);

            let query = state.search.trim().to_lowercase();
            let user = editor.user_fonts();
            let shown: Vec<&String> = state
                .families
                .iter()
                .filter(|f| query.is_empty() || f.to_lowercase().contains(&query))
                .filter(|f| match state.filter {
                    Filter::All => true,
                    Filter::System => !user.contains(*f),
                    Filter::Added => user.contains(*f),
                })
                .collect();
            if shown.is_empty() {
                ui.label(RichText::new(tr("No fonts found")).color(c(theme.text_muted)));
                return;
            }
            let mut hovered = None;
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .auto_shrink([false, true])
                .show_rows(ui, ROW_HEIGHT, shown.len(), |ui, rows| {
                    for family in &shown[rows] {
                        let selected = family.as_str() == current;
                        let mark = if selected {
                            icon(Icon::Check)
                        } else {
                            RichText::new("   ")
                        };
                        let row = ui.add(
                            egui::Button::selectable(selected, (mark, family.as_str()))
                                .min_size(Vec2::new(ui.available_width(), ROW_HEIGHT)),
                        );
                        if row.hovered() {
                            hovered = Some((*family).clone());
                        }
                        if row.clicked() {
                            picked = Some((*family).clone());
                            ui.close();
                        }
                    }
                });
            if hovered.is_some() {
                state.hovered = hovered;
            }
        });
    picked
}

/// The egui font family that previews `family`, once its font is loaded. The first call
/// loads it and returns `None`: egui can only draw with it from the next frame.
fn preview_family(
    ctx: &egui::Context,
    editor: &mut Editor,
    state: &mut FontPickerState,
    family: &str,
) -> Option<egui::FontFamily> {
    let name = format!("preview:{family}");
    if state.ready.contains(&name) {
        return Some(egui::FontFamily::Name(name.into()));
    }
    if state.failed.contains(family) || state.requested.contains(&name) {
        return None;
    }
    match editor.font_data(family) {
        Some((data, index)) => {
            let mut font = egui::FontData::from_owned(data);
            font.index = index;
            ctx.add_font(FontInsert::new(
                &name,
                font,
                vec![InsertFontFamily {
                    family: egui::FontFamily::Name(name.clone().into()),
                    priority: FontPriority::Highest,
                }],
            ));
            state.requested.insert(name);
            ctx.request_repaint();
        }
        None => {
            state.failed.insert(family.to_owned());
        }
    }
    None
}
