//! Modal dialogs: new design, export, settings and unsaved changes.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText, Ui};

use crate::app::Action;
use crate::editor::{ExportFormat, ExportTarget};
use crate::i18n::{tr, trf};
use crate::theme::Theme;
use crate::theme::egui::c;
use crate::ui::inspector::PAGE_PRESETS;
use crate::ui::widgets::{accent_button, strong};

/// What the new design starts with.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Start {
    /// An empty canvas.
    Free,
    /// A frame of one of the `PAGE_PRESETS`.
    Preset(usize),
    /// A frame of the custom size.
    Custom,
}

pub struct NewDocument {
    title: String,
    start: Start,
    width: f64,
    height: f64,
    /// Project folder to save in; `None` keeps the design unsaved until the first save.
    folder: Option<PathBuf>,
    /// The template the design starts from, with its title.
    template: Option<(PathBuf, String)>,
}

impl NewDocument {
    pub fn new(folder: Option<PathBuf>) -> Self {
        Self {
            title: String::new(),
            start: Start::Preset(0),
            width: 1080.0,
            height: 1080.0,
            folder,
            template: None,
        }
    }

    /// A new design that starts as a copy of a template.
    pub fn from_template(template: PathBuf, title: String, folder: Option<PathBuf>) -> Self {
        Self {
            title: title.clone(),
            template: Some((template, title)),
            ..Self::new(folder)
        }
    }

    /// The frame the design starts with, as (name, size).
    fn frame(&self) -> Option<(String, libforma::geom::Size)> {
        match self.start {
            Start::Free => None,
            Start::Preset(i) => {
                let (name, w, h) = PAGE_PRESETS[i];
                Some((name.to_owned(), libforma::geom::Size::new(w, h)))
            }
            Start::Custom => Some((
                tr("Frame 1").to_owned(),
                libforma::geom::Size::new(self.width, self.height),
            )),
        }
    }
}

/// A selectable card with a title and a detail line, left-aligned.
fn start_card(
    ui: &mut Ui,
    theme: &Theme,
    selected: bool,
    title: &str,
    detail: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(229.0, 44.0), egui::Sense::click());
    let (fill, stroke) = if selected {
        (theme.accent_soft, theme.accent)
    } else if response.hovered() {
        (theme.raised, theme.border)
    } else {
        (theme.panel, theme.border)
    };
    let radius = theme.radius as f32;
    ui.painter().rect_filled(rect, radius, c(fill));
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, c(stroke)),
        egui::StrokeKind::Inside,
    );
    let left = rect.left() + 12.0;
    ui.painter().text(
        egui::pos2(left, rect.top() + 8.0),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(13.0),
        c(theme.text),
    );
    ui.painter().text(
        egui::pos2(left, rect.top() + 25.0),
        egui::Align2::LEFT_TOP,
        detail,
        egui::FontId::proportional(11.5),
        c(theme.text_muted),
    );
    response
}

fn folder_label(folder: Option<&Path>) -> String {
    match folder {
        Some(f) => f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| f.display().to_string()),
        None => tr("Don't save yet").to_owned(),
    }
}

