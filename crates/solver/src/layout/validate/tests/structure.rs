// What must be present in the placed grid: an inserter on each side a recipe
// actually needs, no overlapping footprints, a pole in reach of every
// machine — plus the two rotation helpers every one of those rests on.
use super::*;

// ── rotate / to_delta ───────────────────────────────────────────────

#[test]
fn rotate_matches_the_documented_check() {
    // Same worked example `place::helpers::rotate` documents itself against:
    // North's (0, -1) becomes East's (1, 0) after one quarter turn.
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

/// `EntityCategory::from_prototype_name`'s substring rules recognise
/// "assembling", "furnace", "chemical" and "refinery" but not "centrifuge" —
/// before `is_machine` was rekeyed on `crafting_categories`, a centrifuge
/// with no inserters at all silently skipped this check instead of failing
/// it. `uranium-processing`'s own machine, and the reason the gap mattered
/// enough to fix as part of this feature.
#[test]
fn a_centrifuge_is_recognised_as_a_machine_needing_connection() {
    let mut grid = Grid::new();
    grid.place(
        "centrifuge",
        &Position { x: 1.5, y: 1.5 },
        Direction::North,
        Some("uranium-processing".to_string()),
        None,
    )
    .unwrap();

    match check_machine_connectivity(&grid) {
        Err(LayoutError::MachineNotConnected { recipe, missing, .. }) => {
            assert_eq!(recipe, "uranium-processing");
            assert_eq!(missing, "input");
        }
        other => panic!("expected MachineNotConnected, got {other:?}"),
    }
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
