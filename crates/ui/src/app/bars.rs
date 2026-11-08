// The bars around the viewport: blueprint input on top, load status and notices below.
use egui::Color32;

use super::{AppState, FactorioApp, LoadSource};
use crate::build_info;
use crate::clipboard::WatchStatus;
use crate::icons::IconStatus;

impl FactorioApp {
    /// The top bar: blueprint input and Load, the viewport toggles, the
    /// book entry picker, and what is loaded.
    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Blueprint:");

            let input_response = ui.add(
                egui::TextEdit::singleline(&mut self.blueprint_input)
                    .desired_width(600.0)
                    .hint_text("Paste blueprint string..."),
            );

            let enter_pressed =
                input_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if ui.button("Load").clicked() || enter_pressed {
                self.load_blueprint();
            }

            ui.separator();
            ui.checkbox(&mut self.show_grid_lines, "Grid lines");

            let mut watching = self.clipboard.watching();
            if ui
                .checkbox(&mut watching, "Watch clipboard")
                .on_hover_text(
                    "Load a blueprint automatically when one is copied — \
                     export in-game and it appears here.",
                )
                .changed()
            {
                self.clipboard.set_watching(watching);
            }

            if let Some(browser) = &self.book {
                ui.separator();
                ui.label("Book entry:");
                if let Some(index) = browser.picker(ui) {
                    self.show_book_entry(index);
                }
            }

            // Show entity count and label when loaded.
            if let AppState::Loaded {
                ref grid,
                ref label,
                ..
            } = self.state
            {
                ui.separator();
                if let Some(lbl) = label {
                    ui.label(format!("{} — {} entities", lbl, grid.entity_count()));
                } else {
                    ui.label(format!("{} entities", grid.entity_count()));
                }
            }
        });
    }

    /// The bottom bar: how the last load went, then the notices that keep a
    /// silent fallback from reading as a bug, then the build stamp.
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        match &self.state {
            AppState::Empty => {
                ui.label("Paste a blueprint string and click Load");
            }
            AppState::Loaded { skipped, source, .. } => {
                if skipped.is_empty() {
                    let origin = match source {
                        LoadSource::Pasted => "Blueprint loaded",
                        LoadSource::Clipboard => "Loaded from clipboard",
                        LoadSource::Generated => "Generated block",
                    };
                    ui.label(format!("{origin} — {} visible", self.visible_entities));
                } else {
                    ui.colored_label(
                        Color32::YELLOW,
                        format!(
                            "{} entities skipped: {}",
                            skipped.len(),
                            skipped
                                .iter()
                                .map(|s| s.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    );
                    ui.label(format!("{} visible", self.visible_entities));
                }
            }
            AppState::Error(msg) => {
                ui.colored_label(Color32::RED, msg);
            }
        }

        // Icon loading is best-effort, but a silent fallback to label
        // characters looks like a bug rather than a missing install — so
        // say so, and name the way to fix it.
        if self.icons.status() == IconStatus::NoInstall {
            ui.colored_label(
                Color32::from_rgb(180, 180, 180),
                "Entity icons unavailable — no Factorio install found. \
                 Set FACTORIO_INSTALL_DIR to enable them.",
            );
        }

        // Same reasoning as the icon notice above: a toggle that is on and
        // silently does nothing reads as a bug. Say which one it is.
        if let WatchStatus::Unavailable(reason) = self.clipboard.status() {
            ui.colored_label(
                Color32::from_rgb(180, 180, 180),
                format!("Clipboard watching unavailable — {reason}"),
            );
        }

        // Which commit this binary is, always on screen. `launch.sh`
        // launches the *previous* build when one fails, and from in here
        // that fallback is indistinguishable from a healthy launch — the
        // failure notification is a toast in a terminal that closes, this
        // is not. Right-aligned so it sits out of the way of the status
        // line above it rather than competing with it.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let warning = self.build.get().and_then(build_info::Freshness::warning);
            match warning {
                Some(warning) => ui
                    .colored_label(
                        Color32::YELLOW,
                        format!("{} — {warning}", build_info::label()),
                    )
                    .on_hover_text(
                        "This binary was not built from the code that is checked out. \
                         The launcher falls back to the previous build when a build \
                         fails — rebuild and check for the error.",
                    ),
                None => ui.colored_label(Color32::from_gray(120), build_info::label()),
            };
        });
    }
}
