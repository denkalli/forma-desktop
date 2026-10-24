//! Forma: free graphic design with templates and vector editing.

mod app;
mod canvas;
mod editor;
mod i18n;
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
