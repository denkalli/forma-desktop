//! The project folders: the start screen and the Files tab of the editor.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use eframe::egui::{self, Color32, ColorImage, RichText, Sense, TextureHandle, Ui, Vec2};

use crate::app::Action;
use crate::i18n::{tr, trf};
use crate::theme::Theme;
use crate::theme::egui::c;
use crate::theme::icons::{FAMILY, Icon, icon};
use crate::ui::widgets::{accent_button, section_header, small_icon_button, strong};
use crate::workspace::{FileEntry, Workspace};

/// State of the start screen.
#[derive(Default)]
pub struct HomeState {
    pub thumbnails: Thumbnails,
    /// Where the start screen should scroll on its next frame.
    scroll_to: Option<ScrollTarget>,
    page: HomePage,
}

/// The pages of the start screen.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum HomePage {
    #[default]
    Folders,
    Templates,
}

impl HomeState {
    /// Scrolls the start screen to a folder's section.
    pub fn show_folder(&mut self, root: &Path) {
        self.page = HomePage::Folders;
        self.scroll_to = Some(ScrollTarget::Folder(root.to_path_buf()));
    }

    /// Shows the templates page.
    pub fn show_templates(&mut self) {
        self.page = HomePage::Templates;
    }
}

#[derive(PartialEq)]
enum ScrollTarget {
    Top,
    Folder(PathBuf),
}

/// Thumbnails decoded into textures, by file.
#[derive(Default)]
pub struct Thumbnails {
    map: HashMap<PathBuf, (SystemTime, Option<TextureHandle>)>,
}

impl Thumbnails {
    fn get(&mut self, ctx: &egui::Context, entry: &FileEntry) -> Option<TextureHandle> {
        if let Some((modified, texture)) = self.map.get(&entry.path)
            && *modified == entry.modified
        {
            return texture.clone();
        }
        let texture = entry.thumbnail.as_ref().and_then(|png| {
            let image = image::load_from_memory(png).ok()?.into_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            let image = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            Some(ctx.load_texture(
                entry.path.display().to_string(),
                image,
                egui::TextureOptions::LINEAR,
            ))
        });
        self.map
            .insert(entry.path.clone(), (entry.modified, texture.clone()));
        texture
    }
}

/// The sidebar of the start screen: brand, start, the folders and the settings.
pub fn sidebar(
    ui: &mut Ui,
    theme: &Theme,
    folders: &[Workspace],
    state: &mut HomeState,
    actions: &mut Vec<Action>,
) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(RichText::new("◆").size(20.0).color(c(theme.accent)));
        ui.label(strong("Forma").size(17.0));
    });
    ui.add_space(18.0);
    if nav_item(
        ui,
        theme,
        Icon::House,
        tr("Home"),
        state.page == HomePage::Folders,
    )
    .clicked()
    {
        state.page = HomePage::Folders;
        state.scroll_to = Some(ScrollTarget::Top);
    }
    if nav_item(
        ui,
        theme,
        Icon::LayoutTemplate,
        tr("Templates"),
        state.page == HomePage::Templates,
    )
    .clicked()
    {
        state.page = HomePage::Templates;
    }
    ui.add_space(18.0);
    ui.label(
        RichText::new(tr("FOLDERS"))
            .size(10.5)
            .color(c(theme.text_faint)),
    );
    ui.add_space(4.0);
    for ws in folders {
        let icon = if ws.available {
            Icon::Folder
        } else {
            Icon::FolderX
        };
        if nav_item(ui, theme, icon, &ws.name(), false)
            .on_hover_text(display_path(&ws.root))
            .clicked()
        {
            state.show_folder(&ws.root);
        }
    }
    if nav_item(ui, theme, Icon::FolderPlus, tr("Add folder"), false).clicked() {
        actions.push(Action::AddFolder);
    }
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.add_space(4.0);
        if nav_item(ui, theme, Icon::Settings, tr("Settings"), false).clicked() {
            actions.push(Action::OpenSettings);
        }
    });
}

/// A full-width row with an icon and a label.
fn nav_item(ui: &mut Ui, theme: &Theme, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    let bg = if selected {
        c(theme.accent_soft)
    } else if response.hovered() {
        c(theme.raised)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, theme.radius as f32, bg);
    let fg = c(if selected {
        theme.text
    } else {
        theme.text_muted
    });
    ui.painter().text(
        egui::pos2(rect.left() + 18.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        char::from(icon),
        egui::FontId::new(15.0, egui::FontFamily::Name(FAMILY.into())),
        if selected { c(theme.accent) } else { fg },
    );
    let galley = ui.painter().layout(
        label.to_owned(),
        egui::FontId::proportional(13.0),
        fg,
        rect.width() - 42.0,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 34.0, rect.center().y - galley.size().y / 2.0),
        galley,
        fg,
    );
    response
}

