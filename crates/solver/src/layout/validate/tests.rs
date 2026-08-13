// Unit tests for the individual checks. Hand-built grids exercise the hard
// errors directly (through the private per-check functions, via `super::*`)
// rather than only through `generate`, since `generate`'s own cell/pole
// passes are supposed to make every one of these impossible — the point of
// this module is to prove that independently. The mandatory end-to-end
// coverage (a real generated block, the round-trip) lives in
// `tests/layout_output.rs` instead.
use super::*;
use crate::layout::lane_throughput;
use crate::testsupport::{default_cfg, hand_step, rate};
use factorio_blueprint::{Direction, Position};

/// A 3x3 machine with `recipe` set, top-left at `(x, y)`.
fn place_machine_at(grid: &mut Grid, recipe: &str, x: i32, y: i32) {
    grid.place(
        "assembling-machine-2",
        &Position { x: x as f64 + 1.5, y: y as f64 + 1.5 },
        Direction::North,
        Some(recipe.to_string()),
        None,
    )
    .unwrap();
}

/// A bare 3x3 machine with `recipe` set, top-left at (0, 0). Callers add
/// whatever inserters the case under test needs.
fn place_machine(grid: &mut Grid, recipe: &str) {
    place_machine_at(grid, recipe, 0, 0);
}

/// A single 1x1 inserter at `(x, y)` facing `dir`. `fast-inserter` matches
/// what `default_cfg()` resolves to, so its pickup/insert positions are the
/// same (0, -1) / (0, 1) the placement code relies on.
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

/// A single 1x1 belt tile at `(x, y)`, facing `dir`. `express-transport-belt`
/// matches what `default_cfg()` resolves to (22.5/s per lane).
fn place_belt(grid: &mut Grid, x: i32, y: i32, dir: Direction) {
    grid.place(
        "express-transport-belt",
        &Position { x: x as f64 + 0.5, y: y as f64 + 0.5 },
        dir,
        None,
        None,
    )
    .unwrap();
}

/// A single 1x1 inserter at `(x, y)` facing `dir`, filtered for `item` — the
/// same prototype `place_inserter` uses, plus the filter a multi-product
/// step's own output inserter would carry.
fn place_filtered_inserter(grid: &mut Grid, x: i32, y: i32, dir: Direction, item: &str) {
    let id = grid
        .place("fast-inserter", &Position { x: x as f64 + 0.5, y: y as f64 + 0.5 }, dir, None, None)
        .unwrap();
    grid.set_filters(id, vec![item.to_string()]).unwrap();
}

/// A one-step plan whose only step wants `per_sec` of `recipe`'s product —
/// enough to drive `check_delivered_rate` in isolation, without going
/// through `chain::solve`.
fn plan_wanting(recipe: &str, per_sec: f64) -> ProductionPlan {
    let step = hand_step(recipe, 5, vec![], vec![rate(recipe, per_sec)]);
    ProductionPlan { steps: vec![step], inputs: vec![], byproducts: vec![], warnings: vec![] }
}

/// A one-step plan whose step wants `per_sec` of TWO different products —
/// `check_delivered_rate`'s per-product path, driven directly without a full
/// two-product `chain::solve` plan.
fn plan_wanting_two(recipe: &str, a: (&str, f64), b: (&str, f64)) -> ProductionPlan {
    let step = hand_step(recipe, 5, vec![], vec![rate(a.0, a.1), rate(b.0, b.1)]);
    ProductionPlan { steps: vec![step], inputs: vec![], byproducts: vec![], warnings: vec![] }
}

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

// ── mixed product belts ────────────────────────────────────────────

/// Two output inserters filtered for different items, both landing on the
/// same physical belt run from the same side (so, the same lane).
#[test]
fn two_filters_on_the_same_belt_run_is_a_named_error() {
    let mut grid = Grid::new();
    place_machine_at(&mut grid, "uranium-processing", 0, 0);
    place_filtered_inserter(&mut grid, 3, 0, Direction::West, "uranium-235");
    place_filtered_inserter(&mut grid, 3, 1, Direction::West, "uranium-238");
    place_belt(&mut grid, 4, 0, Direction::South);
    place_belt(&mut grid, 4, 1, Direction::South); // contiguous with the belt above: one run

    match check_no_mixed_product_belts(&grid) {
        Err(LayoutError::MixedProductBelt { recipe, items, .. }) => {
            assert_eq!(recipe, "uranium-processing");
            assert!(
                items.contains(&"uranium-235".to_string()) && items.contains(&"uranium-238".to_string()),
                "{items:?}"
            );
        }
        other => panic!("expected MixedProductBelt, got {other:?}"),
    }
}

