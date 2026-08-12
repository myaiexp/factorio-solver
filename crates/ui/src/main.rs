mod app;
mod chain_panel;
mod colors;
mod entity_draw;
mod icons;
mod lod;
mod viewport;

use app::FactorioApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            // Must stay equal to the desktop entry's basename and its
            // StartupWMClass (scripts/install-desktop-entry.sh) — that pairing
            // is how a compositor gives the window our launcher icon.
            .with_app_id("factorio-solver"),
        ..Default::default()
    };
    eframe::run_native(
        "Factorio Layout Solver",
        options,
        Box::new(|_cc| Ok(Box::new(FactorioApp::new()))),
    )
}
