// Block generator: a ProductionPlan in, a Grid of real entities out.
//
// Geometry is computed, never searched — there is no A* here. That is what
// lets the generator ship before a belt router exists, and it is why it
// cannot emit the subtly-broken blueprints a half-working router would.
//
// Layout knows nothing about recipes beyond what a step carries; the chain
// calculator knows nothing about geometry. `ProductionPlan` is the whole
// interface between them.
use factorio_grid::prototype::{self, EntityPrototype};
use factorio_grid::{Grid, GridPos};

use crate::chain::{ProductionPlan, ProductionStep};

pub mod error;
pub mod lanes;
pub mod power;
pub mod rows;
pub mod validate;

pub use error::LayoutError;
pub use lanes::{lane_throughput, lanes_needed, pack_lanes, BeltAssignment};
pub use power::{coverage_gaps, place_poles};
pub use rows::{place_step, StepExtent};
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
}

impl LayoutConfig {
    pub fn new(belt_tier: &str, pole: &str, inserter: &str) -> Self {
        Self {
            belt_tier: belt_tier.to_string(),
            pole: pole.to_string(),
            inserter: inserter.to_string(),
        }
    }

    /// Resolve all three names against the prototype registry up front, so a
    /// typo fails before any entity is placed rather than half way through a
    /// grid.
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
        Ok(ResolvedConfig { belt, pole, inserter })
    }
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
}

/// Turn a plan into a placed grid, ready for `factorio_grid::to_blueprint`.
///
/// Discards the validation report. Use `generate_with_report` where the
/// warnings matter — they name real problems with a block that still emits,
/// like a belt segment over its rating.
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
        let extent = rows::place_step(&mut grid, step, &resolved, GridPos { x: 0, y })?;
        y += extent.height as i32 + STEP_GAP;
    }

    // Poles last: they fill the gaps the machine rows left, so they need the
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
mod tests {
    use super::*;
    use crate::testsupport::{default_cfg, green_circuit_plan, plan_containing_kovarex};

    #[test]
    fn cyclic_step_is_rejected_by_name() {
        let plan = plan_containing_kovarex();
        match generate(&plan, &default_cfg()) {
            Err(LayoutError::CyclicStep { recipe, item }) => {
                assert_eq!(recipe, "kovarex-enrichment-process");
                assert_eq!(item, "uranium-235");
            }
            other => panic!("expected CyclicStep, got {other:?}"),
        }
    }

    #[test]
    fn acyclic_plan_is_accepted() {
        let plan = green_circuit_plan();
        assert!(generate(&plan, &default_cfg()).is_ok(), "green circuits have no self-loop");
    }

    /// Multi-output is not cyclic. `uranium-processing` yields two different
    /// items from one ore and lays out fine; only *self*-consumption has no
    /// topology. It sits in the same plan as the kovarex step above, so a
    /// check that confused the two would reject both.
    #[test]
    fn a_multi_output_step_is_not_mistaken_for_a_cycle() {
        let plan = plan_containing_kovarex();
        let processing = plan
            .steps
            .iter()
            .find(|s| s.recipe.name == "uranium-processing")
            .expect("uranium-processing is in the plan");
        assert_eq!(processing.recipe.results.len(), 2, "it really is multi-output");
        assert!(reject_if_cyclic(processing).is_ok());

        let kovarex = plan
            .steps
            .iter()
            .find(|s| s.recipe.name == "kovarex-enrichment-process")
            .expect("kovarex is in the plan");
        assert!(reject_if_cyclic(kovarex).is_err());
    }

    #[test]
    fn config_typos_fail_before_anything_is_placed() {
        let plan = green_circuit_plan();

        let cfg = LayoutConfig::new("not-a-belt", "medium-electric-pole", "fast-inserter");
        assert!(matches!(generate(&plan, &cfg), Err(LayoutError::BeltTierUnknown(_))));

        // Real entities that are not the thing asked for are rejected too:
        // being in the registry is not enough.
        let cfg = LayoutConfig::new("express-transport-belt", "beacon", "fast-inserter");
        assert!(matches!(generate(&plan, &cfg), Err(LayoutError::PoleUnknown(_))));

        let cfg = LayoutConfig::new("express-transport-belt", "medium-electric-pole", "wooden-chest");
        assert!(matches!(generate(&plan, &cfg), Err(LayoutError::InserterUnknown(_))));
    }

    #[test]
    fn the_default_config_resolves() {
        LayoutConfig::default().resolve().expect("the default names must be real entities");
    }

    #[test]
    fn a_plan_with_nothing_to_build_is_an_error_not_an_empty_blueprint() {
        let plan = ProductionPlan {
            steps: vec![],
            inputs: vec![],
            byproducts: vec![],
            warnings: vec![],
        };
        assert!(matches!(generate(&plan, &default_cfg()), Err(LayoutError::EmptyPlan)));
    }
}
