// Block generator: a ProductionPlan in, a Grid of real entities out.
//
// Geometry is computed, never searched — there is no A* here. That is what
// lets the generator ship before a belt router exists, and it is why it
// cannot emit the subtly-broken blueprints a half-working router would.
//
// The block is columnar cells, not horizontal rows: `cell::size_step` fixes
// how many machines fit in one cell from belt throughput alone (an inserter
// always drops on the *far* lane of the belt it serves — `lane.rs` — so a
// belt needs a machine column on each side to use both its lanes), and
// `tile::place_step` tiles those cells left to right, wrapping into a new
// band when `LayoutConfig::topology.target_width` caps how wide one may run.
//
// Layout knows nothing about recipes beyond what a step carries; the chain
// calculator knows nothing about geometry. `ProductionPlan` is the whole
// interface between them.
use factorio_grid::prototype::{self, EntityPrototype};
use factorio_grid::{Grid, GridPos};

use crate::chain::{ProductionPlan, ProductionStep};

pub mod cell;
pub mod error;
pub mod lane;
pub mod place;
pub mod power;
pub mod tile;
pub mod validate;

pub use cell::{size_step, CellPlan, CellTopology, Side};
pub use error::LayoutError;
pub use lane::{drop_lane, lane_throughput, LaneSide};
pub use place::{cell_width, place_cell, CellExtent};
pub use power::{coverage_gaps, place_poles};
pub use tile::{place_step, StepExtent};
pub use validate::{validate, Validation};

/// One tile of clear space between steps, so a step's output belt never sits
/// flush against the next step's input belt and poles have somewhere to go.
pub const STEP_GAP: i32 = 1;

/// The Factorio version stamped into generated blueprints, matching the dump
/// the registries were built from (2.0.77).
///
/// Factorio packs a version as `major << 48 | minor << 32 | patch << 16 | dev`.
/// It matters beyond cosmetics: `from_blueprint` reads the major version to
/// decide whether a blueprint's directions are 1.x-encoded, so stamping a
/// too-low version would have our own 2.0 directions rewritten on re-import.
pub const BLUEPRINT_VERSION: u64 = (2 << 48) | (77 << 16);

/// The entities the generator builds a block out of. Everything else is
/// derived from the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutConfig {
    /// e.g. `express-transport-belt`.
    pub belt_tier: String,
    /// e.g. `medium-electric-pole`.
    pub pole: String,
    /// e.g. `fast-inserter`.
    pub inserter: String,
    /// Reaches the *outer* belt of a pair, two tiles away — e.g.
    /// `long-handed-inserter`. Only placed where a topology puts two belts on
    /// one side of a machine column; a one-belt side never uses it.
    pub long_inserter: String,
    /// The columnar cell shape `cell::size_step` sizes against. Defaults to
    /// [`CellTopology::default`]; override with [`LayoutConfig::with_topology`].
    pub topology: CellTopology,
}

/// The reach a `long_inserter` must have, in tiles. Anything shorter cannot
/// touch the outer belt of a pair, and a block built with it would have
/// inserters swinging at empty ground.
pub const LONG_INSERTER_REACH: f64 = 2.0;

impl LayoutConfig {
    /// The long inserter defaults to `long-handed-inserter`; override it with
    /// [`LayoutConfig::with_long_inserter`]. It is not a `new` argument
    /// because there is exactly one vanilla answer, and every caller that
    /// does not place two belts on a side never uses it at all.
    pub fn new(belt_tier: &str, pole: &str, inserter: &str) -> Self {
        Self {
            belt_tier: belt_tier.to_string(),
            pole: pole.to_string(),
            inserter: inserter.to_string(),
            long_inserter: "long-handed-inserter".to_string(),
            topology: CellTopology::default(),
        }
    }

    pub fn with_long_inserter(mut self, long_inserter: &str) -> Self {
        self.long_inserter = long_inserter.to_string();
        self
    }

    pub fn with_topology(mut self, topology: CellTopology) -> Self {
        self.topology = topology;
        self
    }

