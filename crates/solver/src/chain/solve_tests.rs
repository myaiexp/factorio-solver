// Tests for the rate solver, including the availability gate's raw-resource split.
use std::collections::BTreeSet;

use super::*;
use crate::availability::{Availability, all_available_recipe_names};
use crate::chain::{MachinePolicy, Rate};

fn unlocked(names: &[&str]) -> Availability {
    Availability::Unlocked(names.iter().map(|n| n.to_string()).collect())
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

// ── Availability gating ─────────────────────────────────────────────────

/// `all_available_recipe_names()` with every casting-* recipe removed —
/// what the UI panel would produce for "I haven't unlocked Vulcanus yet".
/// Exactly nine names drop out, verified against the real 2.0.77 data.
fn casting_locked_names() -> BTreeSet<String> {
    all_available_recipe_names().into_iter().filter(|name| !name.contains("casting")).collect()
}

fn casting_locked_availability() -> Availability {
    Availability::Unlocked(casting_locked_names())
}

/// The same set, with `electromagnetic-plant` also removed — locking the
/// machine itself rather than a recipe category.
fn casting_and_electromagnetic_plant_locked_availability() -> Availability {
    let mut names = casting_locked_names();
    names.remove("electromagnetic-plant");
    Availability::Unlocked(names)
}

/// The test this whole split exists for: a locked intermediate must error,
/// never quietly become a bus input. `electronic-circuit` itself resolves
/// (its one recipe is in the unlocked set), but its `copper-cable`
/// ingredient has two candidates — `copper-cable` and
/// `casting-copper-cable` — both `enabled: false` and neither unlocked.
#[test]
fn a_locked_intermediate_errors_instead_of_becoming_a_bus_input() {
    let goal = ChainGoal::new(
        "electronic-circuit",
        Rate::ItemsPerSec(1.0),
        &["iron-plate", "copper-plate"],
    )
    .with_availability(unlocked(&["electronic-circuit"]));

    match solve(&goal) {
        Err(ChainError::RecipeLocked { item, unlocked_by, .. }) => {
            assert_eq!(item, "copper-cable");
            assert_eq!(unlocked_by, vec!["electronics".to_string(), "foundry".to_string()]);
        }
        Ok(plan) => panic!(
            "a locked intermediate must never resolve into a plan — this is the bug the whole \
             availability gate exists to catch, and it just got Ok with copper-cable presumably \
             listed under inputs: {plan:#?}"
        ),
        other => panic!("expected RecipeLocked, got {other:?}"),
    }
}

/// `wood` has no producing recipe at all (not even a hidden/recycling one),
/// so even the most restrictive availability must still fold it into the
/// bus rather than erroring — it was never a candidate for gating to reject.
#[test]
fn a_genuinely_raw_item_still_folds_into_the_bus() {
    let goal = ChainGoal::new("wooden-chest", Rate::ItemsPerSec(1.0), &[]).with_availability(unlocked(&[]));

    let plan = solve(&goal).expect("wooden-chest must resolve even under the emptiest availability");
    assert!(
        plan.inputs.iter().any(|i| i.item == "wood"),
        "wood has no recipe at all, so it must fold into inputs: {plan:#?}"
    );
}

/// The goal itself, locked. Its only candidate (the `foundry` recipe) is
/// unlocked by exactly the technology of the same name.
#[test]
fn the_locked_goal_names_its_technology() {
    let goal = ChainGoal::new("foundry", Rate::ItemsPerSec(1.0), &[]).with_availability(unlocked(&[]));
    match solve(&goal) {
        Err(ChainError::RecipeLocked { item, unlocked_by, .. }) => {
            assert_eq!(item, "foundry");
            assert_eq!(unlocked_by, vec!["foundry".to_string()]);
        }
        other => panic!("expected RecipeLocked, got {other:?}"),
    }
}

/// Nothing produces `wood` at all, so the ungated-empty case at the goal
/// check keeps its existing `UnreachableBoundary` — even under `Everything`.
#[test]
fn an_item_nothing_ever_produces_is_still_unreachable_boundary() {
    let goal = ChainGoal::new("wood", Rate::ItemsPerSec(1.0), &[]);
    match solve(&goal) {
        Err(ChainError::UnreachableBoundary { item }) => assert_eq!(item, "wood"),
        other => panic!("expected UnreachableBoundary, got {other:?}"),
    }
}

/// The headline case: locking just the nine casting-* recipes resolves the
/// four-way ambiguity a mid-game player hits (`iron-plate`, `copper-cable`,
/// `iron-gear-wheel`, `pipe`) down to exactly one candidate apiece — the
/// assembler/smelter recipe of the same name, never the casting one.
#[test]
fn locking_the_casting_recipes_removes_the_ambiguity() {
    let availability = casting_locked_availability();
    for item in ["iron-plate", "copper-cable", "iron-gear-wheel", "pipe"] {
        let available = available_candidates_for(item, &availability);
        let names: Vec<&str> = available.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec![item], "{item}: expected exactly its own recipe, got {names:?}");
    }
}

/// The plan this whole feature is for: with casting locked and no
/// `recipe_overrides` at all, chemical science solves — today it needs three
/// hand-entered overrides (`copper-cable`, `iron-gear-wheel`, `pipe`; see
/// `crates/ui/src/chain_panel/scroll_tests.rs`), and removing them is the
/// point.
#[test]
fn the_chemical_science_plan_solves_with_no_overrides() {
    let goal = ChainGoal::new(
        "chemical-science-pack",
        Rate::Belts { count: 1, tier: "transport-belt".to_string() },
        &["sulfur", "steel-plate", "plastic-bar", "copper-plate", "iron-plate"],
    )
    .with_availability(casting_locked_availability());

    let plan =
        solve(&goal).expect("chemical science must solve unaided once the casting recipes are locked");

    let mut names: Vec<&str> = plan.steps.iter().map(|s| s.recipe.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "advanced-circuit",
            "chemical-science-pack",
            "copper-cable",
            "electronic-circuit",
            "engine-unit",
            "iron-gear-wheel",
            "pipe",
        ]
    );
}

