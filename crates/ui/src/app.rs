use egui::Color32;
use factorio_blueprint::decode;
use factorio_grid::{from_blueprint, Grid, SkippedEntity};

use crate::viewport::ViewportTransform;

// ── App state ──────────────────────────────────────────────────────────

/// Top-level application state after blueprint loading.
pub enum AppState {
    /// No blueprint loaded yet.
    Empty,
    /// A blueprint was successfully loaded into a grid.
    Loaded {
        grid: Grid,
        label: Option<String>,
        skipped: Vec<SkippedEntity>,
    },
    /// The last load attempt failed.
    Error(String),
}

/// Root application struct for the Factorio Layout Solver GUI.
pub struct FactorioApp {
    /// Raw blueprint string from the text input.
    blueprint_input: String,
    /// Current load state.
    state: AppState,
    /// Camera transform (pan/zoom).
    viewport: ViewportTransform,
    /// Whether to draw grid lines in the viewport.
    show_grid_lines: bool,
}

impl FactorioApp {
    pub fn new() -> Self {
        Self {
            blueprint_input: String::new(),
            state: AppState::Empty,
            viewport: ViewportTransform::new(),
            show_grid_lines: true,
        }
    }

    /// Decode the current `blueprint_input`, build a grid, and update state.
    fn load_blueprint(&mut self) {
        let input = self.blueprint_input.trim();
        if input.is_empty() {
            self.state = AppState::Error("No blueprint string provided".into());
            return;
        }

        let data = match decode(input) {
            Ok(d) => d,
            Err(e) => {
                self.state = AppState::Error(format!("Decode error: {e}"));
                return;
            }
        };

        if data.blueprint_book.is_some() {
            self.state = AppState::Error("Blueprint books not yet supported".into());
            return;
        }

        let blueprint = match data.blueprint {
            Some(bp) => bp,
            None => {
                self.state = AppState::Error("No blueprint found in data".into());
                return;
            }
        };

        let label = blueprint.label.clone();
        let result = from_blueprint(&blueprint);

        // Auto-fit viewport to the loaded grid.
        if let Some((min, max)) = result.grid.bounding_box() {
            self.viewport.fit_to_bounds(
                (min.x as f32, min.y as f32),
                (max.x as f32, max.y as f32),
                (1280.0, 800.0), // initial estimate; re-fitted on first paint if needed
                2.0,
            );
        }

        self.state = AppState::Loaded {
            grid: result.grid,
            label,
            skipped: result.skipped,
        };
    }

    /// Placeholder viewport renderer — paints a dark gray background.
    /// Tasks 3 and 4 will fill this in with entity rendering and interaction.
    fn render_viewport(&mut self, ui: &mut egui::Ui) {
        let (rect, _response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_gray(40));
    }
}

impl eframe::App for FactorioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── Top panel: input controls ──────────────────────────────────
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
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
        });

        // ── Bottom panel: status / messages ────────────────────────────
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            match &self.state {
                AppState::Empty => {
                    ui.label("Paste a blueprint string and click Load");
                }
                AppState::Loaded { skipped, .. } => {
                    if skipped.is_empty() {
                        ui.label("Blueprint loaded successfully");
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
                    }
                }
                AppState::Error(msg) => {
                    ui.colored_label(Color32::RED, msg);
                }
            }
        });

        // ── Central panel: viewport ────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_viewport(ui);
        });
    }
}
