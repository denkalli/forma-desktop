//! The application: wires the workspace, the editor, the panels and the dialogs together.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use eframe::egui_wgpu::RenderState;
use libforma::geom::Vec2;
use libforma::render::{Gpu, OffscreenRenderer};

use crate::canvas::{self, CanvasGpu, CanvasState};
use crate::editor::{Axis, Editor, ExportFormat, Tool};
use crate::i18n::{Language, tr, trf};
use crate::settings::Settings;
use crate::theme::egui::c;
use crate::theme::{self, Theme};
use crate::ui::dialogs::{self, UnsavedChoice};
use crate::ui::home::{self, HomeState};
use crate::ui::inspector::{self, InspectorState};
use crate::ui::layers::{self, LayersState};
use crate::ui::pages::{self, PagesState};
use crate::ui::{rail, toolbar, widgets};
use crate::workspace::Workspace;

/// Delay between the last edit and an automatic save.
const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);
/// How long messages stay on screen.
const TOAST_TIME: Duration = Duration::from_secs(5);

/// Tabs of the icon rail.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RailTab {
    Files,
    #[default]
    Layers,
    Templates,
    Elements,
}

/// Requests from the panels that need the whole application.
#[derive(Clone, PartialEq, Debug)]
pub enum Action {
    GoHome,
    OpenSettings,
    OpenExport,
    Export,
    InsertImage,
    /// Asks for a project folder to add.
    AddFolder,
    AddFolderAt(PathBuf),
    /// Removes a project folder from the list; its files stay.
    RemoveFolder(PathBuf),
    RescanFolders,
    ToggleRulers,
    /// Asks for font files to add to Forma.
    AddFonts,
    OpenFile(PathBuf),
    /// The new-design dialog, with a folder chosen in advance.
    ShowNewDocument {
        folder: Option<PathBuf>,
    },
    /// A new design, saved in `folder` when given. With a `frame` (name and size) it starts
    /// with a frame; without one it is an empty canvas.
    CreateDocument {
        title: String,
        frame: Option<(String, libforma::geom::Size)>,
        folder: Option<PathBuf>,
        /// Start as a copy of this template file instead.
        template: Option<PathBuf>,
    },
    /// The new-design dialog for a design made from a template.
    NewFromTemplate(PathBuf),
    OpenTemplatesFolder,
    Save,
    SaveAs,
    Quit,
}

#[derive(Default)]
struct Dialogs {
    new_document: Option<dialogs::NewDocument>,
    export: Option<dialogs::Export>,
    settings: Option<crate::ui::settings::SettingsState>,
    /// Waiting for the user to decide about unsaved changes before running this action.
    unsaved: Option<Action>,
}

pub struct FormaApp {
    settings: Settings,
    theme: Theme,
    applied_theme: Option<Theme>,
    /// Project folders, in the order of `settings.project_folders`.
    folders: Vec<Workspace>,
    /// Template folders that exist.
    templates: Vec<Workspace>,
    editor: Option<Editor>,
    render_state: Option<RenderState>,
    gpu: Option<CanvasGpu>,
    offscreen: Option<OffscreenRenderer>,
    canvas: CanvasState,
    rail: RailTab,
    dialogs: Dialogs,
    home: HomeState,
    layers: LayersState,
    pages: PagesState,
    inspector: InspectorState,
    toasts: Vec<(String, Instant)>,
    title: String,
    /// The window may close (unsaved changes were dealt with).
    allow_close: bool,
    /// Data of the font files the user added to Forma, registered in every open design.
    user_fonts: Vec<Vec<u8>>,
    /// `FORMA_SCREENSHOT` target and frame counter.
    screenshot: Option<(PathBuf, u32)>,
    mcp: Option<crate::mcp::RunningServer>,
}

