// Tests for the rate solver: net_yield correctness, convergence refusal,
// and availability filtering end to end. Split out of `solve.rs` to keep it
// under the file-length limit.
use super::*;
use crate::chain::{Availability, Rate};

/// Builds `Availability::Unlocked` from a slice of recipe names — a real
/// save's unlocked set naturally includes machine recipes, so a fixture
/// that only lists the item recipes under test will fail at machine
/// selection for an unrelated reason. Callers below include whichever
/// machine recipes their goal is expected to reach.
fn unlocked(names: &[&str]) -> Availability {
    Availability::Unlocked(names.iter().map(|s| s.to_string()).collect())
}

#[test]
fn net_yield_nets_production_against_consumption() {
    let kovarex = crate::recipe::get("kovarex-enrichment-process").expect("exists");
    assert!((net_yield(kovarex, "uranium-235") - 1.0).abs() < 1e-9);
    assert!((net_yield(kovarex, "uranium-238") - (-3.0)).abs() < 1e-9);
}

#[test]
fn net_yield_uses_probability_not_bare_amount() {
    let uranium_processing = crate::recipe::get("uranium-processing").expect("exists");
    // Both results declare amount 1; only probability carries the split.
    assert!((net_yield(uranium_processing, "uranium-235") - 0.007).abs() < 1e-9);
    assert!((net_yield(uranium_processing, "uranium-238") - 0.993).abs() < 1e-9);
}

/// A recipe whose own item nets to zero-or-negative must be rejected
/// before dividing, not iterated toward. Using kovarex directly for
/// U-238 demand (5 consumed, 2 produced, net -3) exercises this with
/// real registry data rather than a synthetic fixture.
#[test]
fn a_recipe_that_nets_negative_for_its_own_item_does_not_converge() {
    let goal = ChainGoal::new("uranium-238", Rate::ItemsPerSec(1.0), &["uranium-ore"])
        .with_override("uranium-238", "kovarex-enrichment-process");
    assert!(matches!(solve(&goal), Err(ChainError::DidNotConverge { .. })));
}

// ── Availability ──────────────────────────────────────────────────

#[test]
fn locked_intermediate_errors_and_is_not_billed_to_the_bus() {
    // THE regression guard. An intermediate whose only recipes are
    // locked must NOT be silently treated as a raw resource by this
    // file's empty-candidates shortcut. It must surface as
    // RecipeLocked. electronic-circuit itself resolves (unlocked), but
    // its copper-cable ingredient does not — so the error must name
    // copper-cable, not electronic-circuit, and must arrive before
    // machine selection ever runs (this fixture deliberately omits any
    // machine recipe).
    let goal = ChainGoal::new("electronic-circuit", Rate::ItemsPerSec(1.0), &["iron-plate"])
        .with_availability(unlocked(&["electronic-circuit"])); // copper-cable locked
    match solve(&goal) {
        Err(ChainError::RecipeLocked { item, .. }) => assert_eq!(item, "copper-cable"),
        Ok(plan) => panic!("copper-cable leaked into inputs: {:?}", plan.inputs),
        other => panic!("expected RecipeLocked, got {other:?}"),
    }
}

#[test]
fn genuinely_raw_items_still_resolve_as_inputs() {
    // The other side of the same coin: a restricted availability must
    // not turn a genuinely raw item into an error. uranium-ore has no
    // non-recycling producer (see `raw_resources_below_the_goal_become_inputs_not_errors`
    // above), so it stands in for "iron-ore" here — this install's
    // Space Age data gives iron-ore two asteroid-crushing producers
    // (see `a_raw_resource_goal_is_unreachable` in chain_ratios.rs), so
    // it is not actually raw and would wrongly turn this into a
    // RecipeLocked/AmbiguousRecipe test instead.
    let goal = ChainGoal::new("uranium-235", Rate::ItemsPerSec(1.0), &[])
        .with_override("uranium-235", "uranium-processing")
        .with_availability(unlocked(&["uranium-processing", "centrifuge"]));
    let plan = solve(&goal).expect("uranium-ore is raw, not locked");
    assert!(plan.inputs.iter().any(|i| i.item == "uranium-ore"));
}

#[test]
fn availability_resolves_an_otherwise_ambiguous_item() {
    // copper-cable has two producers; with only one unlocked the
    // AmbiguousRecipe error disappears without needing an override.
    // The fixture must also unlock a machine for "electronics" or this
    // fails at machine selection for an unrelated reason.
    let goal = ChainGoal::new("copper-cable", Rate::ItemsPerSec(1.0), &["copper-plate"])
        .with_availability(unlocked(&["copper-cable", "assembling-machine-1"]));
    assert!(solve(&goal).is_ok());
}

#[test]
fn an_explicit_override_to_a_locked_recipe_is_still_honoured() {
    // The machine must be unlocked or this fails for an unrelated
    // reason: with only "assembling-machine-2" unlocked, NOTHING else
    // is craftable, so a missing machine unlock would die at machine
    // selection rather than proving the recipe override path works.
    let goal = ChainGoal::new("copper-cable", Rate::ItemsPerSec(1.0), &["copper-plate"])
        .with_availability(unlocked(&["assembling-machine-2"])) // copper-cable itself locked
        .with_override("copper-cable", "copper-cable");
    assert!(solve(&goal).is_ok());
}

#[test]
fn unrestricted_availability_changes_nothing() {
    // The default must be inert: setting it explicitly yields the same
    // plan as not setting it. Every pre-existing chain and layout test
    // is the broader form of this assertion and must still pass
    // untouched.
    let base = ChainGoal::new(
        "electronic-circuit",
        Rate::ItemsPerSec(1.0),
        &["iron-plate", "copper-plate"],
    )
    .with_override("copper-cable", "copper-cable");
    let explicit = base.clone().with_availability(Availability::Unrestricted);
    let (a, b) = (solve(&base).unwrap(), solve(&explicit).unwrap());
    assert_eq!(a.steps.len(), b.steps.len());
    assert_eq!(a.inputs, b.inputs);
}
