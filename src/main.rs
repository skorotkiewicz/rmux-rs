mod ai;
mod app;
mod notification;
mod terminal;
mod workspace;

use app::RmuxApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let _guard = rt.enter();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([600.0, 400.0])
            .with_title("rmux"),
        ..Default::default()
    };

    eframe::run_native(
        "rmux",
        options,
        Box::new(|cc| Ok(Box::new(RmuxApp::new(cc)))),
    )
}
