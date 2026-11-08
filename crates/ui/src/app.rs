// App shell: load state, the load paths, and the frame layout around the viewport.
use factorio_blueprint::{decode, Blueprint};
use factorio_grid::{from_blueprint, Grid, SkippedEntity};

use crate::book::BookBrowser;
use crate::build_info::FreshnessProbe;
use crate::chain_panel::ChainPanel;
use crate::clipboard::{detect, ClipboardWatcher};
use crate::icons::IconCache;
use crate::persist::{self, AppSettings, PanelState};
use crate::viewport::ViewportTransform;

mod bars;
mod viewport_render;

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
    /// The book the viewport's blueprint came from, when it came from one.
    /// Cleared by every load that is not a book, so the entry picker never
    /// outlives what it browses.
    book: Option<BookBrowser>,
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
            book: None,
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
            book: None,
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

        // The whole string, book or not, is what is on screen: the watcher
        // compares against it, so re-copying the same book does not reload.
        self.displayed = Some(input.to_owned());
        self.book = None;
        if let Some(book) = data.blueprint_book {
            match BookBrowser::new(book) {
                Ok(browser) => {
                    let entry = browser.selected();
                    let label = format!("{} / {}", browser.label(), entry.path);
                    let blueprint = entry.blueprint.clone();
                    self.book = Some(browser);
                    self.show_blueprint(&blueprint, Some(label), source);
                }
                Err(msg) => self.state = AppState::Error(msg),
            }
            return;
        }

        match data.blueprint {
            Some(bp) => self.show_blueprint(&bp, bp.label.clone(), source),
            None => self.state = AppState::Error("No blueprint found in data".into()),
        }
    }

    /// Shows the open book's entry `index`, keeping the book and the load
    /// source. A no-op without a book or when `index` is already shown.
    fn show_book_entry(&mut self, index: usize) {
        let Some(browser) = self.book.as_mut() else { return };
        if !browser.select(index) {
            return;
        }
        let label = format!("{} / {}", browser.label(), browser.selected().path);
        let blueprint = browser.selected().blueprint.clone();
        let source = match self.state {
            AppState::Loaded { source, .. } => source,
            _ => LoadSource::Pasted,
        };
        self.show_blueprint(&blueprint, Some(label), source);
    }

    /// Builds a grid from one blueprint and puts it in the viewport.
    fn show_blueprint(&mut self, blueprint: &Blueprint, label: Option<String>, source: LoadSource) {
        let result = from_blueprint(blueprint);

        // Defer fit until `render_viewport` has the real painter size — a hardcoded
        // estimate (e.g. 1280×800) mis-zooms when the window differs in size/aspect.
        self.needs_fit = result.grid.bounding_box().is_some();
        self.state = AppState::Loaded {
            grid: result.grid,
            label,
            skipped: result.skipped,
            source,
        };
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
        // Bars first: egui gives side and central panels what they leave.
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| self.top_bar(ui));
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| self.status_bar(ui));

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
            self.book = None;
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
