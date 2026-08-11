// Hand-checkable production ratios: the numbers a player could verify with a calculator.
//
// These exercise `chain::solve` against the live dump-derived registries, not
// fixtures, so a data update that changes a ratio (a recipe's
// `energy_required`, a machine's `crafting_speed`) fails a test here instead
// of silently shipping a wrong blueprint.

use factorio_solver::chain::{
    ChainError, ChainGoal, ItemRate, MachinePolicy, ProductionPlan, ProductionStep, Rate, solve,
};

/// Finds the one step for `recipe_name`, panicking with the full plan on
/// miss so a failing assertion shows what *did* get built.
fn step<'a>(plan: &'a ProductionPlan, recipe_name: &str) -> &'a ProductionStep {
    plan.steps
        .iter()
        .find(|s| s.recipe.name == recipe_name)
        .unwrap_or_else(|| panic!("no '{recipe_name}' step in {plan:#?}"))
}

fn input<'a>(plan: &'a ProductionPlan, item: &str) -> &'a ItemRate {
    plan.inputs
        .iter()
        .find(|i| i.item == item)
        .unwrap_or_else(|| panic!("no '{item}' input in {plan:#?}"))
}

/// THE canonical hand-checkable ratio.
/// electronic-circuit and copper-cable both have energy_required 0.5.
/// assembling-machine-2 crafting_speed 0.75 -> 1.5 crafts/s per machine.
/// circuits: 45/s / 1 per craft = 45 crafts/s -> 45/1.5 = 30 machines
/// cable demand: 45 crafts/s * 3 = 135/s; cable yields 2/craft -> 67.5 crafts/s
///               -> 67.5/1.5 = 45 machines
#[test]
fn green_circuits_from_plates_is_30_and_45() {
    let goal = ChainGoal::new(
        "electronic-circuit",
        Rate::ItemsPerSec(45.0),
        &["iron-plate", "copper-plate"],
    )
    .with_machines(MachinePolicy::all("assembling-machine-2"))
    .with_override("copper-cable", "copper-cable"); // REQUIRED: 2 candidates

    let plan = solve(&goal).expect("green circuits from raw plates must resolve");

    let circuits = step(&plan, "electronic-circuit");
    assert_eq!(circuits.machines_needed, 30);
    assert!((circuits.exact_count - 30.0).abs() < 1e-9);
    assert!((circuits.crafts_per_sec - 45.0).abs() < 1e-9);

    let cable = step(&plan, "copper-cable");
    assert_eq!(cable.machines_needed, 45);
    assert!((cable.exact_count - 45.0).abs() < 1e-9);
    assert!((cable.crafts_per_sec - 67.5).abs() < 1e-9);

    assert!((input(&plan, "iron-plate").per_sec - 45.0).abs() < 1e-9);
    assert!((input(&plan, "copper-plate").per_sec - 67.5).abs() < 1e-9);
}

/// Same goal, narrower boundary -> one step instead of two, same code path:
/// the bus check happens before recipe selection, so an item's ambiguity
/// never matters once it's declared available.
#[test]
fn declaring_cable_available_removes_its_step() {
    let goal = ChainGoal::new(
        "electronic-circuit",
        Rate::ItemsPerSec(45.0),
        &["iron-plate", "copper-cable"],
    );

    let plan = solve(&goal).expect("copper-cable on the bus needs no override");

    assert_eq!(plan.steps.len(), 1, "cable's own step should vanish: {plan:#?}");
    assert!((input(&plan, "copper-cable").per_sec - 135.0).abs() < 1e-9);
}

#[test]
fn steps_are_topologically_ordered() {
    let goal = ChainGoal::new(
        "electronic-circuit",
        Rate::ItemsPerSec(45.0),
        &["iron-plate", "copper-plate"],
    )
    .with_machines(MachinePolicy::all("assembling-machine-2"))
    .with_override("copper-cable", "copper-cable");

    let plan = solve(&goal).expect("resolves");

    let cable_index = plan.steps.iter().position(|s| s.recipe.name == "copper-cable").unwrap();
    let circuit_index =
        plan.steps.iter().position(|s| s.recipe.name == "electronic-circuit").unwrap();
    assert!(cable_index < circuit_index, "producer must precede consumer: {plan:#?}");
}

