//! The settings window: tabs at the top, and sections made of a title, the current value and
//! an action, separated by lines.

use eframe::egui::{self, Color32, RichText, Sense, Ui};
use libforma::paint::Color;

use crate::app::Action;
use crate::i18n::{Language, tr};
use crate::settings::Settings;
use crate::theme::egui::c;
use crate::theme::icons::{Icon, icon};
use crate::theme::tokens::ACCENT_PRESETS;
use crate::theme::{Theme, ThemeMode};
use crate::ui::home::display_path;
use crate::ui::widgets::{color_field, small_icon_button, strong};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    General,
    Project,
    About,
}

#[derive(Default)]
pub struct SettingsState {
    pub tab: Tab,
}

/// Shows the settings window; changes apply immediately. Returns `false` when it closes.
pub fn show(
    ctx: &egui::Context,
    theme: &Theme,
    settings: &mut Settings,
    state: &mut SettingsState,
    actions: &mut Vec<Action>,
) -> bool {
    let mut open = true;
    let modal = egui::Modal::new(egui::Id::unique("settings"))
        .frame(
            egui::Frame::NONE
                .fill(c(theme.panel))
                .stroke(egui::Stroke::new(1.0, c(theme.border)))
                .corner_radius(theme.radius_large)
                .inner_margin(egui::Margin::ZERO),
        )
        .show(ctx, |ui| {
            ui.set_width(660.0);
            // Tabs, and the close button on the right.
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (tab, label) in [
                            (Tab::General, tr("General")),
                            (Tab::Project, tr("Project")),
                            (Tab::About, tr("About")),
                        ] {
                            if tab_button(ui, theme, label, state.tab == tab).clicked() {
                                state.tab = tab;
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if small_icon_button(ui, theme, Icon::X, tr("Close")).clicked() {
                                open = false;
                            }
                        });
                    });
                });
            let rule = ui.available_rect_before_wrap();
            ui.painter().hline(
                rule.x_range(),
                rule.top(),
                egui::Stroke::new(1.0, c(theme.border)),
            );
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(28, 24))
                .show(ui, |ui| {
                    ui.set_min_height(560.0);
                    ui.horizontal_top(|ui| {
                        identity(ui, theme);
                        ui.add_space(24.0);
                        ui.vertical(|ui| {
                            egui::ScrollArea::vertical()
                                .max_height(600.0)
                                .auto_shrink([false, true])
                                .show(ui, |ui| match state.tab {
                                    Tab::General => general(ui, theme, settings),
                                    Tab::Project => project(ui, theme, settings, actions),
                                    Tab::About => about(ui, theme),
                                });
                        });
                    });
                });
        });
    open && !modal.should_close()
}

fn tab_button(ui: &mut Ui, theme: &Theme, label: &str, selected: bool) -> egui::Response {
    let text = RichText::new(label).color(c(if selected {
        theme.text
    } else {
        theme.text_muted
    }));
    let button = egui::Button::new(text)
        .fill(Color32::TRANSPARENT)
        .stroke(if selected {
            egui::Stroke::new(1.0, c(theme.accent))
        } else {
            egui::Stroke::NONE
        })
        .corner_radius(theme.radius);
    ui.add(button)
}

/// The left column: Forma's mark and version, where an account would show its picture.
fn identity(ui: &mut Ui, theme: &Theme) {
    ui.vertical(|ui| {
        ui.set_width(130.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(120.0, 120.0), Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 58.0, c(theme.accent_soft));
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "◆",
            egui::FontId::proportional(52.0),
            c(theme.accent),
        );
        ui.add_space(8.0);
        ui.vertical_centered(|ui| {
            ui.label(strong("Forma").size(15.0));
            ui.label(
                RichText::new(format!("{} {}", tr("Version"), env!("CARGO_PKG_VERSION")))
                    .color(c(theme.text_muted))
                    .size(11.5),
            );
        });
    });
}

/// A section: title, current value, and an action in the link color. Returns the action's
/// response.
fn item(
    ui: &mut Ui,
    theme: &Theme,
    title: &str,
    value: &str,
    action: Option<&str>,
) -> Option<egui::Response> {
    ui.label(strong(title).size(15.0));
    ui.add_space(6.0);
    if !value.is_empty() {
        ui.label(RichText::new(value).color(c(theme.text)));
    }
    let response = action.map(|a| link(ui, theme, a));
    end_item(ui, theme);
    response
}