/// Returns `false` when the dialog should close. `folders` are the project folders the
/// design can be saved in.
pub fn new_document(
    ctx: &egui::Context,
    theme: &Theme,
    state: &mut NewDocument,
    folders: &[PathBuf],
    actions: &mut Vec<Action>,
) -> bool {
    let mut open = true;
    let modal = egui::Modal::new(egui::Id::unique("new-document")).show(ctx, |ui| {
        ui.set_width(470.0);
        ui.label(strong(tr("New design")).size(16.0));
        ui.add_space(10.0);
        ui.add(
            egui::TextEdit::singleline(&mut state.title)
                .hint_text(tr("Design name"))
                .desired_width(f32::INFINITY),
        );
        ui.add_space(10.0);
        if let Some((_, name)) = &state.template {
            ui.label(RichText::new(tr("Template")).color(c(theme.text_muted)));
            ui.label(strong(name.as_str()));
        } else {
            ui.label(RichText::new(tr("Start with")).color(c(theme.text_muted)));
            let option = |ui: &mut Ui, selected: bool, title: &str, detail: String| {
                start_card(ui, theme, selected, title, &detail).clicked()
            };
            egui::Grid::new("presets")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    if option(
                        ui,
                        state.start == Start::Free,
                        tr("Free design"),
                        tr("Empty canvas, no frame").into(),
                    ) {
                        state.start = Start::Free;
                    }
                    for (i, (name, w, h)) in PAGE_PRESETS.iter().enumerate() {
                        if option(
                            ui,
                            state.start == Start::Preset(i),
                            name,
                            format!("{w:.0} × {h:.0}"),
                        ) {
                            state.start = Start::Preset(i);
                        }
                        if i % 2 == 0 {
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.radio_value(&mut state.start, Start::Custom, tr("Custom:"));
                let w = ui.add(
                    egui::DragValue::new(&mut state.width)
                        .range(1.0..=20000.0)
                        .suffix(" px"),
                );
                ui.label("×");
                let h = ui.add(
                    egui::DragValue::new(&mut state.height)
                        .range(1.0..=20000.0)
                        .suffix(" px"),
                );
                if w.changed() || h.changed() {
                    state.start = Start::Custom;
                }
            });
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Save in")).color(c(theme.text_muted)));
            egui::ComboBox::from_id_salt("new-document-folder")
                .width(300.0)
                .selected_text(folder_label(state.folder.as_deref()))
                .show_ui(ui, |ui| {
                    for folder in folders {
                        ui.selectable_value(
                            &mut state.folder,
                            Some(folder.clone()),
                            folder_label(Some(folder)),
                        )
                        .on_hover_text(folder.display().to_string());
                    }
                    ui.selectable_value(&mut state.folder, None, folder_label(None));
                });
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if accent_button(ui, theme, tr("Create")).clicked() {
                let title = if state.title.trim().is_empty() {
                    tr("Untitled").to_owned()
                } else {
                    state.title.trim().to_owned()
                };
                actions.push(Action::CreateDocument {
                    title,
                    frame: state.frame(),
                    folder: state.folder.clone(),
                    template: state.template.as_ref().map(|(path, _)| path.clone()),
                });
                open = false;
            }
            if ui.button(tr("Cancel")).clicked() {
                open = false;
            }
        });
    });
    open && !modal.should_close()
}

pub struct Export {
    pub format: ExportFormat,
    pub scale: f64,
    pub target: ExportTarget,
}

impl Export {
    pub fn new(target: ExportTarget) -> Self {
        Self {
            format: ExportFormat::Png,
            scale: 1.0,
            target,
        }
    }
}