/// Multi-output. CRITICAL: both results declare amount 1; the split is
/// entirely in `probability`. Using `amount` alone gives a silent 1:1.
#[test]
fn uranium_processing_uses_probability_weighted_yield() {
    let goal = ChainGoal::new("uranium-235", Rate::ItemsPerSec(1.0), &["uranium-ore"])
        .with_override("uranium-235", "uranium-processing"); // REQUIRED: kovarex also produces it

    let plan = solve(&goal).expect("resolves");

    let processing = step(&plan, "uranium-processing");
    assert!((processing.crafts_per_sec - 1.0 / 0.007).abs() < 1e-6); // ~142.86, NOT 1

    let u238 = plan
        .byproducts
        .iter()
        .find(|b| b.item == "uranium-238")
        .unwrap_or_else(|| panic!("expected a uranium-238 byproduct in {plan:#?}"));
    assert!((u238.per_sec - 0.993 / 0.007).abs() < 1e-6); // ~141.86

    // centrifuge: crafting_speed 1.0, energy_required 12 -> 1/12 crafts/s/machine.
    assert!((processing.exact_count - (1.0 / 0.007) * 12.0).abs() < 1e-3); // ~1714.3
}

/// Cyclic. Must terminate; net +1 U-235 and -3 U-238 per craft of kovarex.
///
/// This test uses the suggested demand-propagation-with-netting method's own
/// numbers (see `crates/solver/src/chain/solve.rs`): kovarex is popped first
/// (from the goal), so it always gets exactly 1.0 crafts/s (net yield 1 per
/// craft for a demand of 1.0/s); uranium-processing only picks up the
/// remainder of kovarex's U-238 appetite that kovarex's own output doesn't
/// cover. A method that solved the coupled system exactly instead would
/// land on different (still self-consistent) numbers.
#[test]
fn kovarex_cycle_terminates_and_balances() {
    let goal = ChainGoal::new("uranium-235", Rate::ItemsPerSec(1.0), &["uranium-ore"])
        .with_override("uranium-235", "kovarex-enrichment-process")
        .with_override("uranium-238", "uranium-processing");

    let plan = solve(&goal).expect("a self-consuming recipe must terminate, not hang");

    let kovarex = step(&plan, "kovarex-enrichment-process");
    assert!((kovarex.crafts_per_sec - 1.0).abs() < 1e-9);

    let processing = step(&plan, "uranium-processing");
    let expected_processing = 3.0 / 0.993; // kovarex needs 5 U-238/craft, makes 2 itself
    assert!((processing.crafts_per_sec - expected_processing).abs() < 1e-6);

    let expected_ore = expected_processing * 10.0;
    assert!((input(&plan, "uranium-ore").per_sec - expected_ore).abs() < 1e-6);

    // uranium-processing's own U-235 byproduct (0.007/craft) is never
    // consumed by anything else in this plan.
    let expected_u235_byproduct = expected_processing * 0.007;
    let u235 = plan
        .byproducts
        .iter()
        .find(|b| b.item == "uranium-235")
        .unwrap_or_else(|| panic!("expected a small uranium-235 byproduct in {plan:#?}"));
    assert!((u235.per_sec - expected_u235_byproduct).abs() < 1e-6);
}

#[test]
fn fluid_ingredient_is_rejected_with_named_error() {
    let goal = ChainGoal::new("plastic-bar", Rate::ItemsPerSec(1.0), &["coal"])
        .with_override("plastic-bar", "plastic-bar"); // REQUIRED: bioplastic also makes it

    match solve(&goal) {
        Err(ChainError::FluidIngredient { recipe, fluid }) => {
            assert_eq!(recipe, "plastic-bar");
            assert_eq!(fluid, "petroleum-gas");
        }
        other => panic!("expected FluidIngredient, got {other:?}"),
    }
}

#[test]
fn declaring_the_fluid_product_available_succeeds() {
    // advanced-circuit needs electronic-circuit, plastic-bar and
    // copper-cable directly, none of them fluids — the fluid two steps
    // upstream (plastic-bar's own petroleum-gas) never gets resolved
    // because plastic-bar itself is on the bus. advanced-circuit is
    // unambiguous; no override needed.
    let goal = ChainGoal::new(
        "advanced-circuit",
        Rate::ItemsPerSec(1.0),
        &["plastic-bar", "electronic-circuit", "copper-cable"],
    );
    solve(&goal).expect("no fluid should ever need resolving here");
}