fn link(ui: &mut Ui, theme: &Theme, text: &str) -> egui::Response {
    ui.add_space(4.0);
    ui.add(egui::Label::new(RichText::new(text).color(c(theme.accent))).sense(Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn end_item(ui: &mut Ui, theme: &Theme) {
    ui.add_space(16.0);
    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(1.0, c(theme.border)),
    );
    ui.add_space(16.0);
}

fn general(ui: &mut Ui, theme: &Theme, settings: &mut Settings) {
    if let Some(change) = item(
        ui,
        theme,
        tr("Language"),
        settings.language.name(),
        Some(tr("Change language")),
    ) {
        egui::Popup::menu(&change).show(|ui| {
            for language in Language::ALL {
                ui.selectable_value(&mut settings.language, language, language.name());
            }
        });
    }

    let theme_name = |mode| match mode {
        ThemeMode::Dark => tr("Forma Dark"),
        ThemeMode::Light => tr("Forma Light"),
    };
    if let Some(change) = item(
        ui,
        theme,
        tr("Theme"),
        theme_name(settings.theme),
        Some(tr("Change theme")),
    ) {
        egui::Popup::menu(&change).show(|ui| {
            for mode in [ThemeMode::Dark, ThemeMode::Light] {
                ui.selectable_value(&mut settings.theme, mode, theme_name(mode));
            }
        });
    }

    ui.label(strong(tr("Accent color")).size(15.0));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        for preset in ACCENT_PRESETS {
            let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), Sense::click());
            ui.painter().circle_filled(rect.center(), 11.0, c(preset));
            if settings.accent.with_alpha(255) == preset {
                ui.painter().circle_stroke(
                    rect.center(),
                    11.0,
                    egui::Stroke::new(2.0, c(theme.text)),
                );
            }
            if response.clicked() {
                settings.accent = preset;
            }
        }
    });
    ui.add_space(8.0);
    ui.allocate_ui(egui::vec2(220.0, 26.0), |ui| {
        let mut accent: Color = settings.accent;
        if color_field(ui, theme, "accent", &mut accent).1 {
            settings.accent = accent.with_alpha(255);
        }
    });
    end_item(ui, theme);
}

fn project(ui: &mut Ui, theme: &Theme, settings: &mut Settings, actions: &mut Vec<Action>) {
    let (value, action) = if settings.rulers {
        (tr("Shown on the canvas"), tr("Hide rulers"))
    } else {
        (
            tr("Hidden. Right-click the canvas or press Shift+R to show them."),
            tr("Show rulers"),
        )
    };
    if item(ui, theme, tr("Rulers"), value, Some(action)).is_some_and(|r| r.clicked()) {
        settings.rulers = !settings.rulers;
    }

    let (value, action) = if settings.autosave {
        (
            tr("Designs with a file are saved two seconds after each change."),
            tr("Turn off"),
        )
    } else {
        (tr("Off. Save with Ctrl+S."), tr("Turn on"))
    };
    if item(ui, theme, tr("Autosave"), value, Some(action)).is_some_and(|r| r.clicked()) {
        settings.autosave = !settings.autosave;
    }

    ui.label(strong(tr("Project folders")).size(15.0));
    ui.add_space(6.0);
    if settings.project_folders.is_empty() {
        ui.label(RichText::new(tr("No folders yet.")).color(c(theme.text_muted)));
    }
    for folder in settings.project_folders.clone() {
        // The link first, from the right, so a long path is cut instead of covering it.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if link(ui, theme, tr("Remove")).clicked() {
                    actions.push(Action::RemoveFolder(folder.clone()));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(icon(Icon::Folder).color(c(theme.text_muted)));
                    ui.add(egui::Label::new(display_path(&folder)).truncate())
                        .on_hover_text(folder.display().to_string());
                });
            });
        });
    }
    if link(ui, theme, tr("Add folder")).clicked() {
        actions.push(Action::AddFolder);
    }
    end_item(ui, theme);

    let fonts = crate::app::fonts_dir();
    if item(
        ui,
        theme,
        tr("Fonts"),
        &fonts
            .as_ref()
            .map_or_else(String::new, |d| d.display().to_string()),
        Some(tr("Open folder")),
    )
    .is_some_and(|r| r.clicked())
        && let Some(dir) = fonts
    {
        let _ = std::fs::create_dir_all(&dir);
        crate::app::open_in_file_manager(&dir);
    }

    let templates = crate::workspace::user_templates_dir();
    if item(
        ui,
        theme,
        tr("Templates"),
        &templates.as_deref().map_or_else(String::new, display_path),
        Some(tr("Open folder")),
    )
    .is_some_and(|r| r.clicked())
    {
        actions.push(Action::OpenTemplatesFolder);
    }
}

fn about(ui: &mut Ui, theme: &Theme) {
    item(
        ui,
        theme,
        "Forma",
        tr("Templates and vector design, free for everyone."),
        None,
    );
    item(ui, theme, tr("Version"), env!("CARGO_PKG_VERSION"), None);
    item(
        ui,
        theme,
        tr("License"),
        tr(
            "GNU General Public License, version 3 or later. Forma is free software and non-profit.",
        ),
        None,
    );
    item(
        ui,
        theme,
        tr("Credits"),
        tr("Interface font: Inter (SIL Open Font License). Icons: Lucide (ISC License)."),
        None,
    );
}
