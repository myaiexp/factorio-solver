// Geometry tests for `place_step`: what lands on the grid, where, facing
// which way, and every refusal. Split out of `rows.rs` to keep both files
// readable; `super::*` still reaches the private rotation helpers, which the
// inserter-direction test needs to check orientation the same way the
// implementation derives it.
use super::*;
use factorio_grid::prototype;
use std::collections::BTreeMap;

use crate::chain::{self, ChainGoal, MachinePolicy, ProductionPlan, Rate};
use crate::testsupport::{default_cfg, green_circuit_plan, rate};

/// Group live machine entities by their top row, top-to-bottom — the
/// per-sub-row machine counts, in placement order.
fn machine_counts_by_row(grid: &Grid, machine_name: &str) -> Vec<usize> {
    let mut rows: BTreeMap<i32, usize> = BTreeMap::new();
    for e in grid.entities().filter(|e| e.prototype_name == machine_name) {
        *rows.entry(e.top_left.y).or_insert(0) += 1;
    }
    rows.into_values().collect()
}

/// Stack every step of `plan` downward from the origin — `Grid::place`
/// rejects collisions on its own, so an overlap here fails loudly.
fn place_plan(plan: &ProductionPlan, cfg: &ResolvedConfig) -> Grid {
    let mut grid = Grid::new();
    let mut y = 0;
    for step in &plan.steps {
        let extent = place_step(&mut grid, step, cfg, GridPos { x: 0, y }).unwrap();
        y += extent.height as i32 + STEP_GAP;
    }
    grid
}

/// A minimal hand-built step for exercising geometry the design's own
/// fixtures don't happen to hit (a too-narrow machine, an uneven split).
/// The recipe is a stand-in — only its name shows up in error messages.
fn hand_step(
    machine: &'static EntityPrototype,
    machines_needed: u32,
    inputs: Vec<ItemRate>,
    outputs: Vec<ItemRate>,
) -> ProductionStep {
    ProductionStep {
        recipe: crate::recipe::get("electronic-circuit").unwrap(),
        machine,
        exact_count: machines_needed as f64,
        machines_needed,
        crafts_per_sec: 1.0,
        inputs,
        outputs,
    }
}

#[test]
fn machines_do_not_overlap() {
    let grid = place_plan(&green_circuit_plan(), &default_cfg().resolve().unwrap());
    let count = grid.entities().filter(|e| e.prototype_name == "assembling-machine-2").count();
    assert_eq!(count, 75, "30 circuit machines + 45 cable machines");
}

#[test]
fn every_machine_has_an_adjacent_inserter() {
    let cfg = default_cfg().resolve().unwrap();
    let grid = place_plan(&green_circuit_plan(), &cfg);
    let inserter_name = cfg.inserter.name.as_str();
    let footprints: Vec<(i32, i32, i32, i32)> = grid
        .entities()
        .filter(|e| e.prototype_name == "assembling-machine-2")
        .map(|e| e.aabb())
        .collect();
    assert!(!footprints.is_empty());

    for (min_x, min_y, max_x, max_y) in footprints {
        let is_inserter = |x: i32, y: i32| {
            grid.get_at(x, y).map(|e| e.prototype_name) == Some(inserter_name)
        };
        let touches = (min_x..=max_x).any(|x| is_inserter(x, min_y - 1) || is_inserter(x, max_y + 1))
            || (min_y..=max_y).any(|y| is_inserter(min_x - 1, y) || is_inserter(max_x + 1, y));
        assert!(touches, "machine at ({min_x},{min_y})-({max_x},{max_y}) has no adjacent inserter");
    }
}

#[test]
fn inserters_face_between_machine_and_belt() {
    let plan = green_circuit_plan();
    let cfg = default_cfg().resolve().unwrap();
    let mut grid = Grid::new();
    place_step(&mut grid, &plan.steps[0], &cfg, GridPos { x: 0, y: 0 }).unwrap();

    let inserters: Vec<(GridPos, Direction)> = grid
        .entities()
        .filter(|e| e.prototype_name == cfg.inserter.name)
        .map(|e| (e.top_left, e.direction))
        .collect();
    assert!(!inserters.is_empty());

    let belt = cfg.belt.name.as_str();
    let machine = "assembling-machine-2";

    for (pos, dir) in inserters {
        let turns = dir.as_u8() / 4;
        let pickup = to_delta(rotate(cfg.inserter.pickup_position.unwrap(), turns));
        let insert = to_delta(rotate(cfg.inserter.insert_position.unwrap(), turns));
        let pickup_name =
            grid.get_at(pos.x + pickup.0, pos.y + pickup.1).map(|e| e.prototype_name);
        let insert_name =
            grid.get_at(pos.x + insert.0, pos.y + insert.1).map(|e| e.prototype_name);

        match pickup_name {
            Some(n) if n == belt => {
                assert_eq!(insert_name, Some(machine), "input inserter must feed the machine")
            }
            Some(n) if n == machine => {
                assert_eq!(insert_name, Some(belt), "output inserter must feed the belt")
            }
            other => panic!("inserter at {pos:?} picks up from unexpected entity: {other:?}"),
        }
    }
}