/// The shape the real bug actually produces, and the reason this check is
/// keyed on the run alone: the two machine columns sit on OPPOSITE sides of
/// a shared belt, so their inserters land on different rows and claim
/// different *lanes* of one run. A `(run, lane)` key files those as two
/// separate buckets and reports nothing — measured, by reintroducing the
/// mirror bug in `place_cell` and watching all 224 tests pass. A belt's two
/// lanes are one physical belt here: a downstream inserter picks from both.
#[test]
fn two_filters_on_opposite_lanes_of_one_run_is_still_mixed() {
    let mut grid = Grid::new();
    place_machine_at(&mut grid, "uranium-processing", 0, 0);
    place_belt(&mut grid, 4, 0, Direction::South);
    place_belt(&mut grid, 4, 1, Direction::South); // one contiguous run
    // West of the belt, dropping on its east lane...
    place_filtered_inserter(&mut grid, 3, 0, Direction::West, "uranium-235");
    // ...and east of it, dropping on its west lane. Different lanes, one belt.
    place_machine_at(&mut grid, "uranium-processing", 6, 1);
    place_filtered_inserter(&mut grid, 5, 1, Direction::East, "uranium-238");

    match check_no_mixed_product_belts(&grid) {
        Err(LayoutError::MixedProductBelt { recipe, items, .. }) => {
            assert_eq!(recipe, "uranium-processing");
            assert_eq!(items, vec!["uranium-235".to_string(), "uranium-238".to_string()]);
        }
        other => panic!("expected MixedProductBelt across the two lanes, got {other:?}"),
    }
}

/// Same shape, but the two belts are not contiguous (a gap at y=1), so they
/// are two separate runs — one filter each, nothing mixed.
#[test]
fn distinct_filters_on_distinct_belt_runs_pass() {
    let mut grid = Grid::new();
    place_machine_at(&mut grid, "uranium-processing", 0, 0);
    place_filtered_inserter(&mut grid, 3, 0, Direction::West, "uranium-235");
    place_filtered_inserter(&mut grid, 3, 2, Direction::West, "uranium-238");
    place_belt(&mut grid, 4, 0, Direction::South);
    place_belt(&mut grid, 4, 2, Direction::South);

    assert!(check_no_mixed_product_belts(&grid).is_ok());
}

// ── delivered rate ──────────────────────────────────────────────────

/// Five machines, five output inserters, one twenty-tile belt run — all
/// landing on the same `(run, lane)` pair (a `West`-facing inserter picks up
/// from the machine to its west and drops onto the belt to its east). A
/// per-tile or per-inserter bug would report five, or twenty, times a single
/// lane's throughput; the real check caps it at one claimed lane, proving
/// the check measures the run rather than restating placement.
#[test]
fn delivered_rate_counts_one_lane_once_no_matter_how_many_inserters_or_tiles_reach_it() {
    let mut grid = Grid::new();
    for i in 0..5 {
        let y = 4 * i;
        place_machine_at(&mut grid, "iron-gear-wheel", 0, y);
        place_inserter(&mut grid, 3, y + 1, Direction::West);
    }
    for y in 0..20 {
        place_belt(&mut grid, 4, y, Direction::South);
    }
    let cfg = default_cfg().resolve().unwrap();
    let lane = lane_throughput(cfg.belt);

    check_delivered_rate(&grid, &plan_wanting("iron-gear-wheel", lane - 2.0), &cfg)
        .expect("one claimed lane comfortably covers a rate under it");

    match check_delivered_rate(&grid, &plan_wanting("iron-gear-wheel", lane + 2.0), &cfg) {
        Err(LayoutError::UnderDelivers { delivered, .. }) => assert_eq!(
            delivered, lane,
            "five inserters on one twenty-tile run must still count as exactly one lane"
        ),
        other => panic!("expected UnderDelivers, got {other:?}"),
    }
}

