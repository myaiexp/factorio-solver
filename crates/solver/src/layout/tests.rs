// Tests for `LayoutConfig`/`generate` itself: config resolution, the two
// named refusals `build` checks before placing anything, and end-to-end
// shape assertions on a real generated block. Geometry-level tests for the
// pieces `generate` composes live beside those pieces instead (`cell/tests.rs`,
// `place/tests.rs`, `power/tests.rs`, `validate/tests.rs`).
use super::*;
use crate::chain::{self, ChainGoal, MachinePolicy, Rate};
use crate::testsupport::{default_cfg, green_circuit_plan, plan_containing_kovarex, uranium_processing_plan};

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
/// items from one ore; only *self*-consumption has no topology. (It can
/// still be refused by `generate` — a cell column owns its product lanes
/// outright, so two results need a topology with at least two belts on the
/// product side, and the default topology's product side has only one — but
/// that is `TooManyProductsForBelts`, a different check for a different
/// reason.) It sits in the same plan as the kovarex step above, so a check
/// that confused the two would reject both.
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

    // A real inserter is still not a *long* inserter: reach is checked, so
    // a one-tile arm named here cannot silently be placed beside a belt
    // pair it can never touch.
    let cfg = default_cfg().with_long_inserter("fast-inserter");
    assert!(matches!(generate(&plan, &cfg), Err(LayoutError::LongInserterUnknown(_))));
}

#[test]
fn reach_comes_from_the_prototype_not_the_name() {
    let long = prototype::lookup("long-handed-inserter").unwrap();
    assert_eq!(reach(long), 2.0);
    assert_eq!(reach(prototype::lookup("fast-inserter").unwrap()), 1.0);
    assert_eq!(reach(prototype::lookup("wooden-chest").unwrap()), 0.0);
}

#[test]
fn the_default_config_resolves() {
    LayoutConfig::default().resolve().expect("the default names must be real entities");
}

#[test]
fn a_plan_with_nothing_to_build_is_an_error_not_an_empty_blueprint() {
    let plan = ProductionPlan { steps: vec![], inputs: vec![], byproducts: vec![], warnings: vec![] };
    assert!(matches!(generate(&plan, &default_cfg()), Err(LayoutError::EmptyPlan)));
}

#[test]
fn the_green_circuit_block_generates_and_is_powered() {
    let grid = generate(&green_circuit_plan(), &default_cfg()).unwrap();
    assert_eq!(
        grid.entities().filter(|e| e.prototype_name == "assembling-machine-2").count(),
        75 // 30 circuit machines + 45 cable machines
    );
    assert!(coverage_gaps(&grid).is_empty());
}

/// The same plan with and without a width cap. Capped must be narrower
/// (bounding box) and taller; machine count identical. The cap is two
/// cells' worth of the real geometry — `cell_width` computed the same
/// way `tile::place_step` computes it — rather than a guessed constant.
#[test]
fn target_width_wraps_cells_into_bands() {
    let plan = green_circuit_plan();
    let cfg = default_cfg();
    let machine = prototype::lookup("assembling-machine-2").unwrap();

    let unwrapped = generate(&plan, &cfg).unwrap();
    let (umin, umax) = unwrapped.bounding_box().expect("a generated block is never empty");
    let (uw, uh) = (umax.x - umin.x + 1, umax.y - umin.y + 1);

    let one_cell = cell_width(machine, &cfg.topology, true);
    let two_cells = one_cell + cell_width(machine, &cfg.topology, false);
    let capped_cfg =
        cfg.clone().with_topology(CellTopology { target_width: Some(two_cells), ..cfg.topology });

    let wrapped = generate(&plan, &capped_cfg).unwrap();
    let (wmin, wmax) = wrapped.bounding_box().expect("a generated block is never empty");
    let (ww, wh) = (wmax.x - wmin.x + 1, wmax.y - wmin.y + 1);

    assert!(ww < uw, "a capped band must be narrower: capped={ww} unwrapped={uw}");
    assert!(wh > uh, "wrapping must add height: capped={wh} unwrapped={uh}");

    let machines =
        |g: &Grid| g.entities().filter(|e| e.prototype_name == "assembling-machine-2").count();
    assert_eq!(machines(&wrapped), machines(&unwrapped), "wrapping must not drop machines");
}

/// Unchanged behaviour through the rewrite: the kovarex plan still errors
/// `CyclicStep`, and a plan with a fluid on a belt still errors
/// `FluidOnBelt`. See `cell::tests::a_fluid_is_refused_rather_than_sized`
/// for the fluid goal this reuses, which reaches the layout because
/// `chain::solve` only rejects a fluid *ingredient* off the bus.
#[test]
fn cyclic_and_fluid_refusals_still_fire() {
    let cyclic = plan_containing_kovarex();
    assert!(matches!(generate(&cyclic, &default_cfg()), Err(LayoutError::CyclicStep { .. })));

    let goal = ChainGoal::new("heavy-oil", Rate::ItemsPerSec(1.0), &["crude-oil", "water"])
        .with_machines(MachinePolicy::all("oil-refinery"))
        .with_override("heavy-oil", "advanced-oil-processing");
    let fluid_plan = chain::solve(&goal).expect("the calculator is happy: every fluid is on the bus");
    assert!(matches!(generate(&fluid_plan, &default_cfg()), Err(LayoutError::FluidOnBelt { .. })));
}

/// A cell column owns its product lanes outright, so a recipe with two
/// results — like `uranium-processing` — needs a product side wide enough
/// to give each one a whole belt. The default topology's product side (the
/// 1-belt edge) isn't, so it still refuses — by belt count now, not by
/// product count outright, which is what a topology with a wider product
/// side (`a_uranium_processing_plan_generates_and_validates`, below) exists
/// to prove.
#[test]
fn a_multi_product_step_refuses_through_generate() {
    let plan = uranium_processing_plan();
    match generate(&plan, &default_cfg()) {
        Err(LayoutError::TooManyProductsForBelts { recipe, products, belts }) => {
            assert_eq!(recipe, "uranium-processing");
            assert_eq!(products.len(), 2);
            assert_eq!(belts, 1, "the default topology's product side is the 1-belt edge");
        }
        other => panic!("expected TooManyProductsForBelts, got {other:?}"),
    }
}

/// End to end: `uranium-processing`'s two products need a topology with at
/// least two belts on the product side, so this flips products onto the
/// 2-belt spine (same flip the `cell`/`place` unit tests use). Exercises
/// the whole pipeline — `build`, then `validate`, including
/// `check_no_mixed_product_belts` and the per-product delivered-rate check
/// — on a real centrifuge, now that `validate::is_machine` recognises one
/// (it didn't before this feature: `EntityCategory`'s name-substring match
/// has no rule for "centrifuge", so a centrifuge-based block used to pass
/// validation without a single assertion running).
#[test]
fn a_uranium_processing_plan_generates_and_validates() {
    let plan = uranium_processing_plan();
    let cfg = default_cfg()
        .with_topology(CellTopology { ingredients_on: Side::Edge, ..CellTopology::default() });

    let (grid, report) = generate_with_report(&plan, &cfg).expect("uranium-processing should lay out");
    assert!(
        grid.entities().any(|e| e.prototype_name == "centrifuge"),
        "the plan should have selected a centrifuge, uranium-processing's only crafting machine"
    );
    assert!(coverage_gaps(&grid).is_empty());
    assert!(report.bindings.iter().any(|(recipe, _)| recipe == "uranium-processing"));
}