#[test]
fn ambiguity_surfaces_as_an_error() {
    let goal = ChainGoal::new(
        "electronic-circuit",
        Rate::ItemsPerSec(45.0),
        &["iron-plate", "copper-plate"],
    ); // no copper-cable override

    match solve(&goal) {
        Err(ChainError::AmbiguousRecipe { item, candidates }) => {
            assert_eq!(item, "copper-cable");
            assert!(candidates.len() > 1, "{candidates:?}");
        }
        other => panic!("expected AmbiguousRecipe, got {other:?}"),
    }
}

#[test]
fn byproducts_are_reported_and_warned_about() {
    let goal = ChainGoal::new("uranium-235", Rate::ItemsPerSec(1.0), &["uranium-ore"])
        .with_override("uranium-235", "uranium-processing");

    let plan = solve(&goal).expect("resolves");

    assert!(plan.byproducts.iter().any(|b| b.item == "uranium-238"));
    assert!(
        plan.warnings.iter().any(|w| w.contains("uranium-238")),
        "byproduct must be named in a warning: {:?}",
        plan.warnings
    );
}

#[test]
fn goal_already_on_the_bus_builds_nothing() {
    let goal = ChainGoal::new("iron-plate", Rate::ItemsPerSec(10.0), &["iron-plate"]);
    let plan = solve(&goal).expect("an available goal is a legitimate, error-free answer");
    assert!(plan.steps.is_empty());
    assert_eq!(plan.inputs, vec![ItemRate::new("iron-plate", 10.0)]);
}

#[test]
fn a_raw_resource_goal_is_unreachable() {
    // NOTE: the plan that specified this test used "iron-ore" as the
    // example, but under this install's Space Age data iron-ore has two
    // producing recipes (metallic-asteroid-crushing and its "advanced"
    // variant), so it is ambiguous rather than unreachable. "wood" has no
    // producing recipe at all in the live registry and stands in for it.
    let goal = ChainGoal::new("wood", Rate::ItemsPerSec(1.0), &[]);
    match solve(&goal) {
        Err(ChainError::UnreachableBoundary { item }) => assert_eq!(item, "wood"),
        other => panic!("expected UnreachableBoundary, got {other:?}"),
    }
}

#[test]
fn raw_resources_below_the_goal_become_inputs_not_errors() {
    // available is empty: uranium-ore is still reached, but since it has no
    // recipe of its own it becomes a plan input instead of erroring, unlike
    // the goal-level check in `a_raw_resource_goal_is_unreachable`.
    let goal = ChainGoal::new("uranium-235", Rate::ItemsPerSec(1.0), &[])
        .with_override("uranium-235", "uranium-processing");
    let plan = solve(&goal).expect("a raw resource short of the goal is an input, not an error");
    assert!(plan.inputs.iter().any(|i| i.item == "uranium-ore"));
}

#[test]
fn rate_units_agree() {
    let base = |rate: Rate| {
        ChainGoal::new("electronic-circuit", rate, &["iron-plate", "copper-plate"])
            .with_machines(MachinePolicy::all("assembling-machine-2"))
            .with_override("copper-cable", "copper-cable")
    };

    let by_sec = solve(&base(Rate::ItemsPerSec(45.0))).expect("resolves");
    let by_min = solve(&base(Rate::ItemsPerMin(2700.0))).expect("resolves");

    let circuits_sec = step(&by_sec, "electronic-circuit");
    let circuits_min = step(&by_min, "electronic-circuit");
    assert_eq!(circuits_sec.machines_needed, circuits_min.machines_needed);
    assert!((circuits_sec.exact_count - circuits_min.exact_count).abs() < 1e-9);

    let cable_sec = step(&by_sec, "copper-cable");
    let cable_min = step(&by_min, "copper-cable");
    assert_eq!(cable_sec.machines_needed, cable_min.machines_needed);
    assert!((cable_sec.exact_count - cable_min.exact_count).abs() < 1e-9);
}