/// The start screen, shown when no document is open: one section per project folder.
pub fn show(
    ui: &mut Ui,
    theme: &Theme,
    folders: &[Workspace],
    templates: &[Workspace],
    state: &mut HomeState,
    actions: &mut Vec<Action>,
) {
    if state.page == HomePage::Templates {
        templates_page(ui, theme, templates, state, actions);
        return;
    }
    egui::Frame::NONE
        .fill(c(theme.canvas))
        .inner_margin(egui::Margin::symmetric(32, 24))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let top = ui.horizontal(|ui| {
                        ui.label(strong(tr("Home")).size(22.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if accent_button(ui, theme, (icon(Icon::FilePlus), tr("New design")))
                                .clicked()
                            {
                                actions.push(Action::ShowNewDocument { folder: None });
                            }
                            if ui
                                .button((icon(Icon::FolderPlus), tr("Add folder")))
                                .clicked()
                            {
                                actions.push(Action::AddFolder);
                            }
                        });
                    });
                    if state.scroll_to == Some(ScrollTarget::Top) {
                        ui.scroll_to_rect(top.response.rect, Some(egui::Align::TOP));
                        state.scroll_to = None;
                    }
                    if folders.is_empty() {
                        welcome(ui, theme, actions);
                    }
                    for ws in folders {
                        folder_section(ui, theme, ws, state, actions);
                    }
                    ui.add_space(24.0);
                });
        });
}

fn welcome(ui: &mut Ui, theme: &Theme, actions: &mut Vec<Action>) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.15).max(40.0));
        ui.label(RichText::new("◆").size(48.0).color(c(theme.accent)));
        ui.label(strong(tr("Welcome to Forma")).size(26.0));
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                tr("Add the folders where you keep your designs. Each one shows up here with thumbnails of its files."),
            )
            .color(c(theme.text_muted)),
        );
        ui.add_space(20.0);
        if accent_button(
            ui,
            theme,
            (icon(Icon::FolderPlus), tr("Add project folder")),
        )
        .clicked()
        {
            actions.push(Action::AddFolder);
        }
    });
}

fn folder_section(
    ui: &mut Ui,
    theme: &Theme,
    ws: &Workspace,
    state: &mut HomeState,
    actions: &mut Vec<Action>,
) {
    ui.add_space(28.0);
    // Buttons first, from the right, so a long path is cut instead of covering them.
    let header = ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if small_icon_button(
                ui,
                theme,
                Icon::X,
                tr("Remove from the list (the files stay in the folder)"),
            )
            .clicked()
            {
                actions.push(Action::RemoveFolder(ws.root.clone()));
            }
            if small_icon_button(ui, theme, Icon::RefreshCw, tr("Refresh")).clicked() {
                actions.push(Action::RescanFolders);
            }
            if ws.available
                && ui
                    .button((icon(Icon::FilePlus), tr("New design here")))
                    .clicked()
            {
                actions.push(Action::ShowNewDocument {
                    folder: Some(ws.root.clone()),
                });
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(icon(Icon::Folder).size(17.0).color(c(theme.text_muted)));
                ui.label(strong(ws.name()).size(17.0));
                ui.add(
                    egui::Label::new(
                        RichText::new(display_path(&ws.root))
                            .size(11.5)
                            .color(c(theme.text_faint)),
                    )
                    .truncate(),
                );
            });
        });
    });
    if state.scroll_to.as_ref() == Some(&ScrollTarget::Folder(ws.root.clone())) {
        ui.scroll_to_rect(header.response.rect, Some(egui::Align::TOP));
        state.scroll_to = None;
    }
    ui.add_space(12.0);
    if !ws.available {
        ui.label(
            RichText::new(tr("Folder not found. It may be on a disconnected drive."))
                .color(c(theme.danger)),
        );
        return;
    }
    if ws.files.is_empty() {
        ui.label(RichText::new(tr("This folder has no designs yet.")).color(c(theme.text_muted)));
        return;
    }
    if let Some(entry) = card_grid(
        ui,
        theme,
        ("files", &ws.root),
        ws.files.iter(),
        &mut state.thumbnails,
    ) {
        actions.push(Action::OpenFile(entry.path.clone()));
    }
}

