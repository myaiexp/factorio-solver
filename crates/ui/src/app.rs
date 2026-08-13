use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use factorio_blueprint::{decode, Direction};
use factorio_grid::{from_blueprint, Grid, SkippedEntity};

use crate::build_info::{self, FreshnessProbe};
use crate::chain_panel::ChainPanel;
use crate::clipboard::{detect, ClipboardWatcher, WatchStatus};
use crate::entity_draw::EntityPainter;
use crate::icons::{IconCache, IconStatus};
use crate::lod::lod_for_zoom;
use crate::persist::{self, AppSettings, PanelState};
use crate::viewport::ViewportTransform;

fn direction_name(dir: Direction) -> &'static str {
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

// ── App state ──────────────────────────────────────────────────────────

/// Where the grid on screen came from. Only the status bar reads it, but it
/// has to be part of the loaded state rather than a separate flag — the two
/// would otherwise drift the moment a fourth load path appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadSource {
    /// Typed or pasted into the top bar and loaded with the button.
    Pasted,
    /// Picked up by the clipboard watcher with no interaction at all.
    Clipboard,
    /// Handed over by the chain panel's Generate button.
    Generated,
}

/// Top-level application state after blueprint loading.
pub enum AppState {
    /// No blueprint loaded yet.
    Empty,
    /// A blueprint was successfully loaded into a grid.
    Loaded {
        grid: Grid,
        label: Option<String>,
        skipped: Vec<SkippedEntity>,
        source: LoadSource,
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
    /// Number of entities rendered in the last frame (updated by render_viewport).
    /// Used by the bottom status bar to confirm culling is working.
    visible_entities: usize,
    /// When true, `render_viewport` re-fits the camera to the grid using the real
    /// painter size on the next frame (set after a successful blueprint load).
    needs_fit: bool,
    /// Entity icons read from the player's Factorio install. Best-effort: with
    /// no install found this stays empty and the viewport falls back to label
    /// characters, which is what it drew before icons existed.
    icons: IconCache,
    /// Production-chain calculator side panel.
    chain: ChainPanel,
    /// Background clipboard poller — an in-game export loads with no paste.
    clipboard: ClipboardWatcher,
    /// The blueprint string the viewport is currently showing, whatever put it
    /// there. The clipboard watcher compares against this and skips a match,
    /// which is what stops "Copy blueprint" from reloading the app's own
    /// output — see `clipboard::detect::should_load`.
    displayed: Option<String>,
    /// Whether this binary is the checked-out code. Answered once in the
    /// background, then painted in the status bar for the rest of the run.
    build: FreshnessProbe,
}

impl FactorioApp {
    /// `cc` is only used to restore the chain panel's saved inputs — the
    /// window's own geometry is restored by eframe itself once the
    /// "persistence" feature is on (`with_inner_size` in `main.rs` becomes a
    /// first-run default rather than a fixed size, which is desirable, not a
    /// regression).
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = cc
            .storage
            .and_then(|s| eframe::get_value::<AppSettings>(s, persist::APP_KEY))
            .unwrap_or_default();

        let mut app = Self {
            blueprint_input: String::new(),
            state: AppState::Empty,
            viewport: ViewportTransform::new(),
            show_grid_lines: true,
            visible_entities: 0,
            needs_fit: false,
            // Detection runs once, here — not per frame and not per entity.
            icons: IconCache::new(None),
            chain: ChainPanel::new(),
            clipboard: ClipboardWatcher::spawn(&cc.egui_ctx, settings.watch_clipboard()),
            displayed: None,
            build: FreshnessProbe::spawn(&cc.egui_ctx),
        };
        if let Some(storage) = cc.storage
            && let Some(state) = eframe::get_value::<PanelState>(storage, persist::STORAGE_KEY)
        {
            state.apply(&mut app.chain);
        }
        app
    }

