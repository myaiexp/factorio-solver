// Unit tests for the individual checks. Hand-built grids exercise the hard
// errors directly (through the private per-check functions, via `super::*`)
// rather than only through `generate`, since `generate`'s own row/pole
// passes are supposed to make every one of these impossible — the point of
// this module is to prove that independently. The mandatory end-to-end
// coverage (a real generated block, the round-trip, the reachable throughput
// warning) lives in `tests/layout_output.rs` instead.
use super::*;
use factorio_blueprint::{Direction, Position};

use crate::testsupport::{blue_belt, rate};

/// A bare 3x3 machine with `recipe` set, top-left at (0, 0). Callers add
/// whatever inserters the case under test needs.
fn place_machine(grid: &mut Grid, recipe: &str) {
    grid.place(
        "assembling-machine-2",
        &Position { x: 1.5, y: 1.5 },
        Direction::North,
        Some(recipe.to_string()),
        None,
    )
    .unwrap();
}

/// A single 1x1 inserter at `(x, y)` facing `dir`. `fast-inserter` matches
/// what `default_cfg()` resolves to, so its pickup/insert positions are the
/// same (0, -1) / (0, 1) `rows.rs` relies on.
fn place_inserter(grid: &mut Grid, x: i32, y: i32, dir: Direction) {
    grid.place(
        "fast-inserter",
        &Position { x: x as f64 + 0.5, y: y as f64 + 0.5 },
        dir,
        None,
        None,
    )
    .unwrap();
}

// ── rotate / to_delta ───────────────────────────────────────────────

#[test]
fn rotate_matches_the_documented_check() {
    // Same worked example `rows::rotate` documents itself against: North's
    // (0, -1) becomes East's (1, 0) after one quarter turn.
    assert_eq!(rotate((0.0, -1.0), 1), (1.0, 0.0));
    assert_eq!(rotate((0.0, -1.0), 0), (0.0, -1.0));
    assert_eq!(rotate((0.0, -1.0), 4), (0.0, -1.0), "four turns is a no-op");
}

#[test]
fn to_delta_rounds_to_the_nearest_cell() {
    assert_eq!(to_delta((0.0, -1.0)), (0, -1));
    assert_eq!(to_delta((1.2, -0.9)), (1, -1));
}

// ── machine connectivity ────────────────────────────────────────────

#[test]
fn missing_input_inserter_is_reported() {
    let mut grid = Grid::new();
    place_machine(&mut grid, "iron-gear-wheel"); // needs both an input and an output
    // Output only: picks up from the machine's top row, inserts below it.
    place_inserter(&mut grid, 1, 3, Direction::North);

    match check_machine_connectivity(&grid) {
        Err(LayoutError::MachineNotConnected { recipe, x, y, missing }) => {
            assert_eq!(recipe, "iron-gear-wheel");
            assert_eq!((x, y), (0, 0));
            assert_eq!(missing, "input");
        }
        other => panic!("expected a missing-input MachineNotConnected, got {other:?}"),
    }
}

#[test]
fn missing_output_inserter_is_reported() {
    let mut grid = Grid::new();
    place_machine(&mut grid, "iron-gear-wheel");
    // Input only: picks up above the machine, inserts into its top row.
    place_inserter(&mut grid, 1, -1, Direction::North);

    match check_machine_connectivity(&grid) {
        Err(LayoutError::MachineNotConnected { recipe, x, y, missing }) => {
            assert_eq!(recipe, "iron-gear-wheel");
            assert_eq!((x, y), (0, 0));
            assert_eq!(missing, "output");
        }
        other => panic!("expected a missing-output MachineNotConnected, got {other:?}"),
    }
}

#[test]
fn fully_connected_machine_passes() {
    let mut grid = Grid::new();
    place_machine(&mut grid, "iron-gear-wheel");
    place_inserter(&mut grid, 1, -1, Direction::North); // input
    place_inserter(&mut grid, 1, 3, Direction::North); // output
    assert!(check_machine_connectivity(&grid).is_ok());
}