/// A thumbnail with the file's title and details. Returns `true` when clicked.
fn file_card(
    ui: &mut Ui,
    theme: &Theme,
    entry: &FileEntry,
    size: Vec2,
    thumbnails: &mut Thumbnails,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let preview = egui::Rect::from_min_size(rect.min, Vec2::new(size.x, size.y - 44.0));
    let hovered = response.hovered();
    ui.painter()
        .rect_filled(preview, theme.radius as f32, c(theme.panel));
    if hovered {
        ui.painter().rect_stroke(
            preview,
            theme.radius as f32,
            egui::Stroke::new(2.0, c(theme.accent)),
            egui::StrokeKind::Outside,
        );
    }
    if let Some(texture) = thumbnails.get(ui.ctx(), entry) {
        let tex_size = texture.size_vec2();
        let scale =
            ((preview.width() - 16.0) / tex_size.x).min((preview.height() - 16.0) / tex_size.y);
        let image_rect = egui::Rect::from_center_size(preview.center(), tex_size * scale);
        ui.painter().image(
            texture.id(),
            image_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    } else {
        ui.painter().text(
            preview.center(),
            egui::Align2::CENTER_CENTER,
            char::from(Icon::File),
            egui::FontId::new(
                28.0,
                egui::FontFamily::Name(crate::theme::icons::FAMILY.into()),
            ),
            c(theme.text_faint),
        );
    }
    ui.painter().text(
        egui::pos2(rect.left() + 2.0, preview.bottom() + 8.0),
        egui::Align2::LEFT_TOP,
        &entry.title,
        egui::FontId::proportional(12.5),
        c(theme.text),
    );
    let detail = match &entry.error {
        Some(err) => err.clone(),
        None => {
            let pages = if entry.pages == 1 {
                tr("1 page").to_owned()
            } else {
                trf("{n} pages", &[("n", &entry.pages.to_string())])
            };
            format!("{pages}  ·  {}", entry.relative.display())
        }
    };
    ui.painter().text(
        egui::pos2(rect.left() + 2.0, preview.bottom() + 26.0),
        egui::Align2::LEFT_TOP,
        detail,
        egui::FontId::proportional(11.0),
        c(if entry.error.is_some() {
            theme.danger
        } else {
            theme.text_faint
        }),
    );
    response.clicked() && entry.error.is_none()
}

/// Grid of file cards. Returns the entry that was clicked.
fn card_grid<'a>(
    ui: &mut Ui,
    theme: &Theme,
    id: impl std::hash::Hash + std::fmt::Debug,
    entries: impl Iterator<Item = &'a FileEntry>,
    thumbnails: &mut Thumbnails,
) -> Option<&'a FileEntry> {
    let card = Vec2::new(200.0, 220.0);
    let columns = ((ui.available_width() + 16.0) / (card.x + 16.0))
        .floor()
        .max(1.0) as usize;
    let mut clicked = None;
    egui::Grid::new(id).spacing([16.0, 16.0]).show(ui, |ui| {
        for (i, entry) in entries.enumerate() {
            if file_card(ui, theme, entry, card, thumbnails) {
                clicked = Some(entry);
            }
            if (i + 1) % columns == 0 {
                ui.end_row();
            }
        }
    });
    clicked
}

/// The templates page: every `.forma` file in the template folders, to start a design from.
fn templates_page(
    ui: &mut Ui,
    theme: &Theme,
    templates: &[Workspace],
    state: &mut HomeState,
    actions: &mut Vec<Action>,
) {
    egui::Frame::NONE
        .fill(c(theme.canvas))
        .inner_margin(egui::Margin::symmetric(32, 24))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            ui.horizontal(|ui| {
                ui.label(strong(tr("Templates")).size(22.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button((icon(Icon::FolderOpen), tr("Open templates folder")))
                        .clicked()
                    {
                        actions.push(Action::OpenTemplatesFolder);
                    }
                    if small_icon_button(ui, theme, Icon::RefreshCw, tr("Refresh")).clicked() {
                        actions.push(Action::RescanFolders);
                    }
                });
            });
            ui.add_space(20.0);
            let entries = templates.iter().flat_map(|ws| ws.files.iter());
            if entries.clone().next().is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() * 0.15).max(40.0));
                    ui.label(icon(Icon::LayoutTemplate).size(44.0).color(c(theme.accent)));
                    ui.add_space(8.0);
                    ui.label(strong(tr("No templates yet")).size(20.0));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            tr("Forma's own templates arrive in a future version. Any .forma file you put in the templates folder shows up here, ready to start a design from."),
                        )
                        .color(c(theme.text_muted)),
                    );
                    ui.add_space(16.0);
                    if accent_button(
                        ui,
                        theme,
                        (icon(Icon::FolderOpen), tr("Open templates folder")),
                    )
                    .clicked()
                    {
                        actions.push(Action::OpenTemplatesFolder);
                    }
                });
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(entry) =
                        card_grid(ui, theme, "templates", entries, &mut state.thumbnails)
                    {
                        actions.push(Action::NewFromTemplate(entry.path.clone()));
                    }
                });
        });
}

