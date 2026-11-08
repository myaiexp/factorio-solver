// Draws the grid viewport: pan/zoom input, grid lines, culled entities, hover tooltip.
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use factorio_blueprint::Direction;

use super::{AppState, FactorioApp};
use crate::entity_draw::EntityPainter;
use crate::lod::lod_for_zoom;

pub(super) fn direction_name(dir: Direction) -> &'static str {
    match dir {
        Direction::North => "North",
        Direction::NorthNorthEast => "NNE",
        Direction::NorthEast => "NE",
        Direction::EastNorthEast => "ENE",
        Direction::East => "East",
        Direction::EastSouthEast => "ESE",
        Direction::SouthEast => "SE",
        Direction::SouthSouthEast => "SSE",
        Direction::South => "South",
        Direction::SouthSouthWest => "SSW",
        Direction::SouthWest => "SW",
        Direction::WestSouthWest => "WSW",
        Direction::West => "West",
        Direction::WestNorthWest => "WNW",
        Direction::NorthWest => "NW",
        Direction::NorthNorthWest => "NNW",
    }
}

impl FactorioApp {
    /// Render the grid viewport with pan, zoom, grid lines, and entity drawing.
    ///
    /// ## Performance Design
    ///
    /// This function uses three layered optimisations to remain fast with large blueprints:
    ///
    /// 1. **Spatial culling via `Grid::query_rect`** — instead of iterating every entity in the
    ///    grid, the viewport's visible world rectangle is computed and passed to `query_rect`.
    ///    The underlying `SpatialIndex` (16×16 chunk buckets) returns only entities that overlap
    ///    the visible area, so render cost scales with *visible* entity count, not total count.
    ///
    /// 2. **Level-of-Detail via `lod_for_zoom`** — the current zoom level is mapped to one of
    ///    three `LodLevel` tiers:
    ///    - `Full`    (zoom ≥ 16 px/cell) — filled rect + border stroke + label character.
    ///    - `Medium`  (4 ≤ zoom < 16)     — filled rect only; stroke and text skipped.
    ///    - `Minimal` (zoom < 4)           — single `rect_filled` with a muted colour; at this
    ///      scale entities are < 4 px wide so individual detail is imperceptible.
    ///
    /// 3. **O(1) bounding-box cache** — `Grid::bounding_box()` returns a cached
    ///    `Option<(i32,i32,i32,i32)>` maintained incrementally by `Grid::place`/`Grid::remove`.
    ///    The camera fit-to-blueprint operation (and any code that needs the grid extent) costs
    ///    O(1) rather than scanning all occupied cells.
    pub(super) fn render_viewport(&mut self, ui: &mut egui::Ui) {
        let (response, painter) =
            ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let rect = response.rect;
        let screen_size = (rect.width(), rect.height());

        // ── Deferred fit after load (real painter size, not a hardcoded estimate) ──
        if self.needs_fit
            && screen_size.0 > 0.0
            && screen_size.1 > 0.0
            && let AppState::Loaded { ref grid, .. } = self.state
            && let Some((min, max)) = grid.bounding_box()
        {
            self.viewport.fit_to_bounds(
                (min.x as f32, min.y as f32),
                (max.x as f32, max.y as f32),
                screen_size,
                2.0,
            );
            self.needs_fit = false;
        }

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
                self.viewport.clamp_zoom();
            }
        }

        // ── Background ────────────────────────────────────────────────
        painter.rect_filled(rect, 0.0, Color32::from_gray(40));

        // ── Grid lines (visible range only) ───────────────────────────
        if self.show_grid_lines {
            let (vw_min_x, vw_min_y, vw_max_x, vw_max_y) =
                self.viewport.visible_world_rect(screen_size);
            let grid_color = Color32::from_gray(60);
            let grid_stroke = Stroke::new(0.5_f32, grid_color);

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
        //
        // Performance design — three layers:
        //   1. Spatial culling: `grid.query_rect` uses the chunk-based SpatialIndex
        //      to return only entities whose footprint overlaps the visible viewport,
        //      skipping all entities outside the camera frustum in O(chunks touched).
        //   2. LOD (level of detail): rendering detail is scaled to zoom level —
        //      full detail (rect + border + label) at high zoom, rect-only at medium
        //      zoom, and a single filled rect at minimal zoom (see lod.rs / Phase 4).
        //   3. Bounding-box cache: `grid.bounding_box()` is O(1) via the incremental
        //      `bbox` field, avoiding a full cell scan on every fit-to-bounds call.
        let mut visible_count = 0usize;
        if let AppState::Loaded { ref grid, .. } = self.state {
            let zoom = self.viewport.zoom;
            let lod = lod_for_zoom(zoom);
            let border_stroke = Stroke::new(1.0_f32, Color32::from_gray(20));

            // Compute visible world rect and convert to integer cell coordinates.
            let (vw_min_x, vw_min_y, vw_max_x, vw_max_y) =
                self.viewport.visible_world_rect(screen_size);
            let query_min_x = vw_min_x.floor() as i32;
            let query_min_y = vw_min_y.floor() as i32;
            let query_max_x = vw_max_x.ceil() as i32;
            let query_max_y = vw_max_y.ceil() as i32;

            // Fetch only entities that overlap the visible rectangle — O(chunks touched).
            let visible = grid.query_rect(query_min_x, query_min_y, query_max_x, query_max_y);
            visible_count = visible.len();

            let mut entity_painter = EntityPainter {
                painter: &painter,
                ctx: ui.ctx(),
                icons: &mut self.icons,
                lod,
                zoom,
                border_stroke,
            };

            for entity in &visible {
                let top_left_world = (entity.top_left.x as f32, entity.top_left.y as f32);
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

                entity_painter.draw(entity_rect, entity.prototype_name);
            }
        }
        // Store visible count for the status bar (shown next frame — imperceptible lag).
        self.visible_entities = visible_count;

        // ── Hover tooltip ──────────────────────────────────────────────
        if let AppState::Loaded { ref grid, .. } = self.state
            && let Some(mouse_abs) = ui.input(|i| i.pointer.hover_pos())
            && rect.contains(mouse_abs)
        {
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
                                egui::RichText::new(entity.prototype_name).strong(),
                            );
                            // Both coordinates are shown because they mean different
                            // things: the top-left cell is what the grid indexes on,
                            // while "position" in a Factorio blueprint is the center.
                            ui.label(format!(
                                "Top-left: ({}, {})",
                                entity.top_left.x, entity.top_left.y
                            ));
                            ui.label(format!(
                                "Center: ({}, {})",
                                entity.center.x, entity.center.y
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

        // ── Home key: re-fit viewport to grid bounds ───────────────────
        if ui.input(|i| i.key_pressed(egui::Key::Home))
            && let AppState::Loaded { ref grid, .. } = self.state
            && let Some((min, max)) = grid.bounding_box()
        {
            self.viewport.fit_to_bounds(
                (min.x as f32, min.y as f32),
                (max.x as f32, max.y as f32),
                screen_size,
                2.0,
            );
        }
    }
}
