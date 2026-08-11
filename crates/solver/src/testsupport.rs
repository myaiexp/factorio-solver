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

use crate::chain::{self, ChainGoal, ItemRate, MachinePolicy, ProductionPlan, Rate};
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

    #[test]
    fn blue_belt_is_the_45_per_second_tier() {
        assert_eq!(blue_belt().belt_throughput, Some(45.0));
    }
}
