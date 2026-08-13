// The "Generate" button: turns the last solved plan into a Grid and a
// pasteable blueprint string, and hands the grid to the viewport once.
//
// Errors are stored as their already-complete `Display` message rather than
// the `LayoutError` value itself, because `LayoutError` (unlike `ChainError`)
// does not implement `Clone`, and `results.rs`'s clone-before-render pattern
// needs a cloneable value to match its style.
use factorio_blueprint::{encode, BlueprintData};
use factorio_grid::{to_blueprint, Grid};
use factorio_solver::layout::{generate_with_report, LayoutConfig, BLUEPRINT_VERSION};

use super::ChainPanel;

/// The generator's output, ready to show and to copy. The `Grid` itself is
/// not kept here — it lives in `ChainPanel::pending_grid` so
/// `take_generated_grid` can hand it to the viewport exactly once.
#[derive(Clone, Debug)]
pub(super) struct GeneratedBlock {
    pub(super) entity_count: usize,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) blueprint_string: String,
    pub(super) warnings: Vec<String>,
    /// `(recipe, bound_by)` from `Validation::bindings` — the stream that
    /// capped how many machines fit in one cell, per step. The knob-turning
    /// hint, not an error: it tells the player which stream to widen a
    /// topology or belt tier for if they want fewer, bigger cells.
    pub(super) bindings: Vec<(String, String)>,
}

impl ChainPanel {
    /// Take the most recently generated grid, if any, so the caller can load
    /// it into the viewport. Yields it exactly once — the panel has no
    /// viewport of its own to keep showing it in, so holding onto it after
    /// handoff would only let a second, stale load happen by accident.
    pub fn take_generated_grid(&mut self) -> Option<Grid> {
        self.pending_grid.take()
    }

    /// The blueprint string for the block currently shown in the panel — the
    /// same text "Copy blueprint" puts on the clipboard.
    ///
    /// The app records it as what the viewport is displaying, so the clipboard
    /// watcher recognises the app's own output and declines to re-import it.
    /// Unlike `take_generated_grid` this does not consume: the panel keeps
    /// showing the string, and the copy button can be pressed any number of
    /// times.
    pub fn generated_blueprint_string(&self) -> Option<&str> {
        match self.generated {
            Some(Ok(ref block)) => Some(&block.blueprint_string),
            _ => None,
        }
    }

    /// Run the block generator against the last solved plan and the current
    /// layout controls. A no-op if the last solve did not succeed: the
    /// button that calls this is disabled in that case, but checking again
    /// here means a stale `self.result` can never produce a stale block.
    pub(super) fn generate_block(&mut self) {
        let Some(Ok(plan)) = &self.result else { return };
        let cfg = LayoutConfig::new(&self.layout_belt, &self.layout_pole, &self.layout_inserter)
            .with_long_inserter(&self.layout_long_inserter)
            .with_topology(self.layout_topology.clone());

        let (grid, validation) = match generate_with_report(plan, &cfg) {
            Ok(pair) => pair,
            Err(e) => {
                self.generated = Some(Err(e.to_string()));
                return;
            }
        };

        let label = Some(format!("{} block", self.product));
        let data = BlueprintData {
            blueprint: Some(to_blueprint(&grid, label, BLUEPRINT_VERSION)),
            blueprint_book: None,
        };
        let blueprint_string = match encode(&data) {
            Ok(s) => s,
            Err(e) => {
                self.generated = Some(Err(e.to_string()));
                return;
            }
        };

        let (width, height) = grid
            .bounding_box()
            .map(|(min, max)| (max.x - min.x + 1, max.y - min.y + 1))
            .unwrap_or((0, 0));

        self.generated = Some(Ok(GeneratedBlock {
            entity_count: grid.entity_count(),
            width,
            height,
            blueprint_string,
            warnings: validation.warnings,
            bindings: validation.bindings,
        }));
        self.pending_grid = Some(grid);
    }
}

/// The button plus whatever it last produced: stats and a copyable string on
/// success, the plain error message on failure.
pub(super) fn show(panel: &mut ChainPanel, ui: &mut egui::Ui) {
    let plan_ready = matches!(panel.result, Some(Ok(_)));
    if ui.add_enabled(plan_ready, egui::Button::new("Generate")).clicked() {
        panel.generate_block();
    }
    if !plan_ready {
        ui.label("Solve a plan first.");
    }

    // Cloned for the same reason `results::show` clones `panel.result`: the
    // TextEdit buffer below needs a `&mut String` it owns, not one borrowed
    // out of `panel`, since `panel` itself is otherwise free of the borrow.
    let Some(outcome) = panel.generated.clone() else { return };
    match outcome {
        Ok(block) => show_block(ui, block),
        Err(msg) => {
            ui.colored_label(egui::Color32::RED, msg);
        }
    }
}

fn show_block(ui: &mut egui::Ui, mut block: GeneratedBlock) {
    ui.label(format!("{} x {} tiles, {} entities", block.width, block.height, block.entity_count));

    if !block.warnings.is_empty() {
        ui.heading("Warnings");
        for warning in &block.warnings {
            ui.colored_label(egui::Color32::YELLOW, warning);
        }
    }

    // Not an error — the knob-turning hint: which stream to widen a topology
    // or belt tier for if the player wants fewer, bigger cells out of this step.
    if !block.bindings.is_empty() {
        ui.heading("Cell sizing");
        for (recipe, bound_by) in &block.bindings {
            ui.label(format!("{recipe} step limited by {bound_by}"));
        }
    }

    // Read-only: `interactive(false)` still needs a `&mut String` to bind to
    // (egui's widget model, not an invitation to edit), and thousands of
    // characters render fine in a capped-height scrollable box.
    egui::ScrollArea::vertical().max_height(120.0).id_salt("chain_blueprint_string").show(ui, |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut block.blueprint_string)
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .interactive(false),
        );
    });

    if ui.button("Copy blueprint").clicked() {
        ui.ctx().copy_text(block.blueprint_string.clone());
    }
}