/// `recipe-unknown` is a real registry entry with empty ingredients *and*
/// empty results — genuinely nothing to connect. Neither inserter is
/// required, which is the case the "by construction" carve-out in the spec
/// exists for.
#[test]
fn a_recipe_with_neither_ingredients_nor_results_needs_no_inserters() {
    let mut grid = Grid::new();
    place_machine(&mut grid, "recipe-unknown");
    assert!(check_machine_connectivity(&grid).is_ok());
}

/// `biter-egg` has results but no ingredients: only the output side is
/// required.
#[test]
fn an_ingredient_free_recipe_needs_only_an_output() {
    let mut grid = Grid::new();
    place_machine(&mut grid, "biter-egg");
    place_inserter(&mut grid, 1, 3, Direction::North); // output only
    assert!(check_machine_connectivity(&grid).is_ok());
}

// ── overlap ─────────────────────────────────────────────────────────

#[test]
fn a_normally_placed_grid_has_no_overlaps() {
    // `Grid::place` refuses collisions itself, so this exercises the happy
    // path of the invariant re-check rather than a case it can catch today.
    let mut grid = Grid::new();
    place_machine(&mut grid, "iron-gear-wheel");
    place_inserter(&mut grid, 1, -1, Direction::North);
    assert!(check_no_overlaps(&grid).is_ok());
}

// ── pole coverage ───────────────────────────────────────────────────

#[test]
fn unpowered_machine_is_reported() {
    let mut grid = Grid::new();
    place_machine(&mut grid, "iron-gear-wheel");

    match check_pole_coverage(&grid) {
        Err(LayoutError::Unpowered { recipe, x, y }) => {
            assert_eq!(recipe, "iron-gear-wheel");
            assert_eq!((x, y), (0, 0));
        }
        other => panic!("expected Unpowered, got {other:?}"),
    }
}

#[test]
fn a_pole_in_reach_covers_the_machine() {
    let mut grid = Grid::new();
    place_machine(&mut grid, "iron-gear-wheel");
    grid.place(
        "medium-electric-pole",
        &Position { x: 4.5, y: 1.5 },
        Direction::North,
        None,
        None,
    )
    .unwrap();
    assert!(check_pole_coverage(&grid).is_ok());
}

// ── belt-lane bookkeeping ───────────────────────────────────────────

#[test]
fn lanes_for_item_matches_pack_lanes_own_split() {
    let alone = [rate("copper-cable", 135.0)];
    assert_eq!(lanes_for_item(&alone, "copper-cable"), 2);
    assert_eq!(pack_lanes(&alone, blue_belt())[0].lanes_for("copper-cable"), 2);

    let paired = [rate("iron-plate", 45.0), rate("copper-cable", 135.0)];
    assert_eq!(lanes_for_item(&paired, "iron-plate"), 1);
    assert_eq!(lanes_for_item(&paired, "copper-cable"), 1);
    for belt in pack_lanes(&paired, blue_belt()) {
        assert_eq!(belt.lanes_for("iron-plate") + belt.lanes_for("copper-cable"), 2);
    }
}

// ── fixing-tier lookup ──────────────────────────────────────────────

#[test]
fn fixing_tier_names_the_slowest_sufficient_mainline_belt() {
    // Two lanes: express-transport-belt tops out at 45/s, turbo at 60/s.
    assert_eq!(fixing_tier(50.0, 2), Some("turbo-transport-belt".to_string()));
    // One lane: transport-belt's 7.5/s is not enough, fast's 15/s is.
    assert_eq!(fixing_tier(10.0, 1), Some("fast-transport-belt".to_string()));
    // Exactly at a tier's capacity is sufficient, not just strictly under.
    assert_eq!(fixing_tier(45.0, 2), Some("express-transport-belt".to_string()));
}

#[test]
fn fixing_tier_is_none_when_nothing_registered_is_fast_enough() {
    assert_eq!(fixing_tier(10_000.0, 2), None);
}

/// Sanity check on the category filter itself: a loader shares its
/// throughput with a mainline belt tier, and must not be the name that
/// comes back.
#[test]
fn fixing_tier_never_names_a_loader_or_splitter() {
    let tier = fixing_tier(1.0, 2).expect("something this slow is always coverable");
    assert_eq!(EntityCategory::from_prototype_name(&tier), EntityCategory::Belt);
}
