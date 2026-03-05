mod app;
mod colors;
mod viewport;

use app::FactorioApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Factorio Layout Solver",
        options,
        Box::new(|_cc| Ok(Box::new(FactorioApp::new()))),
    )
}