/// The returned extent must exactly bound what actually landed on the
/// grid, at a positive origin and — since the grid is unbounded and
/// signed — at a negative one too. An implementation that assumes
/// non-negative coordinates fails the second case.
#[test]
fn step_extent_matches_placed_entities_at_any_origin() {
    let plan = green_circuit_plan();
    let cfg = default_cfg().resolve().unwrap();

    for origin in [GridPos { x: 5, y: 5 }, GridPos { x: -12, y: -30 }] {
        let mut grid = Grid::new();
        let extent = place_step(&mut grid, &plan.steps[0], &cfg, origin).unwrap();
        let (min, max) = grid.bounding_box().unwrap();
        assert_eq!(min, origin);
        assert_eq!(
            max,
            GridPos { x: origin.x + extent.width as i32 - 1, y: origin.y + extent.height as i32 - 1 }
        );
    }
}

#[test]
fn circuit_step_needs_six_sub_rows_of_five() {
    // The design's worked example (see `lanes::cable_demand_needs_six_lanes`):
    // 135/s copper cable on blue belts is 6 lanes, which is 6 physical
    // belts once paired with iron plate, so 30 machines split 6 x 5.
    let plan = green_circuit_plan();
    let cfg = default_cfg().resolve().unwrap();
    let circuit =
        plan.steps.iter().find(|s| s.recipe.name == "electronic-circuit").unwrap();

    let in_belts = pack_lanes(&circuit.inputs, cfg.belt);
    let out_belts = pack_lanes(&circuit.outputs, cfg.belt);
    let expected_sub_rows =
        in_belts.len().max(out_belts.len()).max(1).min(circuit.machines_needed as usize);
    let expected_per_row = circuit.machines_needed as usize / expected_sub_rows;

    let mut grid = Grid::new();
    place_step(&mut grid, circuit, &cfg, GridPos { x: 0, y: 0 }).unwrap();
    let counts = machine_counts_by_row(&grid, "assembling-machine-2");

    assert_eq!(counts.len(), expected_sub_rows);
    assert!(counts.iter().all(|&c| c == expected_per_row), "{counts:?}");
    assert_eq!((expected_sub_rows, expected_per_row), (6, 5));
}

#[test]
fn uneven_distribution_gives_the_remainder_to_the_first_sub_rows_and_belts_stay_full_width() {
    // Two items sharing every belt (see `pack_lanes`): lanes =
    // max(3, 1) = 3 belts, so this hand-built step needs 3 sub-rows.
    // 7 machines over 3 sub-rows is an uneven 3/2/2 split.
    let machine = prototype::lookup("assembling-machine-2").unwrap();
    let cfg = default_cfg().resolve().unwrap();
    let step =
        hand_step(machine, 7, vec![rate("iron-plate", 67.5), rate("copper-plate", 22.5)], vec![]);

    let mut grid = Grid::new();
    let extent = place_step(&mut grid, &step, &cfg, GridPos { x: 0, y: 0 }).unwrap();
    assert_eq!(machine_counts_by_row(&grid, "assembling-machine-2"), vec![3, 2, 2]);

    let (_, mh) = effective_size(machine, Direction::North);
    assert_eq!(extent.width, 9, "the widest sub-row (3 machines) sets the width");
    for s in 0..3i32 {
        let band_top = s * (mh as i32 + 4 + STEP_GAP);
        for x in 0..extent.width as i32 {
            assert!(grid.get_at(x, band_top).is_some(), "sub-row {s} belt missing at column {x}");
        }
    }
}