/// The Templates tab of the editor's side panel: a compact list of the templates.
pub fn templates_tab(
    ui: &mut Ui,
    theme: &Theme,
    templates: &[Workspace],
    actions: &mut Vec<Action>,
) {
    section_header(ui, tr("Templates"), |ui| {
        if small_icon_button(ui, theme, Icon::FolderOpen, tr("Open templates folder")).clicked() {
            actions.push(Action::OpenTemplatesFolder);
        }
    });
    let mut any = false;
    for entry in templates.iter().flat_map(|ws| ws.files.iter()) {
        any = true;
        let response = ui.add(
            egui::Button::selectable(false, (icon(Icon::LayoutTemplate), entry.title.as_str()))
                .min_size(egui::vec2(ui.available_width(), 24.0)),
        );
        if response
            .on_hover_text(tr("New design from this template"))
            .clicked()
        {
            actions.push(Action::NewFromTemplate(entry.path.clone()));
        }
    }
    if !any {
        ui.label(
            RichText::new(tr("Forma's own templates arrive in a future version. The .forma files in the templates folder show up here."))
                .color(c(theme.text_muted)),
        );
    }
}

/// A path for display, with the home folder shortened to `~`.
pub fn display_path(path: &Path) -> String {
    match dirs::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => path.display().to_string(),
    }
}

/// The Files tab of the editor's side panel: the project folders, each with its files.
pub fn files_tab(
    ui: &mut Ui,
    theme: &Theme,
    folders: &[Workspace],
    current: Option<&Path>,
    actions: &mut Vec<Action>,
) {
    section_header(ui, tr("Folders"), |ui| {
        if small_icon_button(ui, theme, Icon::FolderPlus, tr("Add folder")).clicked() {
            actions.push(Action::AddFolder);
        }
    });
    ui.add_space(4.0);
    if folders.is_empty() {
        ui.label(RichText::new(tr("No project folders yet.")).color(c(theme.text_muted)));
        ui.add_space(6.0);
        if accent_button(ui, theme, tr("Add folder")).clicked() {
            actions.push(Action::AddFolder);
        }
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for ws in folders {
                egui::CollapsingHeader::new(strong(ws.name()))
                    .id_salt(("folder", &ws.root))
                    .default_open(true)
                    .show(ui, |ui| folder_files(ui, theme, ws, current, actions));
            }
        });
}

fn folder_files(
    ui: &mut Ui,
    theme: &Theme,
    ws: &Workspace,
    current: Option<&Path>,
    actions: &mut Vec<Action>,
) {
    if !ws.available {
        ui.label(RichText::new(tr("Folder not found")).color(c(theme.danger)));
        return;
    }
    ui.horizontal(|ui| {
        if small_icon_button(ui, theme, Icon::FilePlus, tr("New design in this folder")).clicked() {
            actions.push(Action::ShowNewDocument {
                folder: Some(ws.root.clone()),
            });
        }
        if small_icon_button(ui, theme, Icon::RefreshCw, tr("Refresh")).clicked() {
            actions.push(Action::RescanFolders);
        }
    });
    if ws.files.is_empty() {
        ui.label(RichText::new(tr("No designs")).color(c(theme.text_faint)));
    }
    for entry in &ws.files {
        let selected = current == Some(entry.path.as_path());
        let label = (
            icon(Icon::File),
            entry.relative.with_extension("").display().to_string(),
        );
        let response = ui.add(
            egui::Button::selectable(selected, label)
                .min_size(egui::vec2(ui.available_width(), 24.0)),
        );
        if response.clicked() && !selected && entry.error.is_none() {
            actions.push(Action::OpenFile(entry.path.clone()));
        }
    }
}
