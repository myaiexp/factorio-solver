// Sizing tests for `size_step`: the worked examples from the design, every
// refusal, and the lane-allocation search itself. No `Grid` involved — that
// is a later task's job, and these tests would keep passing unchanged once
// it exists.
use super::*;
use factorio_grid::prototype;

use crate::chain::{self, ChainGoal, MachinePolicy, Rate};
use crate::testsupport::{blue_belt, green_circuit_plan, hand_step, plan_containing_kovarex, rate, step};

/// The design's worked table, asserted exactly. Both binding cases appear:
/// the circuit step is input-bound on cable, the cable step output-bound on
/// its own 2x yield.
#[test]
fn worked_example_express_belts() {
    let plan = green_circuit_plan();
    let topo = CellTopology::default();

    let circuit = size_step(step(&plan, "electronic-circuit"), blue_belt(), &topo).unwrap();
    assert_eq!(circuit.machines_per_cell, 15);
    assert_eq!(circuit.columns, (8, 7));
    assert_eq!(circuit.cells, 2);
    assert!(circuit.bound_by.contains("copper-cable"));

    let cable = size_step(step(&plan, "copper-cable"), blue_belt(), &topo).unwrap();
    assert_eq!(cable.machines_per_cell, 14); // NOT 15: its product caps the column at 7
    assert_eq!(cable.columns, (7, 7));
    assert_eq!(cable.cells, 4);
    assert!(cable.bound_by.contains("copper-cable"));
}

/// Same plan, a slower belt tier: every number moves, but the shape of the
/// worked table (input-bound circuit, output-bound cable) does not.
#[test]
fn worked_example_yellow_belts() {
    let yellow = prototype::lookup("transport-belt").unwrap();
    let plan = green_circuit_plan();
    let topo = CellTopology::default();

    let circuit = size_step(step(&plan, "electronic-circuit"), yellow, &topo).unwrap();
    assert_eq!(circuit.machines_per_cell, 5);
    assert_eq!(circuit.columns, (3, 2));
    assert_eq!(circuit.cells, 6);

    let cable = size_step(step(&plan, "copper-cable"), yellow, &topo).unwrap();
    assert_eq!(cable.machines_per_cell, 4);
    assert_eq!(cable.columns, (2, 2));
    assert_eq!(cable.cells, 12);
}

/// Moving the product to the wider side turns 14 machines/cell into 30.
/// This is the reason the topology is configurable at all.
#[test]
fn flipping_the_topology_unbinds_an_output_bound_step() {
    let topo = CellTopology { edge_belts: 1, ingredients_on: Side::Edge, ..Default::default() };
    let cable = size_step(step(&green_circuit_plan(), "copper-cable"), blue_belt(), &topo).unwrap();
    assert_eq!(cable.machines_per_cell, 30);
    assert_eq!(cable.cells, 2);
}

/// A 3-ingredient recipe over 4 lanes: hand-compute every composition's cap
/// and check the search's own allocation beats or matches all of them.
#[test]
fn lane_allocation_maximises_machines_per_cell() {
    let goal = ChainGoal::new(
        "advanced-circuit",
        Rate::ItemsPerSec(1.0),
        &["electronic-circuit", "plastic-bar", "copper-cable"],
    )
    .with_machines(MachinePolicy::all("assembling-machine-2"))
    .with_override("advanced-circuit", "advanced-circuit");
    let plan = chain::solve(&goal).expect("advanced-circuit resolves against a flat bus");
    let ac_step = step(&plan, "advanced-circuit");
    assert_eq!(ac_step.inputs.len(), 3, "three ingredients is the point of this test");

    let topo = CellTopology::default();
    let sized = size_step(ac_step, blue_belt(), &topo).unwrap();

    let machines = ac_step.machines_needed.max(1) as f64;
    let lane = lane_throughput(blue_belt());
    let rates: Vec<(String, f64)> =
        ac_step.inputs.iter().map(|r| (r.item.clone(), r.per_sec / machines)).collect();
    let cap_of = |alloc: &[u32]| -> f64 {
        alloc
            .iter()
            .zip(&rates)
            .map(|(&lanes, (_, rate))| (lanes as f64 * lane) / rate)
            .fold(f64::INFINITY, f64::min)
    };

    // Every composition of 4 lanes into 3 positive parts, by hand.
    let candidates = [[2u32, 1, 1], [1, 2, 1], [1, 1, 2]];
    let won: Vec<u32> = sized.lane_allocation.iter().map(|(_, lanes)| *lanes).collect();
    let won_cap = cap_of(&won);
    for candidate in &candidates {
        assert!(
            won_cap >= cap_of(candidate) - 1e-9,
            "hand allocation {candidate:?} (cap {}) beats the search's {won:?} (cap {won_cap})",
            cap_of(candidate)
        );
    }
}

/// Same input, same allocation, every time — the search has no randomness
/// and no iteration-order dependence to smuggle nondeterminism in through.
#[test]
fn lane_allocation_is_deterministic() {
    let plan = green_circuit_plan();
    let circuit = step(&plan, "electronic-circuit");
    let topo = CellTopology::default();
    let first = size_step(circuit, blue_belt(), &topo).unwrap().lane_allocation;
    for _ in 0..100 {
        assert_eq!(size_step(circuit, blue_belt(), &topo).unwrap().lane_allocation, first);
    }
}