impl FormaApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        theme::egui::install_fonts(&cc.egui_ctx);
        let settings = Settings::load();
        let theme = Theme::new(settings.theme, settings.accent);
        let render_state = cc.wgpu_render_state.clone();
        let mut toasts = Vec::new();
        let gpu = render_state
            .as_ref()
            .and_then(|rs| match CanvasGpu::new(rs) {
                Ok(gpu) => Some(gpu),
                Err(err) => {
                    toasts.push((
                        trf(
                            "Could not start rendering: {error}",
                            &[("error", &err.to_string())],
                        ),
                        Instant::now(),
                    ));
                    None
                }
            });
        let folders = settings
            .project_folders
            .iter()
            .map(|f| Workspace::new(f))
            .collect();
        let mut app = Self {
            settings,
            theme,
            applied_theme: None,
            folders,
            templates: read_templates(),
            editor: None,
            render_state,
            gpu,
            offscreen: None,
            canvas: CanvasState::default(),
            rail: RailTab::Files,
            dialogs: Dialogs::default(),
            home: HomeState::default(),
            layers: LayersState::default(),
            pages: PagesState::default(),
            inspector: InspectorState::default(),
            toasts,
            title: String::new(),
            allow_close: false,
            screenshot: std::env::var_os("FORMA_SCREENSHOT").map(|p| (PathBuf::from(p), 0)),
            mcp: None,
            user_fonts: read_user_fonts(),
        };
        // Development aid: `FORMA_THEME=light|dark` overrides the theme without saving it.
        match std::env::var("FORMA_THEME").as_deref() {
            Ok("light") => app.settings.theme = crate::theme::ThemeMode::Light,
            Ok("dark") => app.settings.theme = crate::theme::ThemeMode::Dark,
            _ => {}
        }
        // Likewise, `FORMA_LANG=en|pt` overrides the language.
        match std::env::var("FORMA_LANG").as_deref() {
            Ok("en") => app.settings.language = Language::English,
            Ok("pt") => app.settings.language = Language::Portuguese,
            _ => {}
        }
        crate::i18n::set_language(app.settings.language);
        // `forma <folder>` adds the folder to the start screen; `forma <file>` opens it.
        match open {
            Some(path) if path.is_dir() => app.add_folder(&path),
            Some(path) => app.open_file(&path),
            None => {}
        }
        app
    }

    /// Development aid: with `FORMA_SCREENSHOT=<file.png>`, saves a screenshot of the window
    /// after a few frames and quits.
    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some((_, frames)) = &mut self.screenshot else {
            return;
        };
        *frames += 1;
        // `FORMA_SCREENSHOT_SELECT=<layer name>` selects a layer before the capture.
        if *frames == 3
            && let (Some(name), Some(editor)) = (
                std::env::var_os("FORMA_SCREENSHOT_SELECT"),
                &mut self.editor,
            )
        {
            let mut found = None;
            editor.current_page().walk(&mut |n| {
                if found.is_none() && n.name == name.to_string_lossy() {
                    found = Some(n.id);
                }
            });
            if let Some(id) = found {
                editor.select_only(id);
            }
        }
        // `FORMA_SCREENSHOT_OPEN=new|export|files|settings|settings-project|templates` opens a
        // dialog, a tab or a page.
        if *frames == 4 {
            match std::env::var("FORMA_SCREENSHOT_OPEN").as_deref() {
                Ok("new") => self.run(Action::ShowNewDocument { folder: None }),
                Ok("export") => self.run(Action::OpenExport),
                Ok("files") => self.rail = RailTab::Files,
                Ok("settings") => self.run(Action::OpenSettings),
                Ok("settings-project") => {
                    self.run(Action::OpenSettings);
                    if let Some(state) = &mut self.dialogs.settings {
                        state.tab = crate::ui::settings::Tab::Project;
                    }
                }
                Ok("templates") => self.home.show_templates(),
                Ok("fonts") => self.inspector.fonts.open_request = true,
                _ => {}
            }
        }
        let Some((path, frames)) = &mut self.screenshot else {
            return;
        };
        if *frames == 30 {
            let path = path.clone();
            let ctx2 = ctx.clone();
            ctx.request_screenshot(move |image| {
                let [w, h] = image.size;
                let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                if let Some(buffer) = image::RgbaImage::from_raw(w as u32, h as u32, pixels) {
                    let _ = buffer.save(&path);
                }
                ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
            });
        }
        ctx.request_repaint();
    }

    fn toast(&mut self, message: impl Into<String>) {
        self.toasts.push((message.into(), Instant::now()));
    }

    fn offscreen(&mut self) -> Option<&mut OffscreenRenderer> {
        if self.offscreen.is_none() {
            let rs = self.render_state.as_ref()?;
            let gpu = Gpu {
                device: rs.device.clone(),
                queue: rs.queue.clone(),
            };
            match OffscreenRenderer::with_gpu(gpu) {
                Ok(r) => self.offscreen = Some(r),
                Err(err) => {
                    self.toast(trf(
                        "Image export is unavailable: {error}",
                        &[("error", &err.to_string())],
                    ));
                    return None;
                }
            }
        }
        self.offscreen.as_mut()
    }

    // ---- Files ------------------------------------------------------------------------

    /// Adds a project folder (or shows it, if it is already in the list).
    fn add_folder(&mut self, folder: &Path) {
        if !folder.is_dir() {
            self.toast(trf(
                "“{folder}” is not a folder.",
                &[("folder", &folder.display().to_string())],
            ));
            return;
        }
        let folder = folder
            .canonicalize()
            .unwrap_or_else(|_| folder.to_path_buf());
        if self.settings.add_folder(&folder) {
            self.save_settings();
            self.folders.push(Workspace::new(&folder));
        }
        self.home.show_folder(&folder);
        self.rail = RailTab::Files;
    }

    fn remove_folder(&mut self, folder: &Path) {
        self.settings.remove_folder(folder);
        self.save_settings();
        self.folders.retain(|ws| ws.root != folder);
    }

    /// The project folder holding `path`.
    fn folder_of(&self, path: &Path) -> Option<&Workspace> {
        self.folders.iter().find(|ws| path.starts_with(&ws.root))
    }

    /// Where file dialogs start: the open document's folder, or the first project folder.
    fn default_dir(&self) -> Option<PathBuf> {
        let open = self
            .editor
            .as_ref()
            .and_then(|e| e.path.as_deref())
            .and_then(Path::parent);
        open.map(Path::to_path_buf).or_else(|| {
            self.folders
                .iter()
                .find(|ws| ws.available)
                .map(|ws| ws.root.clone())
        })
    }

    /// Makes the user's fonts available to a newly opened design.
    fn adopt(&mut self, mut editor: Editor) {
        for data in &self.user_fonts {
            editor.register_font(data.clone());
        }
        self.inspector.fonts.invalidate();
        self.editor = Some(editor);
    }

    /// Asks for font files, copies them to Forma's font folder and makes them available.
    fn add_fonts(&mut self) {
        let Some(files) = rfd::FileDialog::new()
            .set_title(tr("Add fonts"))
            .add_filter(tr("Fonts"), &["ttf", "otf", "ttc", "otc"])
            .pick_files()
        else {
            return;
        };
        let Some(dir) = fonts_dir() else {
            return;
        };
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.toast(trf(
                "Could not create the font folder: {error}",
                &[("error", &err.to_string())],
            ));
            return;
        }
        let mut added = std::collections::BTreeSet::new();
        for file in files {
            let Ok(data) = std::fs::read(&file) else {
                continue;
            };
            // Check the file with a scratch engine before keeping it.
            let families = libforma::text::TextEngine::new().register_font(data.clone());
            if families.is_empty() {
                self.toast(trf(
                    "“{file}” is not a font Forma can read.",
                    &[("file", &file.display().to_string())],
                ));
                continue;
            }
            if let Some(name) = file.file_name() {
                let _ = std::fs::write(dir.join(name), &data);
            }
            if let Some(editor) = &mut self.editor {
                editor.register_font(data.clone());
            }
            self.user_fonts.push(data);
            added.extend(families);
        }
        if !added.is_empty() {
            let names: Vec<String> = added.into_iter().collect();
            self.toast(trf("Fonts added: {names}", &[("names", &names.join(", "))]));
            self.inspector.fonts.invalidate();
        }
    }

    fn open_file(&mut self, path: &Path) {
        match Editor::open(path) {
            Ok(editor) => {
                self.adopt(editor);
                self.rail = RailTab::Layers;
            }
            Err(err) => self.toast(trf(
                "Could not open “{file}”: {error}",
                &[
                    ("file", &path.display().to_string()),
                    ("error", &err.to_string()),
                ],
            )),
        }
    }

    fn create_document(
        &mut self,
        title: &str,
        frame: Option<(String, libforma::geom::Size)>,
        folder: Option<PathBuf>,
        template: Option<PathBuf>,
    ) {
        let mut editor = match template {
            Some(path) => match Editor::from_template(&path, title) {
                Ok(editor) => editor,
                Err(err) => {
                    self.toast(trf(
                        "Could not open the template: {error}",
                        &[("error", &err.to_string())],
                    ));
                    return;
                }
            },
            None => {
                let frame = frame.as_ref().map(|(name, size)| (name.as_str(), *size));
                Editor::new_document(title, frame)
            }
        };
        // In a project folder the file exists from the start, so autosave keeps it up to date.
        if let Some(folder) = folder {
            let path = match self.folders.iter().find(|ws| ws.root == folder) {
                Some(ws) => ws.unique_path(title),
                None => Workspace::new(&folder).unique_path(title),
            };
            let renderer = self.offscreen_raw();
            if let Err(err) = editor.save_to(&path, renderer) {
                self.toast(trf(
                    "Could not create the file: {error}",
                    &[("error", &err.to_string())],
                ));
            }
        }
        self.adopt(editor);
        self.rail = RailTab::Layers;
        self.rescan();
    }

    /// The offscreen renderer without the error toast (for use while `self` is borrowed).
    fn offscreen_raw(&mut self) -> Option<&mut OffscreenRenderer> {
        self.offscreen();
        self.offscreen.as_mut()
    }

    /// Saves the open document. Returns `false` if it was not saved.
    fn save(&mut self, save_as: bool) -> bool {
        let Some(editor) = &self.editor else {
            return false;
        };
        let path = match (&editor.path, save_as) {
            (Some(path), false) => Some(path.clone()),
            _ => {
                let mut dialog = rfd::FileDialog::new()
                    .add_filter(tr("Forma design"), &[libforma::format::EXTENSION])
                    .set_file_name(format!("{}.forma", editor.display_name()));
                if let Some(dir) = self.default_dir() {
                    dialog = dialog.set_directory(dir);
                }
                dialog
                    .save_file()
                    .map(|p| p.with_extension(libforma::format::EXTENSION))
            }
        };
        let Some(path) = path else { return false };
        self.offscreen();
        let editor = self.editor.as_mut().expect("editor exists");
        match editor.save_to(&path, self.offscreen.as_mut()) {
            Ok(()) => {
                self.rescan();
                true
            }
            Err(err) => {
                self.toast(trf(
                    "Could not save: {error}",
                    &[("error", &err.to_string())],
                ));
                false
            }
        }
    }

    fn rescan(&mut self) {
        for ws in &mut self.folders {
            ws.rescan();
        }
        self.templates = read_templates();
    }

    fn autosave(&mut self) {
        if !self.settings.autosave {
            return;
        }
        let due = self.editor.as_ref().is_some_and(|e| {
            e.path.is_some()
                && e.is_modified()
                && !e.interaction.is_busy()
                && e.last_edit.is_some_and(|t| t.elapsed() >= AUTOSAVE_DELAY)
        });
        if due {
            self.save(false);
        }
    }

    /// Runs `action` once the open document's changes are safe. Documents with a file are
    /// saved silently; new documents ask.
    fn guarded(&mut self, action: Action) {
        let Some(editor) = &self.editor else {
            self.run(action);
            return;
        };
        if !editor.is_modified() {
            self.run(action);
        } else if editor.path.is_some() {
            if self.save(false) {
                self.run(action);
            }
        } else {
            self.dialogs.unsaved = Some(action);
        }
    }

    fn save_settings(&mut self) {
        if let Err(err) = self.settings.save() {
            self.toast(trf(
                "Could not save the settings: {error}",
                &[("error", &err.to_string())],
            ));
        }
    }

    // ---- Actions ----------------------------------------------------------------------

    fn handle(&mut self, action: Action) {
        match action {
            // Actions that replace the open document go through the unsaved-changes check.
            Action::GoHome | Action::OpenFile(_) | Action::CreateDocument { .. } | Action::Quit => {
                self.guarded(action)
            }
            other => self.run(other),
        }
    }

    fn run(&mut self, action: Action) {
        match action {
            Action::GoHome => {
                self.editor = None;
                self.rescan();
            }
            Action::OpenSettings => self.dialogs.settings = Some(Default::default()),
            Action::OpenExport => {
                if let Some(editor) = &self.editor {
                    let target = editor.default_export_target();
                    self.dialogs.export = Some(dialogs::Export::new(target));
                }
            }
            Action::Export => self.export(),
            Action::InsertImage => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(tr("Images"), &["png", "jpg", "jpeg"])
                    .pick_file()
                {
                    self.insert_image_file(&path);
                }
            }
            Action::AddFolder => {
                if let Some(folder) = rfd::FileDialog::new()
                    .set_title(tr("Add project folder"))
                    .pick_folder()
                {
                    self.add_folder(&folder);
                }
            }
            Action::AddFolderAt(folder) => self.add_folder(&folder),
            Action::RemoveFolder(folder) => self.remove_folder(&folder),
            Action::RescanFolders => self.rescan(),
            Action::AddFonts => self.add_fonts(),
            Action::ToggleRulers => {
                self.settings.rulers = !self.settings.rulers;
                self.save_settings();
            }
            Action::OpenFile(path) => self.open_file(&path),
            Action::ShowNewDocument { folder } => {
                // Without a choice, new designs go next to the open one, or in the first folder.
                let folder = folder.or_else(|| {
                    let open = self.editor.as_ref().and_then(|e| e.path.as_deref());
                    open.and_then(|p| self.folder_of(p))
                        .or_else(|| self.folders.iter().find(|ws| ws.available))
                        .map(|ws| ws.root.clone())
                });
                self.dialogs.new_document = Some(dialogs::NewDocument::new(folder));
            }
            Action::CreateDocument {
                title,
                frame,
                folder,
                template,
            } => self.create_document(&title, frame, folder, template),
            Action::NewFromTemplate(template) => {
                let title = self
                    .templates
                    .iter()
                    .flat_map(|ws| &ws.files)
                    .find(|f| f.path == template)
                    .map_or_else(String::new, |f| f.title.clone());
                let folder = self
                    .folders
                    .iter()
                    .find(|ws| ws.available)
                    .map(|ws| ws.root.clone());
                self.dialogs.new_document =
                    Some(dialogs::NewDocument::from_template(template, title, folder));
            }
            Action::OpenTemplatesFolder => {
                if let Some(dir) = crate::workspace::user_templates_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    open_in_file_manager(&dir);
                }
            }
            Action::Save => {
                self.save(false);
            }
            Action::SaveAs => {
                self.save(true);
            }
            Action::Quit => {
                self.allow_close = true;
            }
        }
    }

    fn insert_image_file(&mut self, path: &Path) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        match std::fs::read(path) {
            Ok(data) => {
                editor.set_tool(Tool::Select);
                editor.insert_image(data);
            }
            Err(err) => self.toast(trf(
                "Could not read the image: {error}",
                &[("error", &err.to_string())],
            )),
        }
    }

    fn export(&mut self) {
        let Some(options) = self.dialogs.export.take() else {
            return;
        };
        let format = options.format;
        let Some(name) = self.editor.as_ref().map(Editor::display_name) else {
            return;
        };
        if format == ExportFormat::Forma {
            let dialog = rfd::FileDialog::new()
                .add_filter(tr("Forma design"), &[libforma::format::EXTENSION])
                .set_file_name(format!("{name} - Selection.forma"));
            let dialog = match self.default_dir() {
                Some(dir) => dialog.set_directory(dir),
                None => dialog,
            };
            let Some(path) = dialog
                .save_file()
                .map(|p| p.with_extension(libforma::format::EXTENSION))
            else {
                return;
            };
            let Some(editor) = &mut self.editor else {
                return;
            };
            let document = match editor.selected_document() {
                Ok(document) => document,
                Err(err) => {
                    self.toast(trf(
                        "Export failed: {error}",
                        &[("error", &err.to_string())],
                    ));
                    return;
                }
            };
            match libforma::format::save(
                &document,
                &path,
                &libforma::format::WriteOptions::default(),
            ) {
                Ok(()) => {
                    self.rescan();
                    self.toast(tr("Selection saved as a Forma design"));
                }
                Err(err) => self.toast(trf(
                    "Could not save: {error}",
                    &[("error", &err.to_string())],
                )),
            }
            return;
        }
        let multiple = format != ExportFormat::Pdf
            && self
                .editor
                .as_ref()
                .is_some_and(|e| e.export_count(options.target) > 1);
        // Pick the destination before rendering.
        let dialog = rfd::FileDialog::new();
        let dialog = match self.default_dir() {
            Some(dir) => dialog.set_directory(dir),
            None => dialog,
        };
        let target = if multiple {
            dialog.pick_folder().map(|dir| (dir, None))
        } else {
            dialog
                .add_filter(format.extension().to_uppercase(), &[format.extension()])
                .set_file_name(format!("{name}.{}", format.extension()))
                .save_file()
                .map(|file| (PathBuf::new(), Some(file)))
        };
        let Some((dir, file)) = target else { return };

        self.offscreen();
        let editor = self.editor.as_mut().expect("editor exists");
        let result = editor.export(
            format,
            options.target,
            options.scale,
            self.offscreen.as_mut(),
        );
        let outputs = match result {
            Ok(outputs) => outputs,
            Err(err) => {
                self.toast(trf(
                    "Export failed: {error}",
                    &[("error", &err.to_string())],
                ));
                return;
            }
        };
        let mut written = 0;
        let mut used = std::collections::HashSet::new();
        for (i, (part, bytes)) in outputs.iter().enumerate() {
            let path = match &file {
                Some(file) => file.clone(),
                None => {
                    let part = if part.is_empty() {
                        format!("{}", i + 1)
                    } else {
                        part.clone()
                    };
                    // Frames with the same name get a number.
                    let mut stem = format!("{name} - {part}");
                    let mut n = 2;
                    while !used.insert(stem.clone()) {
                        stem = format!("{name} - {part} {n}");
                        n += 1;
                    }
                    dir.join(format!("{stem}.{}", format.extension()))
                }
            };
            match std::fs::write(&path, bytes) {
                Ok(()) => written += 1,
                Err(err) => self.toast(trf(
                    "Could not write {file}: {error}",
                    &[
                        ("file", &path.display().to_string()),
                        ("error", &err.to_string()),
                    ],
                )),
            }
        }
        if written > 0 {
            self.toast(if written == 1 {
                tr("Exported 1 file").to_owned()
            } else {
                trf("Exported {n} files", &[("n", &written.to_string())])
            });
        }
    }

    // ---- Input ------------------------------------------------------------------------

    fn shortcuts(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let typing = ctx.egui_wants_keyboard_input();
        let pressed = |mods: Modifiers, key: Key| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(mods, key)))
        };
        let shift_cmd = Modifiers::COMMAND | Modifiers::SHIFT;

        if pressed(Modifiers::COMMAND, Key::N) {
            actions.push(Action::ShowNewDocument { folder: None });
        }
        if pressed(Modifiers::COMMAND, Key::O) {
            actions.push(Action::AddFolder);
        }
        let Some(editor) = &mut self.editor else {
            return;
        };
        if pressed(shift_cmd, Key::S) {
            actions.push(Action::SaveAs);
        }
        if pressed(Modifiers::COMMAND, Key::S) {
            actions.push(Action::Save);
        }
        if pressed(Modifiers::COMMAND, Key::E) {
            actions.push(Action::OpenExport);
        }
        if typing {
            return;
        }
        if pressed(shift_cmd, Key::Z) || pressed(Modifiers::COMMAND, Key::Y) {
            editor.redo();
        }
        if pressed(Modifiers::COMMAND, Key::Z) {
            editor.undo();
        }
        if pressed(Modifiers::COMMAND, Key::D) {
            editor.duplicate_selection(Vec2::new(10.0, 10.0));
        }
        let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
        if pressed(alt_shift, Key::H) {
            editor.distribute_selection(Axis::Horizontal);
        }
        if pressed(alt_shift, Key::V) {
            editor.distribute_selection(Axis::Vertical);
        }
        if pressed(shift_cmd, Key::G) {
            editor.ungroup_selection();
        }
        if pressed(Modifiers::COMMAND, Key::G) {
            editor.group_selection();
        }
        if pressed(Modifiers::COMMAND, Key::A) {
            editor.select_all();
        }
        if pressed(Modifiers::COMMAND, Key::CloseBracket) {
            editor.reorder_selection(true);
        }
        if pressed(Modifiers::COMMAND, Key::OpenBracket) {
            editor.reorder_selection(false);
        }
        if pressed(Modifiers::COMMAND, Key::Equals) || pressed(Modifiers::COMMAND, Key::Plus) {
            editor.view.zoom_around(
                1.25,
                Vec2::new(
                    f64::from(self.canvas.rect.width() / 2.0),
                    f64::from(self.canvas.rect.height() / 2.0),
                ),
            );
        }
        if pressed(Modifiers::COMMAND, Key::Minus) {
            editor.view.zoom_around(
                0.8,
                Vec2::new(
                    f64::from(self.canvas.rect.width() / 2.0),
                    f64::from(self.canvas.rect.height() / 2.0),
                ),
            );
        }
        if pressed(Modifiers::SHIFT, Key::R) {
            actions.push(Action::ToggleRulers);
        }
        if pressed(Modifiers::SHIFT, Key::Num0) {
            editor.view.fit_requested = true;
        }
        if pressed(Modifiers::SHIFT, Key::Num1) {
            let factor = 1.0 / editor.view.zoom;
            editor.view.zoom_around(
                factor,
                Vec2::new(
                    f64::from(self.canvas.rect.width() / 2.0),
                    f64::from(self.canvas.rect.height() / 2.0),
                ),
            );
        }
        if pressed(Modifiers::NONE, Key::Delete) || pressed(Modifiers::NONE, Key::Backspace) {
            editor.delete_selection();
        }
        if pressed(Modifiers::NONE, Key::Escape) {
            editor.escape();
        }
        if pressed(Modifiers::NONE, Key::Enter) && editor.interaction.is_drawing_path() {
            editor.finish_path();
        }
        for (key, delta) in [
            (Key::ArrowLeft, Vec2::new(-1.0, 0.0)),
            (Key::ArrowRight, Vec2::new(1.0, 0.0)),
            (Key::ArrowUp, Vec2::new(0.0, -1.0)),
            (Key::ArrowDown, Vec2::new(0.0, 1.0)),
        ] {
            if pressed(Modifiers::SHIFT, key) {
                editor.nudge(delta * 10.0);
            }
            if pressed(Modifiers::NONE, key) {
                editor.nudge(delta);
            }
        }
        for (key, tool) in [
            (Key::V, Tool::Select),
            (Key::F, Tool::Frame),
            (Key::R, Tool::Rect),
            (Key::O, Tool::Ellipse),
            (Key::L, Tool::Line),
            (Key::P, Tool::Pen),
            (Key::T, Tool::Text),
        ] {
            if pressed(Modifiers::NONE, key) {
                editor.set_tool(tool);
            }
        }

        // Clipboard (egui turns Ctrl+C/X/V into these events).
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Copy | egui::Event::Cut => {
                    if let Some(text) = editor.copy_selection() {
                        ctx.copy_text(text);
                        if event == egui::Event::Cut {
                            editor.delete_selection();
                        }
                    }
                }
                egui::Event::Paste(text) => {
                    // "Paste here" from the canvas menu pastes at the clicked point.
                    let pasted = match self.canvas.take_paste_target() {
                        Some(at) => editor.paste_at(&text, at),
                        None => editor.paste(&text, Vec2::new(10.0, 10.0)),
                    };
                    if !pasted {
                        self.toasts.push((
                            tr("The clipboard has no Forma layers.").into(),
                            Instant::now(),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    fn dropped_files(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let files = ctx.input(|i| i.raw.dropped_files.clone());
        for file in files {
            let path = file.path().to_path_buf();
            if path.as_os_str().is_empty() {
                continue;
            }
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            match ext.as_str() {
                "forma" => actions.push(Action::OpenFile(path)),
                "png" | "jpg" | "jpeg" => self.insert_image_file(&path),
                _ if path.is_dir() => actions.push(Action::AddFolderAt(path)),
                _ => self.toast(tr("Drop PNG or JPEG images, or .forma files.")),
            }
        }
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.allow_close {
            return;
        }
        let needs_dialog = self.editor.as_ref().is_some_and(|e| e.is_modified());
        if needs_dialog {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.handle(Action::Quit);
            if self.allow_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match &self.editor {
            Some(e) => format!(
                "{}{} — Forma",
                e.display_name(),
                if e.is_modified() { " •" } else { "" }
            ),
            None => "Forma".to_owned(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    // ---- Layout -------------------------------------------------------------------------

    fn side_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let theme = self.theme.clone();
        // Document title and location.
        if let Some(editor) = &mut self.editor {
            // The title opens the file menu, like the document menu of other design tools.
            ui.horizontal(|ui| {
                let title = ui.add(
                    egui::Button::new((
                        widgets::strong(editor.display_name()).size(13.5),
                        theme::icons::icon(theme::icons::Icon::ChevronDown).size(11.0),
                    ))
                    .frame(false),
                );
                egui::Popup::menu(&title).show(|ui| {
                    for (label, action) in [
                        (tr("Save   Ctrl S"), Action::Save),
                        (tr("Save as…   Ctrl Shift S"), Action::SaveAs),
                        (tr("Export…   Ctrl E"), Action::OpenExport),
                        (
                            tr("New design…   Ctrl N"),
                            Action::ShowNewDocument { folder: None },
                        ),
                    ] {
                        if ui.button(label).clicked() {
                            actions.push(action);
                        }
                    }
                    ui.separator();
                    if ui.button(tr("Back to start")).clicked() {
                        actions.push(Action::GoHome);
                    }
                });
                if editor.is_modified() {
                    ui.label(egui::RichText::new("•").color(c(theme.text_muted)))
                        .on_hover_text(tr("Unsaved changes"));
                }
            });
            let folder = editor
                .path
                .as_deref()
                .and_then(|p| self.folders.iter().find(|ws| p.starts_with(&ws.root)));
            let location = match (&editor.path, folder) {
                (Some(path), Some(ws)) => format!(
                    "{} / {}",
                    ws.name(),
                    path.strip_prefix(&ws.root).unwrap_or(path).display()
                ),
                (Some(path), None) => path.display().to_string(),
                (None, _) => tr("Not saved").to_owned(),
            };
            ui.label(
                egui::RichText::new(location)
                    .size(11.0)
                    .color(c(theme.text_faint)),
            );
            widgets::separator(ui, &theme);
        }

        match self.rail {
            RailTab::Files => home::files_tab(
                ui,
                &theme,
                &self.folders,
                self.editor.as_ref().and_then(|e| e.path.as_deref()),
                actions,
            ),
            RailTab::Layers => match &mut self.editor {
                Some(editor) => {
                    let pages_height = (editor.doc.pages.len() as f32 * 28.0 + 40.0)
                        .min(ui.available_height() * 0.35);
                    ui.allocate_ui(egui::vec2(ui.available_width(), pages_height), |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("pages")
                            .show(ui, |ui| {
                                pages::show(ui, editor, &theme, &mut self.pages);
                            });
                    });
                    widgets::separator(ui, &theme);
                    layers::show(ui, editor, &theme, &mut self.layers);
                }
                None => {
                    ui.label(
                        egui::RichText::new(tr(
                            "Open or create a design to see its pages and layers.",
                        ))
                        .color(c(theme.text_muted)),
                    );
                }
            },
            RailTab::Templates => home::templates_tab(ui, &theme, &self.templates, actions),
            RailTab::Elements => {
                ui.label(widgets::strong(tr("Elements")));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(tr(
                        "Free shapes, icons and illustrations to drag onto the page arrive in the next phase of Forma.",
                    ))
                    .color(c(theme.text_muted)),
                );
            }
        }
    }

    /// Rail, side panel, inspector, canvas and toolbar, while a design is open.
    fn editor_layout(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        theme: &Theme,
        panel_frame: egui::Frame,
        actions: &mut Vec<Action>,
    ) {
        egui::Panel::left("rail")
            .exact_size(64.0)
            .resizable(false)
            .frame(panel_frame)
            .show(ui, |ui| rail::show(ui, theme, &mut self.rail, actions));
        egui::Panel::left("side")
            .default_size(250.0)
            .size_range(200.0..=420.0)
            .frame(panel_frame.inner_margin(egui::Margin::symmetric(12, 12)))
            .show(ui, |ui| self.side_panel(ui, actions));
        let Some(editor) = &mut self.editor else {
            return;
        };
        egui::Panel::right("inspector")
            .default_size(272.0)
            .size_range(240.0..=420.0)
            .frame(panel_frame.inner_margin(egui::Margin::symmetric(12, 10)))
            .show(ui, |ui| {
                inspector::show(ui, editor, theme, &mut self.inspector, actions)
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                canvas::show(
                    ui,
                    editor,
                    self.gpu.as_mut(),
                    self.render_state.as_ref(),
                    &mut self.canvas,
                    theme,
                    self.settings.rulers,
                    actions,
                );
            });
        toolbar::show(ctx, self.canvas.rect, editor, theme, actions);
        for message in editor.interaction.take_messages() {
            self.toasts.push((message, Instant::now()));
        }
    }

    fn toasts(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|(_, t)| t.elapsed() < TOAST_TIME);
        if self.toasts.is_empty() {
            return;
        }
        let theme = &self.theme;
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                for (message, _) in &self.toasts {
                    egui::Frame::NONE
                        .fill(c(theme.raised))
                        .stroke(egui::Stroke::new(1.0, c(theme.border)))
                        .corner_radius(theme.radius)
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_max_width(360.0);
                            ui.label(message);
                        });
                    ui.add_space(6.0);
                }
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
    fn sync_mcp(&mut self, ctx: &egui::Context) {
        if self.settings.mcp_enabled && self.mcp.is_none() {
            if self.settings.mcp_token.is_empty() {
                self.settings.mcp_token = uuid::Uuid::new_v4().simple().to_string();
                self.save_settings();
            }
            match crate::mcp::start(self.settings.mcp_token.clone(), ctx.clone()) {
                Ok(server) => {
                    self.mcp = Some(server);
                    self.toast(crate::i18n::tr(
                        "MCP server listening at http://127.0.0.1:43121/mcp",
                    ));
                }
                Err(error) => {
                    self.settings.mcp_enabled = false;
                    self.save_settings();
                    self.toast(trf(
                        "Could not start MCP server: {error}",
                        &[("error", &error)],
                    ));
                }
            }
        } else if !self.settings.mcp_enabled && self.mcp.is_some() {
            self.mcp = None;
            self.toast(crate::i18n::tr("MCP server stopped"));
        }
    }

    fn process_mcp_requests(&mut self) {
        let Some(server) = &self.mcp else {
            return;
        };
        let requests: Vec<_> = std::iter::from_fn(|| server.receiver.try_recv().ok()).collect();
        for request in requests {
            let result = crate::mcp::execute(self.editor.as_mut(), request.operation);
            let _ = request.reply.send(result);
        }
    }
}

impl eframe::App for FormaApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_mcp(ctx);
        self.process_mcp_requests();
        self.autosave();
        if self
            .editor
            .as_ref()
            .is_some_and(|e| e.is_modified() && e.path.is_some())
        {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        crate::i18n::set_language(self.settings.language);
        self.theme = Theme::new(self.settings.theme, self.settings.accent);
        if self.applied_theme.as_ref() != Some(&self.theme) {
            theme::egui::apply(&ctx, &self.theme);
            self.applied_theme = Some(self.theme.clone());
        }
        let theme = self.theme.clone();
        let mut actions = Vec::new();

        self.handle_close(&ctx);
        self.shortcuts(&ctx, &mut actions);
        self.dropped_files(&ctx, &mut actions);

        let panel_frame = egui::Frame::NONE.fill(c(theme.panel));
        if self.editor.is_none() {
            // The start screen has its own layout: no rail, layers or inspector.
            egui::Panel::left("home-side")
                .exact_size(232.0)
                .resizable(false)
                .frame(panel_frame.inner_margin(egui::Margin::symmetric(12, 14)))
                .show(ui, |ui| {
                    home::sidebar(ui, &theme, &self.folders, &mut self.home, &mut actions)
                });
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    home::show(
                        ui,
                        &theme,
                        &self.folders,
                        &self.templates,
                        &mut self.home,
                        &mut actions,
                    )
                });
        } else {
            self.editor_layout(ui, &ctx, &theme, panel_frame, &mut actions);
        }

        // Dialogs.
        if let Some(state) = &mut self.dialogs.new_document
            && !dialogs::new_document(
                &ctx,
                &theme,
                state,
                &self.settings.project_folders,
                &mut actions,
            )
        {
            self.dialogs.new_document = None;
        }
        if let (Some(state), Some(editor)) = (&mut self.dialogs.export, &self.editor)
            && !dialogs::export(
                &ctx,
                &theme,
                state,
                editor.doc.pages.len(),
                |t| editor.export_count(t),
                &mut actions,
            )
            && !actions.contains(&Action::Export)
        {
            self.dialogs.export = None;
        }
        if let Some(state) = &mut self.dialogs.settings {
            let before = self.settings.clone();
            let open =
                crate::ui::settings::show(&ctx, &theme, &mut self.settings, state, &mut actions);
            if !open {
                self.dialogs.settings = None;
            }
            if self.settings != before || !open {
                self.save_settings();
            }
        }
        if let Some(pending) = self.dialogs.unsaved.clone() {
            let name = self
                .editor
                .as_ref()
                .map(Editor::display_name)
                .unwrap_or_default();
            match dialogs::unsaved(&ctx, &theme, &name) {
                Some(UnsavedChoice::Save) => {
                    self.dialogs.unsaved = None;
                    if self.save(false) {
                        self.run(pending);
                    }
                }
                Some(UnsavedChoice::Discard) => {
                    self.dialogs.unsaved = None;
                    self.run(pending);
                }
                Some(UnsavedChoice::Cancel) => self.dialogs.unsaved = None,
                None => {}
            }
        }

        for action in actions {
            self.handle(action);
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.toasts(&ctx);
        self.update_title(&ctx);
        self.screenshot(&ctx);
    }
}

/// Folder of the fonts the user added to Forma.
pub fn fonts_dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("forma").join("fonts"))
}

/// Reads the font files in Forma's font folder.
fn read_user_fonts() -> Vec<Vec<u8>> {
    let Some(entries) = fonts_dir().and_then(|dir| std::fs::read_dir(dir).ok()) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| {
            e.path().extension().is_some_and(|x| {
                matches!(
                    x.to_string_lossy().to_lowercase().as_str(),
                    "ttf" | "otf" | "ttc" | "otc"
                )
            })
        })
        .filter_map(|e| std::fs::read(e.path()).ok())
        .collect()
}

/// The template folders that exist, read.
fn read_templates() -> Vec<Workspace> {
    crate::workspace::template_dirs()
        .iter()
        .map(|dir| Workspace::new(dir))
        .filter(|ws| ws.available)
        .collect()
}

/// Opens a folder in the system's file manager (through the portal in the Flatpak).
pub fn open_in_file_manager(dir: &Path) {
    #[cfg(windows)]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(windows, target_os = "macos")))]
    let program = "xdg-open";
    let _ = std::process::Command::new(program).arg(dir).spawn();
}