    /// Resolve every name against the prototype registry up front, so a typo
    /// fails before any entity is placed rather than half way through a grid.
    pub fn resolve(&self) -> Result<ResolvedConfig, LayoutError> {
        let belt = prototype::lookup(&self.belt_tier)
            .filter(|p| p.belt_throughput.is_some())
            .ok_or_else(|| LayoutError::BeltTierUnknown(self.belt_tier.clone()))?;
        let pole = prototype::lookup(&self.pole)
            .filter(|p| p.supply_area_distance.is_some())
            .ok_or_else(|| LayoutError::PoleUnknown(self.pole.clone()))?;
        let inserter = prototype::lookup(&self.inserter)
            .filter(|p| p.pickup_position.is_some() && p.insert_position.is_some())
            .ok_or_else(|| LayoutError::InserterUnknown(self.inserter.clone()))?;
        // Reach is checked, not just inserter-ness: a `fast-inserter` named
        // here would resolve fine and then place inserters that reach the
        // inner belt twice and the outer one never.
        let long_inserter = prototype::lookup(&self.long_inserter)
            .filter(|p| p.insert_position.is_some())
            .filter(|p| reach(p) >= LONG_INSERTER_REACH)
            .ok_or_else(|| LayoutError::LongInserterUnknown(self.long_inserter.clone()))?;
        Ok(ResolvedConfig { belt, pole, inserter, long_inserter })
    }
}

/// How far an inserter's arm reaches to pick up, in tiles, from its own
/// prototype data. `0.0` for anything with no pickup position.
pub fn reach(inserter: &EntityPrototype) -> f64 {
    inserter.pickup_position.map_or(0.0, |(x, y)| x.abs().max(y.abs()))
}

impl Default for LayoutConfig {
    /// The tier every base has by the time it is building blocks at all.
    fn default() -> Self {
        Self::new("transport-belt", "medium-electric-pole", "inserter")
    }
}

/// `LayoutConfig` with every name already looked up. Carrying the prototypes
/// rather than the names keeps the placement code from re-resolving (and
/// re-erroring on) the same three strings per machine.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedConfig {
    pub belt: &'static EntityPrototype,
    pub pole: &'static EntityPrototype,
    pub inserter: &'static EntityPrototype,
    pub long_inserter: &'static EntityPrototype,
}

/// Turn a plan into a placed grid, ready for `factorio_grid::to_blueprint`.
///
/// Discards the validation report. Use `generate_with_report` where the
/// warnings matter — `Validation::warnings` is the plan's own soft findings
/// from the chain calculator, carried through unchanged; the layout phase's
/// own findings are hard errors instead (an under-delivering block is
/// `LayoutError::UnderDelivers`, never a warning a caller could ignore).
pub fn generate(plan: &ProductionPlan, cfg: &LayoutConfig) -> Result<Grid, LayoutError> {
    generate_with_report(plan, cfg).map(|(grid, _)| grid)
}

/// `generate`, plus the soft findings from the pre-emit checks.
pub fn generate_with_report(
    plan: &ProductionPlan,
    cfg: &LayoutConfig,
) -> Result<(Grid, Validation), LayoutError> {
    let grid = build(plan, cfg)?;
    let report = validate::validate(&grid, plan, cfg)?;
    Ok((grid, report))
}

fn build(plan: &ProductionPlan, cfg: &LayoutConfig) -> Result<Grid, LayoutError> {
    let resolved = cfg.resolve()?;

    if plan.steps.is_empty() {
        return Err(LayoutError::EmptyPlan);
    }
    for step in &plan.steps {
        reject_if_cyclic(step)?;
    }

    // Steps arrive topologically ordered, producers first, so stacking them
    // downward in plan order puts each step's output belt above the step that
    // consumes it. Wiring the two together is still the player's job — belt
    // routing between steps is a later phase.
    let mut grid = Grid::new();
    let mut y = 0;
    for step in &plan.steps {
        let extent = tile::place_step(&mut grid, step, &resolved, &cfg.topology, GridPos { x: 0, y })?;
        y += extent.height as i32 + STEP_GAP;
    }

    // Poles last: they fill the gaps the machine cells left, so they need the
    // finished geometry to aim at.
    power::place_poles(&mut grid, &resolved)?;

    Ok(grid)
}

/// A step whose recipe both consumes and produces the same item has no
/// realization in the belt-fed-row topology: it needs a return belt looping
/// its own output back to its input. Refused by name rather than approximated.
///
/// Keyed on the *recipe*, which is where the loop is defined, rather than on
/// the step's `inputs`/`outputs`, which are derived rates that happen to show
/// it today (they are gross, so kovarex lists 40/s in and 41/s out) but that
/// the rate solver already nets elsewhere and could net here too.
fn reject_if_cyclic(step: &ProductionStep) -> Result<(), LayoutError> {
    let mut looped: Vec<&str> = step
        .recipe
        .results
        .iter()
        .filter(|r| step.recipe.ingredients.iter().any(|i| i.name == r.name))
        .map(|r| r.name.as_str())
        .collect();
    looped.sort_unstable(); // stable message despite registry ordering

    match looped.first() {
        None => Ok(()),
        Some(item) => Err(LayoutError::CyclicStep {
            recipe: step.recipe.name.clone(),
            item: (*item).to_string(),
        }),
    }
}

#[cfg(test)]
mod tests;