#[test]
fn no_room_for_inserters_when_the_machine_is_too_narrow() {
    // A hand-built step: `inserter` (1x1) stands in for a machine too
    // narrow for the 2 inserters its ingredients need.
    let machine = prototype::lookup("inserter").unwrap();
    let cfg = default_cfg().resolve().unwrap();
    let step = hand_step(
        machine,
        1,
        vec![rate("iron-plate", 10.0), rate("copper-cable", 10.0)],
        vec![rate("electronic-circuit", 10.0)],
    );

    let mut grid = Grid::new();
    match place_step(&mut grid, &step, &cfg, GridPos { x: 0, y: 0 }) {
        Err(LayoutError::NoRoomForInserters { recipe, machine, needed, edge_tiles }) => {
            assert_eq!(recipe, "electronic-circuit");
            assert_eq!(machine, "inserter");
            assert_eq!(needed, 2);
            assert_eq!(edge_tiles, 1);
        }
        other => panic!("expected NoRoomForInserters, got {other:?}"),
    }
}

#[test]
fn too_many_ingredients_from_a_real_recipe() {
    // advanced-circuit takes 3 ingredients — one more than a single
    // input belt's two lanes can carry.
    let goal = ChainGoal::new(
        "advanced-circuit",
        Rate::ItemsPerSec(1.0),
        &["electronic-circuit", "plastic-bar", "copper-cable"],
    )
    .with_machines(MachinePolicy::all("assembling-machine-2"))
    .with_override("advanced-circuit", "advanced-circuit");
    let plan = chain::solve(&goal).expect("advanced-circuit resolves against a flat bus");
    assert_eq!(plan.steps.len(), 1);

    let cfg = default_cfg().resolve().unwrap();
    let mut grid = Grid::new();
    match place_step(&mut grid, &plan.steps[0], &cfg, GridPos { x: 0, y: 0 }) {
        Err(LayoutError::TooManyIngredientsForLanes { recipe, ingredients, lanes }) => {
            assert_eq!(recipe, "advanced-circuit");
            assert_eq!(ingredients.len(), 3);
            assert_eq!(lanes, 2);
        }
        other => panic!("expected TooManyIngredientsForLanes, got {other:?}"),
    }
}

#[test]
fn too_many_outputs_from_a_real_recipe() {
    // Asteroid reprocessing yields 3 *items* — one more than a single output
    // belt's two lanes can carry, and unlike the oil recipes it is not
    // rejected as fluid first.
    let goal =
        ChainGoal::new("metallic-asteroid-chunk", Rate::ItemsPerSec(1.0), &["carbonic-asteroid-chunk"])
            .with_override("metallic-asteroid-chunk", "carbonic-asteroid-reprocessing");
    let plan = chain::solve(&goal).expect("asteroid reprocessing resolves against a flat bus");
    let step = plan
        .steps
        .iter()
        .find(|s| s.recipe.name == "carbonic-asteroid-reprocessing")
        .expect("the override selected it");
    assert_eq!(step.recipe.results.len(), 3);

    let cfg = default_cfg().resolve().unwrap();
    let mut grid = Grid::new();
    match place_step(&mut grid, step, &cfg, GridPos { x: 0, y: 0 }) {
        Err(LayoutError::MultipleProducts { recipe, products }) => {
            assert_eq!(recipe, "carbonic-asteroid-reprocessing");
            assert_eq!(products.len(), 3);
        }
        other => panic!("expected MultipleProducts, got {other:?}"),
    }
}

/// A fluid cannot ride a belt, and the chain calculator does not catch every
/// case: it only rejects a fluid *ingredient* that is off the bus. Declare
/// the fluid available and the step arrives here looking like an ordinary
/// item rate, so the layout has to refuse it itself.
#[test]
fn a_fluid_is_refused_rather_than_belted() {
    let goal = ChainGoal::new("heavy-oil", Rate::ItemsPerSec(1.0), &["crude-oil", "water"])
        .with_machines(MachinePolicy::all("oil-refinery"))
        .with_override("heavy-oil", "advanced-oil-processing");
    let plan = chain::solve(&goal).expect("the calculator is happy: every fluid is on the bus");
    let step = plan
        .steps
        .iter()
        .find(|s| s.recipe.name == "advanced-oil-processing")
        .expect("the override selected it");

    let cfg = default_cfg().resolve().unwrap();
    let mut grid = Grid::new();
    match place_step(&mut grid, step, &cfg, GridPos { x: 0, y: 0 }) {
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
    assert_eq!(grid.entity_count(), 0, "nothing may be placed before the refusal");
}
