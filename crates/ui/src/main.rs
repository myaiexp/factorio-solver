mod app;
mod chain_panel;
mod colors;
mod entity_draw;
mod icons;
mod lod;
mod persist;
mod viewport;

use app::FactorioApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            // With the "persistence" feature on, eframe restores the window's
            // last size/position from storage once one exists — this is only
            // the first-run default, not a fixed size. Desirable, not a
            // regression: it is the same persistence this app now uses for
            // the chain panel's inputs.
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
        Box::new(|cc| Ok(Box::new(FactorioApp::new(cc)))),
    )
}
