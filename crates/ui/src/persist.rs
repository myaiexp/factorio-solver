// Saves and restores the chain panel's *inputs* (not its solve/generate
// results) and the app-level settings across app runs, via eframe's
// key-value storage.
use std::collections::{BTreeSet, HashMap};

use factorio_solver::layout::CellTopology;
use factorio_solver::recipe;
use serde::{Deserialize, Serialize};

use crate::app::FactorioApp;
use crate::chain_panel::{AvailabilityMode, ChainPanel, MachineChoice, RateUnit};

#[cfg(test)]
#[path = "persist_tests.rs"]
mod tests;

/// Storage key for the panel blob.
pub const STORAGE_KEY: &str = "factorio_solver_panel";

/// Storage key for the app-level settings blob. A second key rather than a
/// field on `PanelState`, because these are not the chain panel's inputs and
/// folding them in would make the panel's own restore rules (filtering retired
/// recipe names, reseeding an availability mode) apply to things they have no
/// business touching.
pub const APP_KEY: &str = "factorio_solver_app";

/// App-level settings that outlive a run.
///
/// Same leniency contract as `PanelState`: `#[serde(default)]` at the
/// container level, unknown fields ignored, so a later version adding a
/// setting cannot wipe a saved one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    watch_clipboard: bool,
}

impl Default for AppSettings {
    /// Watching defaults **on**: the whole point is that an in-game export
    /// needs no interaction, and a feature you have to find a checkbox for
    /// first is one you will not remember exists. Turning it off is a
    /// deliberate choice, which is exactly why it is worth persisting.
    fn default() -> Self {
        Self { watch_clipboard: true }
    }
}

impl AppSettings {
    pub fn capture(app: &FactorioApp) -> Self {
        Self { watch_clipboard: app.watching_clipboard() }
    }

    pub fn watch_clipboard(&self) -> bool {
        self.watch_clipboard
    }
}

/// The chain panel's persisted inputs.
///
/// Deliberately excludes `ChainPanel::result`, `generated` and
/// `pending_grid` — those are a solve/generate call's *output*, computed
/// against the inputs at the moment the user clicked the button. Restoring
/// them next to freshly-restored inputs would show a plan on screen that no
/// longer matches what's above it (the goal could differ, an item could have
/// been renamed by a game update between saves) — a lie on screen rather
/// than a convenience. The panel instead always reopens with its inputs
/// filled and no result: one click (Solve) from where it left off, never a
/// stale answer presented as current.
///
/// `#[serde(default)]` at the container level makes every field
/// independently optional on read: a blob missing a field restores that one
/// field to a fresh panel's default (see the `Default` impl below) and
/// leaves the rest alone; a blob carrying a field that no longer exists is
/// silently ignored (serde's ordinary behaviour — `deny_unknown_fields` is
/// deliberately not set). Adding or removing a field in a later version must
/// never wipe a saved setup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelState {
    product: String,
    rate_value: f64,
    rate_unit: RateUnit,
    belt_tier: String,
    available: Vec<String>,
    availability_mode: AvailabilityMode,
    available_recipes: Option<BTreeSet<String>>,
    machine: MachineChoice,
    show_hidden_recycling: bool,
    overrides: HashMap<String, String>,
    layout_belt: String,
    layout_pole: String,
    layout_inserter: String,
    layout_long_inserter: String,
    layout_topology: CellTopology,
}

impl Default for PanelState {
    /// Written from `ChainPanel::new()` rather than derived: a derived
    /// `Default` would give `rate_value: 0.0` and an empty `belt_tier`,
    /// where a fresh panel actually starts at `1.0` and `"transport-belt"`.
    /// A blob missing those fields would then restore something a fresh
    /// panel never shows. Routing through the panel's own constructor makes
    /// that drift impossible instead of relying on two literals kept in
    /// sync by hand.
    fn default() -> Self {
        Self::capture(&ChainPanel::new())
    }
}

impl PanelState {
    /// Snapshot a panel's persisted inputs. Called both to write storage
    /// (`FactorioApp::save`) and by `Default` above.
    pub fn capture(panel: &ChainPanel) -> Self {
        Self {
            product: panel.product.clone(),
            rate_value: panel.rate_value,
            rate_unit: panel.rate_unit,
            belt_tier: panel.belt_tier.clone(),
            available: panel.available.clone(),
            availability_mode: panel.availability_mode,
            available_recipes: panel.available_recipes.clone(),
            machine: panel.machine.clone(),
            show_hidden_recycling: panel.show_hidden_recycling,
            overrides: panel.overrides.clone(),
            layout_belt: panel.layout_belt.clone(),
            layout_pole: panel.layout_pole.clone(),
            layout_inserter: panel.layout_inserter.clone(),
            layout_long_inserter: panel.layout_long_inserter.clone(),
            layout_topology: panel.layout_topology.clone(),
        }
    }

    /// Write this state onto a panel — normally a fresh `ChainPanel::new()`
    /// at startup.
    pub fn apply(self, panel: &mut ChainPanel) {
        panel.product = self.product;
        panel.rate_value = self.rate_value;
        panel.rate_unit = self.rate_unit;
        panel.belt_tier = self.belt_tier;
        panel.available = self.available;
        panel.machine = self.machine;
        panel.show_hidden_recycling = self.show_hidden_recycling;
        // `overrides` and the `layout_*` prototype names are deliberately
        // NOT filtered against the current registries the way
        // `available_recipes` is below. Both are consulted by name at
        // solve/generate time and already produce a legible, actionable
        // error naming exactly what's missing (`ChainError::UnknownItem`, a
        // `LayoutError` variant) if a game update ever retired one — a loud
        // error the user can fix beats a silent drop. `available_recipes` is
        // the opposite case: a ghost recipe name left in the set is
        // invisible (nothing downstream ever surfaces it) and would only
        // quietly skew the availability section's "N of M available" count.
        panel.overrides = self.overrides;
        panel.layout_belt = self.layout_belt;
        panel.layout_pole = self.layout_pole;
        panel.layout_inserter = self.layout_inserter;
        panel.layout_long_inserter = self.layout_long_inserter;
        panel.layout_topology = self.layout_topology;

        // Drop recipe names a game update retired — see the comment above.
        panel.available_recipes = self
            .available_recipes
            .map(|set| set.into_iter().filter(|name| recipe::get(name).is_some()).collect());
        panel.availability_mode = self.availability_mode;

        // A blob can name `OnlyAvailable` with no set attached (reachable
        // from a truncated or hand-edited blob, since every field here is
        // independently optional). Route through the panel's own seeding
        // rather than re-implementing "empty means seed from Everything" a
        // second time here, so the two rules can never drift apart.
        if panel.availability_mode == AvailabilityMode::OnlyAvailable
            && panel.available_recipes.is_none()
        {
            panel.set_availability_mode(AvailabilityMode::OnlyAvailable);
        }
    }
}