#[test]
fn a_machine_with_no_output_inserter_delivers_nothing() {
    let mut grid = Grid::new();
    place_machine(&mut grid, "iron-gear-wheel");
    let cfg = default_cfg().resolve().unwrap();

    match check_delivered_rate(&grid, &plan_wanting("iron-gear-wheel", 1.0), &cfg) {
        Err(LayoutError::UnderDelivers { recipe, item, delivered, wanted }) => {
            assert_eq!(recipe, "iron-gear-wheel");
            assert_eq!(item, "iron-gear-wheel");
            assert_eq!(delivered, 0.0);
            assert_eq!(wanted, 1.0);
        }
        other => panic!("expected UnderDelivers, got {other:?}"),
    }
}

/// A step with nothing left to deliver (netted to zero, or ingredient-only)
/// is skipped, not required to prove it delivers zero of nothing.
#[test]
fn a_step_with_no_positive_output_rate_is_skipped() {
    let grid = Grid::new();
    let cfg = default_cfg().resolve().unwrap();
    assert!(check_delivered_rate(&grid, &plan_wanting("iron-gear-wheel", 0.0), &cfg).is_ok());
}

/// Two products, each with its own filtered inserter and belt: uranium-235's
/// belt comfortably covers what the plan wants, uranium-238's doesn't — one
/// claimed lane against a wanted rate just over it. The old
/// first-positive-output-only check summed lanes per *recipe*, so
/// uranium-238's shortfall would have been checked against whichever output
/// happened to come first in `step.outputs` — this is the case that check
/// passed.
#[test]
fn a_second_product_one_belt_short_under_delivers() {
    let mut grid = Grid::new();
    place_machine_at(&mut grid, "uranium-processing", 0, 0);
    place_filtered_inserter(&mut grid, 3, 0, Direction::West, "uranium-235");
    place_belt(&mut grid, 4, 0, Direction::South);
    place_filtered_inserter(&mut grid, 3, 2, Direction::West, "uranium-238");
    place_belt(&mut grid, 4, 2, Direction::South);

    let cfg = default_cfg().resolve().unwrap();
    let lane = lane_throughput(cfg.belt);
    let plan =
        plan_wanting_two("uranium-processing", ("uranium-235", lane - 1.0), ("uranium-238", lane + 2.0));

    match check_delivered_rate(&grid, &plan, &cfg) {
        Err(LayoutError::UnderDelivers { item, delivered, wanted, .. }) => {
            assert_eq!(item, "uranium-238", "the short product must be named, not the fully-covered one");
            assert_eq!(delivered, lane);
            assert_eq!(wanted, lane + 2.0);
        }
        other => panic!("expected UnderDelivers naming uranium-238, got {other:?}"),
    }
}

/// uranium-235 has ample belts; uranium-238 has none at all. Keying claims
/// on `(recipe, filter)` instead of `recipe` alone is what stops
/// uranium-235's lane from being credited toward uranium-238's requirement.
#[test]
fn one_products_lanes_are_not_credited_to_the_other() {
    let mut grid = Grid::new();
    place_machine_at(&mut grid, "uranium-processing", 0, 0);
    place_filtered_inserter(&mut grid, 3, 0, Direction::West, "uranium-235");
    place_belt(&mut grid, 4, 0, Direction::South);
    // uranium-238 gets no inserter at all.

    let cfg = default_cfg().resolve().unwrap();
    let lane = lane_throughput(cfg.belt);
    let plan = plan_wanting_two("uranium-processing", ("uranium-235", lane - 1.0), ("uranium-238", 1.0));

    match check_delivered_rate(&grid, &plan, &cfg) {
        Err(LayoutError::UnderDelivers { item, delivered, .. }) => {
            assert_eq!(item, "uranium-238");
            assert_eq!(delivered, 0.0, "uranium-235's claimed lane must not count toward uranium-238");
        }
        other => panic!("expected UnderDelivers naming uranium-238, got {other:?}"),
    }
}