    /// An app with a hand-fed clipboard watcher and no eframe behind it.
    /// `FactorioApp::new` needs an `eframe::CreationContext` (not
    /// constructible outside eframe) and would spawn the real arboard thread,
    /// which has nothing to read on a machine with no display server.
    #[cfg(test)]
    fn for_test(clipboard: ClipboardWatcher) -> Self {
        Self {
            blueprint_input: String::new(),
            state: AppState::Empty,
            viewport: ViewportTransform::new(),
            show_grid_lines: true,
            visible_entities: 0,
            needs_fit: false,
            icons: IconCache::new(None),
            chain: ChainPanel::new(),
            clipboard,
            displayed: None,
            // Fixed rather than probed: the real answer depends on this
            // machine's git state, so a test asserting on it would pass on a
            // dev checkout and fail in a sandbox.
            build: FreshnessProbe::ready(crate::build_info::Freshness::Current),
        }
    }

    /// Whether the clipboard watcher is currently armed. Read by
    /// `AppSettings::capture` on the way to storage.
    pub fn watching_clipboard(&self) -> bool {
        self.clipboard.watching()
    }

    /// Decode the current `blueprint_input`, build a grid, and update state.
    fn load_blueprint(&mut self) {
        let input = self.blueprint_input.trim().to_owned();
        self.load_string(&input, LoadSource::Pasted);
    }

    /// Decode one blueprint string into the viewport. The single string→grid
    /// path: the Load button and the clipboard watcher differ only in the
    /// `source` they report, so neither can grow behaviour the other lacks.
    ///
    /// The clipboard path deliberately does *not* write `blueprint_input` on
    /// its way through. Doing so would show the user what arrived, but it
    /// would also overwrite a string they were part-way through typing into
    /// that field — a background poller must not be able to destroy typing.
    fn load_string(&mut self, input: &str, source: LoadSource) {
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

        // Defer fit until `render_viewport` has the real painter size — a hardcoded
        // estimate (e.g. 1280×800) mis-zooms when the window differs in size/aspect.
        self.needs_fit = result.grid.bounding_box().is_some();

        self.displayed = Some(input.to_owned());
        self.state = AppState::Loaded {
            grid: result.grid,
            label,
            skipped: result.skipped,
            source,
        };
    }

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
    fn render_viewport(&mut self, ui: &mut egui::Ui) {
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

impl eframe::App for FactorioApp {
    /// Called periodically and on exit by eframe (persistence feature) — not
    /// something this app schedules itself.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, persist::STORAGE_KEY, &PanelState::capture(&self.chain));
        eframe::set_value(storage, persist::APP_KEY, &AppSettings::capture(self));
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl FactorioApp {
    /// One frame, without eframe. `eframe::Frame` has no public constructor,
    /// so a test can only drive the real UI through a plain `egui::Context` —
    /// which is enough, since nothing here touches the integration. Same split
    /// as `ChainPanel::ui`, and what `app_tests` runs frames against.
    fn ui(&mut self, ctx: &egui::Context) {
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
        });

        // ── Right panel: production-chain calculator ───────────────────
        // Must be declared before the central panel — egui lays out side
        // panels first and gives the remainder to CentralPanel.
        egui::SidePanel::right("chain_panel").show(ctx, |ui| self.chain.ui(ui));

        // A freshly generated block replaces whatever blueprint was loaded
        // before it — same load path a pasted string takes, so the viewport,
        // status bar, and Home-key re-fit all keep working unmodified.
        if let Some(grid) = self.chain.take_generated_grid() {
            self.needs_fit = grid.bounding_box().is_some();
            // Recording the block's own string here is what makes "Copy
            // blueprint" safe with the watcher on: the copy then matches what
            // is already displayed, and `should_load` declines it.
            self.displayed = self.chain.generated_blueprint_string().map(str::to_owned);
            self.state = AppState::Loaded {
                grid,
                label: Some("Generated block".to_string()),
                skipped: vec![],
                source: LoadSource::Generated,
            };
        }

        // Checked after the generate handoff, so a block generated this frame
        // is already `displayed` and cannot be shadowed by a stale candidate.
        if let Some(candidate) = self.clipboard.poll()
            && detect::should_load(&candidate, self.displayed.as_deref())
        {
            self.load_string(&candidate, LoadSource::Clipboard);
        }

        // ── Central panel: viewport ────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_viewport(ui);
        });
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
