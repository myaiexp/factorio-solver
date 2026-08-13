// Side panel that drives the production-chain calculator: pick a product, a
// rate, what's already on the bus, and a machine policy, and see how many
// machines each recipe in the chain needs.
use std::collections::{BTreeSet, HashMap};

use factorio_grid::Grid;
use factorio_solver::availability::{all_available_recipe_names, Availability};
use factorio_solver::chain::{solve, ChainError, ChainGoal, MachinePolicy, ProductionPlan, Rate};
use factorio_solver::layout::{CellTopology, LayoutConfig};
use serde::{Deserialize, Serialize};

mod availability_controls;
#[cfg(test)]
mod availability_tests;
mod controls;
mod generate;
#[cfg(test)]
mod goal_tests;
#[cfg(test)]
mod harness;
mod layout_controls;
mod logic;
#[cfg(test)]
mod render_tests;
mod results;
#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod topology_tests;

// pub(crate), not a plain `use`: `crate::persist` (a sibling of this module,
// not a descendant) names this type in `PanelState` and needs a crate-visible
// path to it — `availability_controls` itself stays private to this module.
pub(crate) use availability_controls::AvailabilityMode;
use generate::GeneratedBlock;
pub use logic::display_name;

/// Rate unit picked in the UI; converted to a `Rate` at solve time.
///
/// Serialize/Deserialize: persisted as part of the chain panel's saved
/// inputs (`persist.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateUnit {
    PerSec,
    PerMin,
    Belts,
}

/// Which machine(s) to use, mirroring `MachinePolicy`'s two constructors —
/// `with_preference` (per-category pins) has no control here yet.
///
/// Serialize/Deserialize: persisted as part of the chain panel's saved
/// inputs (`persist.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
///
/// `persist.rs` saves a subset of these fields (see `PanelState`) across
/// runs — deliberately the *inputs* only, never `result`/`generated`/
/// `pending_grid`. Those three are derived from a solve/generate call against
/// the inputs at the time; restoring them next to freshly-restored inputs
/// would show a plan on screen that no longer matches what's above it (the
/// user could have edited the product or the bus in the meantime, in a
/// version of the app that didn't exist when the result was computed). The
/// panel instead opens with its inputs filled and no result — one click
/// (Solve) from where it left off, never a stale answer presented as current.
///
/// The fields widened to `pub(crate)` below are exactly the ones `PanelState`
/// persists — `persist.rs` lives in a sibling module and needs to read and
/// write them directly rather than through fifteen accessor pairs. That the
/// two sets coincide is not a coincidence to paper over: it is a cheap
/// consistency check. A field that should be persisted but stayed private
/// won't compile in `persist.rs`; a field that got widened but isn't
/// persisted is a visible loose end in this struct.
pub struct ChainPanel {
    /// Doubles as the recipe-picker search query: typing here both names the
    /// goal item and filters the list of recipes shown below it.
    pub(crate) product: String,
    pub(crate) rate_value: f64,
    pub(crate) rate_unit: RateUnit,
    pub(crate) belt_tier: String,
    pub(crate) available: Vec<String>,
    available_input: String,
    /// Everything vs. an edited "what I can build" set. Orthogonal to
    /// `available` above — that pair is the bus, this trio is the
    /// availability gate, and the near-identical names are exactly why a
    /// field here called plain `available_*` would be a landmine. Read
    /// through `availability()`, never matched on directly, so nothing else
    /// has to know `AvailabilityMode` exists.
    pub(crate) availability_mode: AvailabilityMode,
    /// The edited set. `None` means "never seeded" — the state a fresh panel
    /// starts in. That is *not* the same as `Some(empty set)`, which is what
    /// the user gets by ticking `OnlyAvailable` and then unticking every
    /// recipe on purpose; conflating the two would make `set_availability_mode`
    /// re-seed over a deliberate "nothing" and silently discard the edit.
    pub(crate) available_recipes: Option<BTreeSet<String>>,
    availability_query: String,
    pub(crate) machine: MachineChoice,
    pub(crate) show_hidden_recycling: bool,
    /// Item -> recipe name. Populated from the ambiguous-recipe error UI as
    /// well as any future manual override control.
    pub(crate) overrides: HashMap<String, String>,
    result: Option<Result<ProductionPlan, ChainError>>,
    /// The block generator's `LayoutConfig`, as internal prototype names —
    /// distinct from `belt_tier` above, which is only ever the "belts" rate
    /// unit's tier and has no bearing on how a block gets built.
    pub(crate) layout_belt: String,
    pub(crate) layout_pole: String,
    pub(crate) layout_inserter: String,
    pub(crate) layout_long_inserter: String,
    pub(crate) layout_topology: CellTopology,
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
            product: String::new(),
            rate_value: 1.0,
            rate_unit: RateUnit::PerSec,
            belt_tier: "transport-belt".to_string(),
            available: Vec::new(),
            available_input: String::new(),
            availability_mode: AvailabilityMode::Everything,
            available_recipes: None,
            availability_query: String::new(),
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
            .with_availability(self.availability());
        for (item, recipe) in &self.overrides {
            goal = goal.with_override(item, recipe);
        }
        goal
    }

    /// The `Availability` the current state describes: unrestricted in
    /// `Everything` mode, the edited set in `OnlyAvailable`.
    ///
    /// Private, not `pub(super)` — `super` from this file is the crate root,
    /// so `pub(super)` would leak it further out than `AvailabilityMode` in
    /// its return type, which clippy rejects. Private already reaches every
    /// descendant, tests included.
    fn availability(&self) -> Availability {
        match self.availability_mode {
            AvailabilityMode::Everything => Availability::Everything,
            // `unwrap_or_default` reads an unseeded `None` as an empty set —
            // reachable only if `availability_mode` were ever set without
            // going through `set_availability_mode` below, which never
            // happens today. Treat it as the conservative answer (allow
            // only what `enabled: true` already grants for free) rather than
            // a case to paper over with a fallback to `Everything`.
            AvailabilityMode::OnlyAvailable => {
                Availability::Unlocked(self.available_recipes.clone().unwrap_or_default())
            }
        }
    }

    /// Switch modes, seeding the set from [`all_available_recipe_names`] the
    /// first time `OnlyAvailable` is entered — so the switch alone changes no
    /// solve result and the user's first edit is a removal. Seeding is keyed
    /// on `available_recipes.is_none()`, not on the mode being entered again,
    /// so toggling back to `Everything` and returning preserves whatever the
    /// user already unticked instead of re-seeding over it.
    ///
    /// `pub(crate)`, not private: `persist.rs::PanelState::apply` reuses this
    /// exact method to re-seed a restored `OnlyAvailable` mode that carries no
    /// set, rather than re-implementing the seeding rule a second time.
    pub(crate) fn set_availability_mode(&mut self, mode: AvailabilityMode) {
        if mode == AvailabilityMode::OnlyAvailable && self.available_recipes.is_none() {
            self.available_recipes = Some(all_available_recipe_names());
        }
        self.availability_mode = mode;
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
                controls::product_picker(self, ui);
                ui.separator();
                controls::rate_controls(self, ui);
                ui.separator();
                controls::available_list(self, ui);
                ui.separator();
                controls::machine_selector(self, ui);
                ui.separator();
                availability_controls::show(self, ui);
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