/// All 8 topology combinations, on both green-circuit steps, all size
/// without erroring — and the two express-belt pinned numbers are among the
/// results, confirming the sweep actually includes the default topology.
#[test]
fn every_topology_combination_sizes_without_erroring() {
    let plan = green_circuit_plan();
    let circuit = step(&plan, "electronic-circuit");
    let cable = step(&plan, "copper-cable");

    let mut all_mpc = Vec::new();
    for spine_belts in [1u8, 2] {
        for edge_belts in [1u8, 2] {
            for ingredients_on in [Side::Spine, Side::Edge] {
                let topo = CellTopology { spine_belts, edge_belts, ingredients_on, target_width: None };
                for s in [circuit, cable] {
                    let result = size_step(s, blue_belt(), &topo)
                        .unwrap_or_else(|e| panic!("{topo:?} failed on {}: {e}", s.recipe.name));
                    assert!(result.machines_per_cell >= 1);
                    all_mpc.push(result.machines_per_cell);
                }
            }
        }
    }

    assert!(all_mpc.contains(&15), "the default circuit number should appear: {all_mpc:?}");
    assert!(all_mpc.contains(&14), "the default cable number should appear: {all_mpc:?}");
}

/// advanced-circuit's 3 ingredients over a 1-belt (2-lane) ingredient side
/// has no allocation at all.
#[test]
fn more_ingredients_than_lanes_is_a_named_error() {
    let goal = ChainGoal::new(
        "advanced-circuit",
        Rate::ItemsPerSec(1.0),
        &["electronic-circuit", "plastic-bar", "copper-cable"],
    )
    .with_machines(MachinePolicy::all("assembling-machine-2"))
    .with_override("advanced-circuit", "advanced-circuit");
    let plan = chain::solve(&goal).expect("advanced-circuit resolves against a flat bus");
    let ac_step = step(&plan, "advanced-circuit");

    let topo = CellTopology { spine_belts: 1, ..CellTopology::default() };
    match size_step(ac_step, blue_belt(), &topo) {
        Err(LayoutError::TooManyIngredientsForLanes { recipe, ingredients, lanes }) => {
            assert_eq!(recipe, "advanced-circuit");
            assert_eq!(ingredients.len(), 3);
            assert_eq!(lanes, 2);
        }
        other => panic!("expected TooManyIngredientsForLanes, got {other:?}"),
    }
}

/// `uranium-processing` yields two items from one ore. A column owns its
/// product lanes outright, so the second product has nowhere to go.
#[test]
fn a_two_product_recipe_is_refused_by_name() {
    let plan = plan_containing_kovarex();
    let processing = step(&plan, "uranium-processing");
    match size_step(processing, blue_belt(), &CellTopology::default()) {
        Err(LayoutError::MultipleProducts { recipe, products }) => {
            assert_eq!(recipe, "uranium-processing");
            assert_eq!(products.len(), 2);
        }
        other => panic!("expected MultipleProducts, got {other:?}"),
    }
}

/// A per-machine rate above one lane's throughput has no column length that
/// works, so this errors rather than producing a zero-machine cell.
#[test]
fn a_stream_that_cannot_feed_one_machine_is_an_error() {
    let extreme = hand_step(
        "iron-plate",
        1,
        vec![rate("unobtainium", 1000.0)],
        vec![rate("electronic-circuit", 1.0)],
    );
    match size_step(&extreme, blue_belt(), &CellTopology::default()) {
        Err(LayoutError::StreamExceedsOneLane { recipe, item, rate: got_rate, lane }) => {
            assert_eq!(recipe, "iron-plate");
            assert_eq!(item, "unobtainium");
            assert_eq!(got_rate, 1000.0);
            assert_eq!(lane, 22.5);
        }
        other => panic!("expected StreamExceedsOneLane, got {other:?}"),
    }
}

/// Green circuits, default topology: copper-cable gets 3 of the 4 lanes, so
/// it sits on belt slot 0 (both lanes) AND belt slot 1 (one lane);
/// iron-plate gets belt slot 1 only. A machine drawing cable off only belt 0
/// would get 2 of the 3 lanes the sizing promised — the bug this exists to
/// catch.
#[test]
fn ingredient_belts_maps_lanes_onto_belts() {
    let plan = green_circuit_plan();
    let circuit =
        size_step(step(&plan, "electronic-circuit"), blue_belt(), &CellTopology::default()).unwrap();
    assert_eq!(circuit.ingredient_belts("copper-cable"), vec![0, 1]);
    assert_eq!(circuit.ingredient_belts("iron-plate"), vec![1]);
}

/// `chain::solve` only rejects a fluid *ingredient* that is off the bus; a
/// fluid the user declared available reaches `size_step` looking like an
/// ordinary item rate and must be refused here instead.
#[test]
fn a_fluid_is_refused_rather_than_sized() {
    let goal = ChainGoal::new("heavy-oil", Rate::ItemsPerSec(1.0), &["crude-oil", "water"])
        .with_machines(MachinePolicy::all("oil-refinery"))
        .with_override("heavy-oil", "advanced-oil-processing");
    let plan = chain::solve(&goal).expect("the calculator is happy: every fluid is on the bus");
    let oil_step = step(&plan, "advanced-oil-processing");

    match size_step(oil_step, blue_belt(), &CellTopology::default()) {
        Err(LayoutError::FluidOnBelt { recipe, item }) => {
            assert_eq!(recipe, "advanced-oil-processing");
            assert!(
                ["crude-oil", "water", "heavy-oil", "light-oil", "petroleum-gas"]
                    .contains(&item.as_str()),
                "should name one of the recipe's fluids, got {item}"
            );
        }
        other => panic!("expected FluidOnBelt, got {other:?}"),
    }
}
