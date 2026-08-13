// What must actually flow through the placed grid: no belt run carrying two
// products, and every product's own belts adding up to the rate the plan
// wants. Mirrors the source's `validate/delivery.rs`.
use super::*;

use crate::layout::lane_throughput;
use crate::testsupport::{default_cfg, hand_step, rate};

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
