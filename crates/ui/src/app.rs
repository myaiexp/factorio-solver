use egui::{Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};
use factorio_blueprint::{decode, Direction};
use factorio_grid::{from_blueprint, Grid, SkippedEntity};

use crate::colors::EntityCategory;
use crate::viewport::ViewportTransform;

fn direction_name(dir: Direction) -> &'static str {
    match dir {
        Direction::North => "North",
        Direction::NorthEast => "NorthEast",
        Direction::East => "East",
        Direction::SouthEast => "SouthEast",
        Direction::South => "South",
        Direction::SouthWest => "SouthWest",
        Direction::West => "West",
        Direction::NorthWest => "NorthWest",
    }
}

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

    /// Render the grid viewport with pan, zoom, grid lines, and entity drawing.
    fn render_viewport(&mut self, ui: &mut egui::Ui) {
        let (response, painter) =
            ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let rect = response.rect;
        let screen_size = (rect.width(), rect.height());

        // ── Pan: drag with primary or middle button ────────────────────
        if response.dragged() {
            let delta = response.drag_delta();
            self.viewport.pan((delta.x, delta.y));
        }

        // ── Zoom: scroll wheel, anchored at mouse position ────────────
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0.0 {
            // Convert mouse position to be relative to the viewport rect origin.
            if let Some(mouse_abs) = ui.input(|i| i.pointer.hover_pos()) {
                let mouse_rel = (mouse_abs.x - rect.left(), mouse_abs.y - rect.top());
                let ticks = scroll_delta / 50.0;
                let factor = 1.1_f32.powf(ticks);
                self.viewport.zoom_at(mouse_rel, screen_size, factor);
                self.viewport.zoom = self.viewport.zoom.clamp(2.0, 200.0);
            }
        }

        // ── Background ────────────────────────────────────────────────
        painter.rect_filled(rect, 0.0, Color32::from_gray(40));

        // ── Grid lines (visible range only) ───────────────────────────
        if self.show_grid_lines {
            let (vw_min_x, vw_min_y, vw_max_x, vw_max_y) =
                self.viewport.visible_world_rect(screen_size);
            let grid_color = Color32::from_gray(60);
            let grid_stroke = Stroke::new(0.5, grid_color);

            let min_col = vw_min_x.floor() as i32;
            let max_col = vw_max_x.ceil() as i32;
            let min_row = vw_min_y.floor() as i32;
            let max_row = vw_max_y.ceil() as i32;

            // Vertical lines
            for col in min_col..=max_col {
                let screen_pos = self.viewport.world_to_screen((col as f32, 0.0), screen_size);
                let x = rect.left() + screen_pos.0;
                if x >= rect.left() && x <= rect.right() {
                    painter.line_segment(
                        [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                        grid_stroke,
                    );
                }
            }

            // Horizontal lines
            for row in min_row..=max_row {
                let screen_pos = self.viewport.world_to_screen((0.0, row as f32), screen_size);
                let y = rect.top() + screen_pos.1;
                if y >= rect.top() && y <= rect.bottom() {
                    painter.line_segment(
                        [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                        grid_stroke,
                    );
                }
            }
        }

        // ── Entity rendering ──────────────────────────────────────────
        if let AppState::Loaded { ref grid, .. } = self.state {
            let zoom = self.viewport.zoom;
            let border_stroke = Stroke::new(1.0, Color32::from_gray(20));

            for entity in grid.entities() {
                let top_left_world = (entity.position.x as f32, entity.position.y as f32);
                let top_left_screen =
                    self.viewport.world_to_screen(top_left_world, screen_size);
                let entity_w = entity.size.0 as f32 * zoom;
                let entity_h = entity.size.1 as f32 * zoom;

                let entity_rect = Rect::from_min_size(
                    Pos2::new(
                        rect.left() + top_left_screen.0,
                        rect.top() + top_left_screen.1,
                    ),
                    Vec2::new(entity_w, entity_h),
                );

                // Cull: skip if entity rect doesn't intersect the painter rect.
                if !rect.intersects(entity_rect) {
                    continue;
                }

                let category = EntityCategory::from_prototype_name(entity.prototype_name);

                // Filled rect with category color.
                painter.rect_filled(entity_rect, 0.0, category.color());

                // Dark border.
                painter.rect_stroke(entity_rect, 0.0, border_stroke, StrokeKind::Outside);

                // Label character when zoomed in enough.
                if zoom > 20.0 {
                    let font_size = (zoom * 0.5).clamp(8.0, 40.0);
                    let label = category.label_char().to_string();
                    painter.text(
                        entity_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        FontId::monospace(font_size),
                        Color32::WHITE,
                    );
                }
            }
        }

        // ── Hover tooltip ──────────────────────────────────────────────
        if let AppState::Loaded { ref grid, .. } = self.state {
            if let Some(mouse_abs) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(mouse_abs) {
                    let mouse_rel = (mouse_abs.x - rect.left(), mouse_abs.y - rect.top());
                    let world = self.viewport.screen_to_world(mouse_rel, screen_size);
                    let cell_x = world.0.floor() as i32;
                    let cell_y = world.1.floor() as i32;

                    if let Some(entity) = grid.get_at(cell_x, cell_y) {
                        let tooltip_id = response.id.with("entity_tooltip");
                        egui::Area::new(tooltip_id)
                            .order(egui::Order::Tooltip)
                            .fixed_pos(mouse_abs + Vec2::new(12.0, 12.0))
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(entity.prototype_name)
                                            .strong(),
                                    );
                                    ui.label(format!(
                                        "Position: ({}, {})",
                                        entity.position.x, entity.position.y
                                    ));
                                    ui.label(format!(
                                        "Size: {}x{}",
                                        entity.size.0, entity.size.1
                                    ));
                                    ui.label(format!(
                                        "Direction: {}",
                                        direction_name(entity.direction)
                                    ));
                                    if let Some(ref recipe) = entity.recipe {
                                        ui.label(format!("Recipe: {recipe}"));
                                    }
                                    if let Some(ref etype) = entity.entity_type {
                                        ui.label(format!("Type: {etype}"));
                                    }
                                });
                            });
                    }
                }
            }
        }

        // ── Home key: re-fit viewport to grid bounds ───────────────────
        if ui.input(|i| i.key_pressed(egui::Key::Home)) {
            if let AppState::Loaded { ref grid, .. } = self.state {
                if let Some((min, max)) = grid.bounding_box() {
                    self.viewport.fit_to_bounds(
                        (min.x as f32, min.y as f32),
                        (max.x as f32, max.y as f32),
                        screen_size,
                        2.0,
                    );
                }
            }
        }
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

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_names() {
        assert_eq!(direction_name(Direction::North), "North");
        assert_eq!(direction_name(Direction::NorthEast), "NorthEast");
        assert_eq!(direction_name(Direction::East), "East");
        assert_eq!(direction_name(Direction::SouthEast), "SouthEast");
        assert_eq!(direction_name(Direction::South), "South");
        assert_eq!(direction_name(Direction::SouthWest), "SouthWest");
        assert_eq!(direction_name(Direction::West), "West");
        assert_eq!(direction_name(Direction::NorthWest), "NorthWest");
    }
}
