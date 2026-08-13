// Shared test fixtures for the layout phase.
//
// Gated behind `cfg(any(test, feature = "testsupport"))` and exported rather
// than kept in an inline `#[cfg(test)]` module, because the same fixtures are
// used from inline unit tests *and* from `tests/layout_output.rs` — separate
// compilation units, which would otherwise each need their own copy. The
// crate dev-depends on itself with the feature on, so `cargo test` turns it
// on for both.
//
// Every plan here comes out of the real `chain::solve` against the real
// registries. A hand-written `ProductionPlan` would be quicker but would let
// the layout tests keep passing against ratios the calculator no longer
// produces.
use factorio_grid::prototype::{self, EntityPrototype};

use crate::chain::{self, ChainGoal, ItemRate, MachinePolicy, ProductionPlan, ProductionStep, Rate};
use crate::layout::LayoutConfig;

/// Blue belt: 45/s, so 22.5/s per lane. The tier the lane maths is pinned on.
pub fn blue_belt() -> &'static EntityPrototype {
    prototype::lookup("express-transport-belt").expect("express-transport-belt is in the registry")
}

/// Blue belts, medium poles, fast inserters — a mid-game block.
pub fn default_cfg() -> LayoutConfig {
    LayoutConfig::new("express-transport-belt", "medium-electric-pole", "fast-inserter")
}

pub fn rate(item: &str, per_sec: f64) -> ItemRate {
    ItemRate::new(item, per_sec)
}

/// The design's centerpiece: 45/s electronic circuits from plates on
/// assembling-machine-2 — 30 circuit machines fed by 45 cable machines, with
/// 135/s of copper cable moving between them.
///
/// The `copper-cable` override is required, not incidental: two recipes make
/// copper cable and the calculator refuses to guess.
pub fn green_circuit_plan() -> ProductionPlan {
    let goal = ChainGoal::new(
        "electronic-circuit",
        Rate::ItemsPerSec(45.0),
        &["iron-plate", "copper-plate"],
    )
    .with_machines(MachinePolicy::all("assembling-machine-2"))
    .with_override("copper-cable", "copper-cable");

    chain::solve(&goal).expect("the green-circuit plan resolves")
}

/// A plan whose `kovarex-enrichment-process` step consumes and produces
/// uranium-235 — the case the layout phase refuses.
pub fn plan_containing_kovarex() -> ProductionPlan {
    let goal = ChainGoal::new("uranium-235", Rate::ItemsPerSec(1.0), &["uranium-ore"])
        .with_override("uranium-235", "kovarex-enrichment-process")
        .with_override("uranium-238", "uranium-processing");

    chain::solve(&goal).expect("a self-consuming recipe still resolves as ratios")
}

/// A single-step `uranium-processing` plan — no `kovarex-enrichment-process`
/// step alongside it. Unlike `plan_containing_kovarex`, this is safe to run
/// all the way through `layout::generate`: `layout::build` runs
/// `reject_if_cyclic` over *every* step in a plan, and kovarex's own step
/// (consumes and produces uranium-235) fails that check and would refuse the
/// whole plan before `uranium-processing` was ever sized. Use this one for
/// anything exercising the two-product cell end to end; use
/// `plan_containing_kovarex` only for the sizing-level `uranium-processing`
/// tests that take a bare `&ProductionStep` and never call `generate`.
pub fn uranium_processing_plan() -> ProductionPlan {
    let goal = ChainGoal::new("uranium-238", Rate::ItemsPerSec(1.0), &["uranium-ore"])
        .with_override("uranium-238", "uranium-processing");

    chain::solve(&goal).expect("uranium-processing resolves against a flat ore bus")
}

/// Find a step by its recipe name, for tests that only care about one step
/// out of a multi-step plan.
pub fn step<'a>(plan: &'a ProductionPlan, recipe: &str) -> &'a ProductionStep {
    plan.steps.iter().find(|s| s.recipe.name == recipe).unwrap_or_else(|| {
        panic!(
            "no step for recipe `{recipe}` in this plan (have: {:?})",
            plan.steps.iter().map(|s| s.recipe.name.as_str()).collect::<Vec<_>>()
        )
    })
}

/// A hand-built step for exercising sizing/geometry the design's own
/// fixtures don't happen to hit (an absurd rate, an uneven split). `recipe`
/// is a stand-in looked up in the real registry so `step.recipe.ingredients`
/// still exists for the fluid check — only its name and category show up in
/// error messages, the `inputs`/`outputs` rates need not match it for real.
pub fn hand_step(
    recipe: &str,
    machines_needed: u32,
    inputs: Vec<ItemRate>,
    outputs: Vec<ItemRate>,
) -> ProductionStep {
    ProductionStep {
        recipe: crate::recipe::get(recipe).expect("recipe exists in the registry"),
        machine: prototype::lookup("assembling-machine-2").expect("assembling-machine-2 exists"),
        exact_count: machines_needed as f64,
        machines_needed,
        crafts_per_sec: 1.0,
        inputs,
        outputs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn green_circuit_plan_has_the_hand_checkable_ratio() {
        let plan = green_circuit_plan();
        let counts: Vec<u32> = plan.steps.iter().map(|s| s.machines_needed).collect();
        assert_eq!(plan.steps.len(), 2, "{plan:#?}");
        assert!(counts.contains(&30) && counts.contains(&45), "3:2 ratio, got {counts:?}");
    }

    #[test]
    fn kovarex_plan_actually_contains_kovarex() {
        let plan = plan_containing_kovarex();
        assert!(plan.steps.iter().any(|s| s.recipe.name == "kovarex-enrichment-process"));
    }

    /// The property `uranium_processing_plan`'s own doc comment depends on:
    /// exactly one step, and it is not kovarex, so `reject_if_cyclic` never
    /// fires and `generate` can run this plan all the way through.
    #[test]
    fn uranium_processing_plan_has_exactly_one_step() {
        let plan = uranium_processing_plan();
        assert_eq!(plan.steps.len(), 1, "{plan:#?}");
        assert_eq!(plan.steps[0].recipe.name, "uranium-processing");
    }

    #[test]
    fn blue_belt_is_the_45_per_second_tier() {
        assert_eq!(blue_belt().belt_throughput, Some(45.0));
    }
}