/// `count` tells how many images a target produces; targets with none are disabled.
pub fn export(
    ctx: &egui::Context,
    theme: &Theme,
    state: &mut Export,
    pages: usize,
    count: impl Fn(ExportTarget) -> usize,
    actions: &mut Vec<Action>,
) -> bool {
    let mut open = true;
    let modal = egui::Modal::new(egui::Id::unique("export")).show(ctx, |ui| {
        ui.set_width(360.0);
        ui.label(strong(tr("Export")).size(16.0));
        ui.add_space(10.0);
        ui.label(RichText::new(tr("What to export")).color(c(theme.text_muted)));
        let mut targets = vec![
            (ExportTarget::SelectedFrames, tr("Selected frames")),
            (ExportTarget::Selection, tr("Selection")),
            (
                ExportTarget::Frames { all_pages: false },
                tr("All frames on the page"),
            ),
        ];
        if pages > 1 {
            targets.push((
                ExportTarget::Frames { all_pages: true },
                tr("Frames on all pages"),
            ));
        }
        targets.push((ExportTarget::Page, tr("Whole page")));
        for (target, label) in targets {
            let n = count(target);
            let label = match target {
                ExportTarget::SelectedFrames | ExportTarget::Frames { .. } if n > 0 => {
                    format!("{label}  ({n})")
                }
                _ => label.to_owned(),
            };
            let response =
                ui.add_enabled(n > 0, egui::RadioButton::new(state.target == target, label));
            if response.clicked() {
                state.target = target;
                if target != ExportTarget::Selection && state.format == ExportFormat::Forma {
                    state.format = ExportFormat::Png;
                }
            }
        }
        ui.add_space(10.0);
        ui.label(RichText::new(tr("Format")).color(c(theme.text_muted)));
        ui.horizontal(|ui| {
            for (format, label) in [
                (ExportFormat::Png, "PNG"),
                (ExportFormat::Svg, "SVG"),
                (ExportFormat::Pdf, "PDF"),
                (ExportFormat::Forma, ".forma"),
            ] {
                let enabled = format != ExportFormat::Forma || count(ExportTarget::Selection) > 0;
                let previous = state.format;
                ui.add_enabled_ui(enabled, |ui| {
                    ui.selectable_value(&mut state.format, format, label);
                });
                if state.format != previous {
                    if format == ExportFormat::Forma {
                        state.target = ExportTarget::Selection;
                    }
                }
            }
        });
        ui.add_space(8.0);
        if state.format == ExportFormat::Png {
            ui.horizontal(|ui| {
                ui.label(tr("Scale"));
                for scale in [0.5, 1.0, 2.0, 3.0, 4.0] {
                    ui.selectable_value(&mut state.scale, scale, format!("{scale}×"));
                }
            });
        }
        ui.add_space(6.0);
        let n = count(state.target);
        let count = n.to_string();
        let values = [("n", count.as_str())];
        let hint = match (state.format, n == 1) {
            (ExportFormat::Forma, _) => {
                tr("Save the selected layers as an editable Forma design.").to_owned()
            }
            (ExportFormat::Png, true) => trf("1 image, for social media and the web.", &values),
            (ExportFormat::Png, false) => trf("{n} images, for social media and the web.", &values),
            (ExportFormat::Svg, true) => trf(
                "1 file, a vector editable in other programs. Text becomes outlines.",
                &values,
            ),
            (ExportFormat::Svg, false) => trf(
                "{n} files, vectors editable in other programs. Text becomes outlines.",
                &values,
            ),
            (ExportFormat::Pdf, true) => {
                trf("A PDF with 1 page, for printing and sending.", &values)
            }
            (ExportFormat::Pdf, false) => {
                trf("A PDF with {n} pages, for printing and sending.", &values)
            }
        };
        ui.label(RichText::new(hint).color(c(theme.text_muted)).size(11.5));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let button = ui.add_enabled_ui(n > 0, |ui| {
                accent_button(
                    ui,
                    theme,
                    if state.format == ExportFormat::Forma {
                        tr("Save as…")
                    } else {
                        tr("Export…")
                    },
                )
            });
            if button.inner.clicked() {
                actions.push(Action::Export);
                open = false;
            }
            if ui.button(tr("Cancel")).clicked() {
                open = false;
            }
        });
    });
    open && !modal.should_close()
}

/// What the unsaved-changes dialog decided.
pub enum UnsavedChoice {
    Save,
    Discard,
    Cancel,
}

pub fn unsaved(ctx: &egui::Context, theme: &Theme, name: &str) -> Option<UnsavedChoice> {
    let mut choice = None;
    let modal = egui::Modal::new(egui::Id::unique("unsaved")).show(ctx, |ui| {
        ui.set_width(360.0);
        ui.label(strong(trf("Save the changes to “{name}”?", &[("name", name)])).size(15.0));
        ui.add_space(6.0);
        ui.label(
            RichText::new(tr("If you don't save, your changes will be lost."))
                .color(c(theme.text_muted)),
        );
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if accent_button(ui, theme, tr("Save")).clicked() {
                choice = Some(UnsavedChoice::Save);
            }
            if ui.button(tr("Don't save")).clicked() {
                choice = Some(UnsavedChoice::Discard);
            }
            if ui.button(tr("Cancel")).clicked() {
                choice = Some(UnsavedChoice::Cancel);
            }
        });
    });
    if modal.should_close() && choice.is_none() {
        choice = Some(UnsavedChoice::Cancel);
    }
    choice
}