/// Same plan, with `electromagnetic-plant` also locked: `electronics` is
/// also covered by `assembling-machine-1/2/3`, so machine selection falls
/// back to `assembling-machine-3` rather than erroring.
#[test]
fn fastest_available_skips_locked_machines() {
    let goal = ChainGoal::new(
        "chemical-science-pack",
        Rate::Belts { count: 1, tier: "transport-belt".to_string() },
        &["sulfur", "steel-plate", "plastic-bar", "copper-plate", "iron-plate"],
    )
    .with_availability(casting_and_electromagnetic_plant_locked_availability());

    let plan = solve(&goal).expect("must still solve by falling back to assembling-machine-3");

    let used: Vec<(&str, &str)> =
        plan.steps.iter().map(|s| (s.recipe.name.as_str(), s.machine.name.as_str())).collect();
    assert!(
        used.iter().all(|&(_, m)| m != "electromagnetic-plant"),
        "electromagnetic-plant is locked in this availability: {used:?}"
    );
    assert!(
        used.iter().any(|&(_, m)| m == "assembling-machine-3"),
        "expected electronics to fall back to assembling-machine-3: {used:?}"
    );
}

/// A named machine that is locked must error, never silently fall through to
/// something else — "build it all in X" is a claim about the whole plan.
#[test]
fn a_named_locked_machine_errors_rather_than_substituting() {
    let recipe = crate::recipe::get("electronic-circuit").expect("electronic-circuit exists");
    match select_machine(recipe, &MachinePolicy::all("electromagnetic-plant"), &unlocked(&[])) {
        Err(ChainError::MachineLocked { machine, unlocked_by, .. }) => {
            assert_eq!(machine, "electromagnetic-plant");
            assert_eq!(unlocked_by, vec!["electromagnetic-plant".to_string()]);
        }
        other => panic!("expected MachineLocked, not a silent substitution: {other:?}"),
    }
}

// The no-available-machine-for-a-category case (`metallurgy`, covered only
// by the locked `foundry`) lives in `select_machine_tests.rs` as
// `every_machine_for_a_category_locked_names_the_fastest_one` — it exercises
// `select_machine` directly and belongs beside the rest of that function's
// coverage, not duplicated here.

/// The existing `named_machine_that_cannot_craft_the_category_errors` case,
/// but under a restrictive availability, proving the category check still
/// runs first: a machine that cannot craft the category is fatal regardless
/// of research.
#[test]
fn a_named_machine_that_cannot_craft_the_category_still_errors_on_the_category() {
    let recipe = crate::recipe::get("electronic-circuit").expect("electronic-circuit exists");
    match select_machine(recipe, &MachinePolicy::all("stone-furnace"), &unlocked(&[])) {
        Err(ChainError::NoMachineForCategory { category, recipe: recipe_name }) => {
            assert_eq!(category, "electronics");
            assert_eq!(recipe_name, "electronic-circuit");
        }
        other => panic!("expected NoMachineForCategory even under a restrictive availability: {other:?}"),
    }
}

/// The default is unchanged: `Everything` still allows a research-locked
/// machine, so `electromagnetic-plant` keeps winning `electronics` for every
/// existing caller that never opts into a narrower availability.
#[test]
fn everything_still_allows_a_research_locked_machine() {
    let recipe = crate::recipe::get("electronic-circuit").expect("electronic-circuit exists");
    let machine = select_machine(recipe, &MachinePolicy::fastest(), &Availability::Everything)
        .expect("selects a machine");
    assert_eq!(machine.name, "electromagnetic-plant");
}

/// An override is an explicit instruction, so it survives a fully-locked
/// candidate set: `select_recipe` honours it without consulting availability
/// at all, and `solve`'s own locked-item checks defer to it via
/// `locked_without_an_override` rather than refusing first. Those two used to
/// disagree — `solve` refused before `select_recipe` was ever reached, so an
/// override that worked when called directly failed inside a plan.
#[test]
fn an_explicit_override_to_a_locked_recipe_is_still_honoured() {
    // The machine must be unlocked or this fails for an unrelated reason:
    // with only "assembling-machine-2" unlocked, nothing else is craftable,
    // so a missing machine unlock would die at machine selection rather than
    // proving the recipe override path works.
    let goal = ChainGoal::new("copper-cable", Rate::ItemsPerSec(1.0), &["copper-plate"])
        .with_availability(unlocked(&["assembling-machine-2"])) // copper-cable itself locked
        .with_override("copper-cable", "copper-cable");
    assert!(solve(&goal).is_ok());
}

/// The default must be inert: setting `Everything` explicitly yields the
/// same plan as not setting it at all. Every pre-existing chain and layout
/// test is the broader form of this assertion and must still pass untouched.
#[test]
fn explicitly_setting_everything_changes_nothing() {
    let base = ChainGoal::new(
        "electronic-circuit",
        Rate::ItemsPerSec(1.0),
        &["iron-plate", "copper-plate"],
    )
    .with_override("copper-cable", "copper-cable");
    let explicit = base.clone().with_availability(Availability::Everything);
    let (a, b) = (solve(&base).unwrap(), solve(&explicit).unwrap());
    assert_eq!(a.steps.len(), b.steps.len());
    assert_eq!(a.inputs, b.inputs);
}
