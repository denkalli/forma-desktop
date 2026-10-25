//! Forma: free graphic design with templates and vector editing.

// No console window on Windows; debug builds keep it for logs and panics.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod canvas;
mod editor;
mod i18n;
mod mcp;
mod settings;
mod theme;
mod ui;
mod workspace;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Forma")
            // Matches the .desktop file, so desktops show the right name and icon.
            .with_app_id("io.github.denkalli.Forma")
            // Title bar and taskbar icon on Windows (Wayland takes it from the .desktop file).
            .with_icon(icon())
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    // `forma <file.forma>` opens a design; `forma <folder>` adds a project folder.
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    eframe::run_native(
        "Forma",
        options,
        Box::new(move |cc| Ok(Box::new(app::FormaApp::new(cc, path)))),
    )
}

/// The window icon, rendered from `packaging/flatpak/io.github.denkalli.Forma.svg`.
fn icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/icon-256.png"))
        .expect("the bundled icon is a valid PNG")
        .into_rgba8();
    egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}
