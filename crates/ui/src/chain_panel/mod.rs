// Side panel that drives the production-chain calculator: pick a product, a
// rate, what's already on the bus, and a machine policy, and see how many
// machines each recipe in the chain needs.
use std::collections::HashMap;

use factorio_grid::Grid;
use factorio_solver::chain::{solve, ChainError, ChainGoal, MachinePolicy, ProductionPlan, Rate};
use factorio_solver::layout::{CellTopology, LayoutConfig};

mod controls;
mod generate;
#[cfg(test)]
mod harness;
mod layout_controls;
mod logic;
#[cfg(test)]
mod render_tests;
mod results;
mod save_picker;
#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod topology_tests;

use generate::GeneratedBlock;
pub use logic::display_name;
use save_picker::SavePickerState;

/// Rate unit picked in the UI; converted to a `Rate` at solve time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateUnit {
    PerSec,
    PerMin,
    Belts,
}

/// Which machine(s) to use, mirroring `MachinePolicy`'s two constructors —
/// `with_preference` (per-category pins) has no control here yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineChoice {
    Fastest,
    Named(String),
}

impl MachineChoice {
    fn to_policy(&self) -> MachinePolicy {
        match self {
            MachineChoice::Fastest => MachinePolicy::fastest(),
            MachineChoice::Named(name) => MachinePolicy::all(name),
        }
    }
}

/// All panel input state plus the last solve result.
pub struct ChainPanel {
    /// What save (if any) gates the recipes/machines the solve is allowed to
    /// use. Read straight into `build_goal`'s `availability`, never decoded
    /// there — see `SavePickerState`'s own doc comment.
    save_picker: SavePickerState,
    /// Doubles as the recipe-picker search query: typing here both names the
    /// goal item and filters the list of recipes shown below it.
    product: String,
    rate_value: f64,
    rate_unit: RateUnit,
    belt_tier: String,
    available: Vec<String>,
    available_input: String,
    machine: MachineChoice,
    show_hidden_recycling: bool,
    /// Item -> recipe name. Populated from the ambiguous-recipe error UI as
    /// well as any future manual override control.
    overrides: HashMap<String, String>,
    result: Option<Result<ProductionPlan, ChainError>>,
    /// The block generator's `LayoutConfig`, as internal prototype names —
    /// distinct from `belt_tier` above, which is only ever the "belts" rate
    /// unit's tier and has no bearing on how a block gets built.
    layout_belt: String,
    layout_pole: String,
    layout_inserter: String,
    layout_long_inserter: String,
    layout_topology: CellTopology,
    /// The last call to `generate_block`, if any: stats and a blueprint
    /// string on success, the layout error's message on failure.
    generated: Option<Result<GeneratedBlock, String>>,
    /// The grid `generate_block` just built, waiting for `FactorioApp` to
    /// collect it via `take_generated_grid` and load it into the viewport.
    pending_grid: Option<Grid>,
}

impl Default for ChainPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ChainPanel {
    pub fn new() -> Self {
        // Seeded from `LayoutConfig::default()` rather than repeating its
        // three literals here, so the panel can never drift out of sync with
        // what the generator itself considers the baseline tier.
        let default_layout = LayoutConfig::default();
        Self {
            save_picker: SavePickerState::new(),
            product: String::new(),
            rate_value: 1.0,
            rate_unit: RateUnit::PerSec,
            belt_tier: "transport-belt".to_string(),
            available: Vec::new(),
            available_input: String::new(),
            machine: MachineChoice::Fastest,
            show_hidden_recycling: false,
            overrides: HashMap::new(),
            result: None,
            layout_belt: default_layout.belt_tier,
            layout_pole: default_layout.pole,
            layout_inserter: default_layout.inserter,
            layout_long_inserter: default_layout.long_inserter,
            layout_topology: default_layout.topology,
            generated: None,
            pending_grid: None,
        }
    }

    /// Build the `ChainGoal` the current panel state describes. Takes no
    /// `egui::Ui` so it can be driven directly by tests.
    pub fn build_goal(&self) -> ChainGoal {
        let rate = match self.rate_unit {
            RateUnit::PerSec => Rate::ItemsPerSec(self.rate_value),
            RateUnit::PerMin => Rate::ItemsPerMin(self.rate_value),
            RateUnit::Belts => {
                Rate::Belts { count: self.rate_value.max(0.0).round() as u32, tier: self.belt_tier.clone() }
            }
        };
        let available: Vec<&str> = self.available.iter().map(String::as_str).collect();
        let mut goal = ChainGoal::new(&self.product, rate, &available)
            .with_machines(self.machine.to_policy())
            .with_availability(self.save_picker.availability.clone());
        for (item, recipe) in &self.overrides {
            goal = goal.with_override(item, recipe);
        }
        goal
    }

    /// Re-run the calculator against the current panel state.
    fn solve(&mut self) {
        self.result = Some(solve(&self.build_goal()));
    }

    /// Add an override and immediately re-solve — the escape hatch offered
    /// on `AmbiguousRecipe`, without which the panel would be stuck: the
    /// headline "45/s electronic circuits" case is ambiguous on both
    /// `copper-cable` and `iron-plate`/`copper-plate`.
    fn add_override_and_resolve(&mut self, item: String, recipe: String) {
        self.overrides.insert(item, recipe);
        self.solve();
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // The body scrolls because its height is data-dependent — a row per
        // step, per input, per bus item — so any long enough chain is taller
        // than the window. An egui panel clips its overflow instead of
        // scrolling it, which left everything below the steps table, Generate
        // included, painted nowhere and clickable never.
        //
        // `auto_shrink` off on both axes: the sections below size themselves
        // from `available_width`, and a scroll area left to shrink to its
        // content would hand them the width of the widest row rather than the
        // width of the panel the user dragged out.
        egui::ScrollArea::vertical()
            .id_salt("chain_panel_body")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.heading("Production Chain");
                ui.separator();
                // First section, ahead of the product/machine pickers: the
                // save is what `build_goal`'s `availability` comes from, and
                // the pickers below don't filter by it yet (idea #3385) — a
                // save loaded *after* picking a locked recipe or machine
                // gets no warning until Solve. Putting it first at least
                // answers "is a save loaded" before that choice is made.
                controls::save_picker(self, ui);
                ui.separator();
                controls::product_picker(self, ui);
                ui.separator();
                controls::rate_controls(self, ui);
                ui.separator();
                controls::available_list(self, ui);
                ui.separator();
                controls::machine_selector(self, ui);
                ui.separator();
                if ui.button("Solve").clicked() {
                    self.solve();
                }
                ui.separator();
                results::show(self, ui);
                ui.separator();
                layout_controls::show(self, ui);
                ui.separator();
                generate::show(self, ui);
            });
    }
}

#[cfg(test)]
mod tests;
