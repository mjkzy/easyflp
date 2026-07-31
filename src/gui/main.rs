#![windows_subsystem = "windows"]

mod app;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let initial = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .filter(|p| p.exists());
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1020.0, 680.0])
            .with_min_inner_size([720.0, 480.0])
            .with_decorations(false),
        ..Default::default()
    };
    eframe::run_native(
        "easyflp",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, initial)))),
    )
}
