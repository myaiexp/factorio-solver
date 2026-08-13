// End-to-end guarantees for the block generator: a real plan in, a grid and
// a blueprint string out, and both survive the round trip a player actually
// exercises (paste into the game, or re-import for another pass). Also
// carries the delivered-rate guarantee (#3364): the plan's own goal rate,
// measured from the entities `generate` actually placed rather than from the
// sizing arithmetic that decided the placement.
use factorio_grid::prototype;
use factorio_grid::{EntityCategory, EntityId, Grid};

use factorio_blueprint::{factorio_major_version, BlueprintData};
use factorio_solver::layout::{generate, validate, LaneSide, LayoutError, BLUEPRINT_VERSION};
use factorio_solver::testsupport::{default_cfg, green_circuit_plan};

/// The distinct `(belt-run anchor, lane)` pairs a set of inserters claims.
/// A run anchor is the belt run's first tile, so two inserters on the same
/// run and lane collapse to one claim — which is the point: a lane's
/// throughput is shared by everything that drops onto it.
type LaneClaims = std::collections::HashSet<((i32, i32), LaneSide)>;

#[test]
fn generated_blueprint_round_trips() {
    let grid = generate(&green_circuit_plan(), &default_cfg()).unwrap();
    // NOTE: to_blueprint returns a `Blueprint`, but encode takes `&BlueprintData`.
    // It must be wrapped — a bare Blueprint will not compile.
    let data = BlueprintData {
        blueprint: Some(factorio_grid::to_blueprint(&grid, Some("test".into()), BLUEPRINT_VERSION)),
        blueprint_book: None,
    };
    let s = factorio_blueprint::encode(&data).unwrap();
    let back = factorio_blueprint::decode(&s).unwrap();
    let regrid = factorio_grid::from_blueprint(back.blueprint.as_ref().unwrap());
    assert!(regrid.skipped.is_empty(), "no entity may be dropped on round-trip");
    assert_eq!(regrid.grid.entity_count(), grid.entity_count());

    // Equal counts alone would pass a blueprint whose directions or recipes
    // got scrambled in transit — a real risk given `from_blueprint` decides
    // whether to upgrade legacy directions from the stamped version. Compare
    // the actual multiset of (prototype, position, direction, recipe): order
    // is not guaranteed to survive re-import, so sort before comparing.
    let mut before: Vec<(String, i32, i32, u8, Option<String>)> = grid
        .entities()
        .map(|e| (e.prototype_name.to_string(), e.top_left.x, e.top_left.y, e.direction.as_u8(), e.recipe.clone()))
        .collect();
    let mut after: Vec<(String, i32, i32, u8, Option<String>)> = regrid
        .grid
        .entities()
        .map(|e| (e.prototype_name.to_string(), e.top_left.x, e.top_left.y, e.direction.as_u8(), e.recipe.clone()))
        .collect();
    before.sort();
    after.sort();
    assert_eq!(before, after, "position, direction and recipe must all survive the round trip");
}

#[test]
fn validate_accepts_the_green_circuit_block() {
    let plan = green_circuit_plan();
    let cfg = default_cfg();
    let grid = generate(&plan, &cfg).unwrap();

    assert!(factorio_solver::layout::coverage_gaps(&grid).is_empty());

    // `Validation::warnings` is just the plan's own soft findings, carried
    // through from `chain::solve` — the layout phase has none of its own to
    // add; its findings are hard errors instead (see `check_delivered_rate`).
    let v = validate(&grid, &plan, &cfg).unwrap();
    assert!(v.warnings.is_empty(), "the design's own worked example should not warn: {:?}", v.warnings);
}

/// The test that would have caught #3364: measured from the placed grid, not
/// from the sizing arithmetic that decided the placement.
#[test]
fn the_block_delivers_its_goal_rate() {
    let plan = green_circuit_plan();
    let grid = generate(&plan, &default_cfg()).unwrap();
    validate(&grid, &plan, &default_cfg()).unwrap();
}

#[test]
fn a_deliberately_undersized_grid_is_caught() {
    let plan = green_circuit_plan();
    let cfg = default_cfg();
    let mut grid = generate(&plan, &cfg).unwrap();
    assert!(validate(&grid, &plan, &cfg).is_ok(), "the healthy block must validate before we break it");

    // Every belt an `electronic-circuit` output inserter's far lane lands
    // on — rebuilt from the public API rather than the private
    // `check_delivered_rate` this is meant to test, mirroring the same
    // independence `validate.rs` itself keeps from `place.rs`.
    let (_, circuit_belts) = product_claims(&grid, "electronic-circuit");
    assert!(!circuit_belts.is_empty(), "the circuit step must have at least one product belt");

    // Removing BELTS (not machines or inserters) is the clean lever: it does
    // not trip `check_machine_connectivity`, which runs first and would mask
    // the failure — a machine still has its output inserter, that inserter
    // just no longer reaches anything. Remove one at a time so the test
    // works regardless of exactly how many lanes the default topology gives
    // this step.
    let mut result = validate(&grid, &plan, &cfg);
    for id in circuit_belts {
        grid.remove(id).unwrap();
        result = validate(&grid, &plan, &cfg);
        if result.is_err() {
            break;
        }
    }

    match result {
        Err(LayoutError::UnderDelivers { recipe, item, .. }) => {
            assert_eq!(recipe, "electronic-circuit");
            assert_eq!(item, "electronic-circuit");
        }
        other => panic!("removing product belts should eventually trip UnderDelivers, got {other:?}"),
    }
}

/// Proves the check is genuinely per-run, not per-tile: a per-tile bug would
/// scale "delivered" with belt length (or inserter count) and pass every
/// block trivially — precisely the failure mode that let #3364 ship. The
/// healthy circuit step's claimed capacity is a small multiple of one lane's
/// throughput, not a number in the hundreds.
#[test]
fn delivered_capacity_is_counted_per_run_not_per_tile() {
    let plan = green_circuit_plan();
    let cfg = default_cfg();
    let grid = generate(&plan, &cfg).unwrap();
    let lane = factorio_solver::layout::lane_throughput(cfg.resolve().unwrap().belt);

    let (lanes, _) = product_claims(&grid, "electronic-circuit");
    // Four product lanes (two machine columns, each landing on both lanes of
    // its own edge belt) at 22.5/s apiece against the plan's 45/s goal.
    assert_eq!(lanes.len(), 4, "expected four claimed product lanes, got {:?}", lanes);
    let delivered = lanes.len() as f64 * lane;
    assert!(
        (10.0..=100.0).contains(&delivered),
        "delivered ({delivered}) should be a handful of lanes' worth ({lane}/lane), not scaled \
         by belt length or inserter count"
    );
}

/// Every distinct `(run anchor, lane)` pair an output inserter for `recipe`
/// claims, and the ids of the belts those inserters actually land on. Rebuilt
/// from public API only (`prototype::lookup`, `drop_lane`) rather than
/// calling into the private `check_delivered_rate`/`run_anchor` this exists
/// to exercise — otherwise a bug shared between the check and this helper
/// could pass both.
fn product_claims(grid: &Grid, recipe: &str) -> (LaneClaims, Vec<EntityId>) {
    let mut lanes = std::collections::HashSet::new();
    // A `HashSet` here, not a `Vec`: two mirrored columns can drop onto
    // opposite lanes of the very same shared edge-belt tile, so the same
    // `belt.id` is a legitimate claim from two different inserters.
    let mut belt_ids = std::collections::HashSet::new();
    for inserter in grid
        .entities()
        .filter(|e| EntityCategory::from_prototype_name(e.prototype_name) == EntityCategory::Inserter)
    {
        let Some((pickup, insert)) = inserter_reach(inserter) else { continue };
        let Some(machine) = grid.get_at(pickup.0, pickup.1) else { continue };
        if machine.recipe.as_deref() != Some(recipe) {
            continue;
        }
        let Some(belt) = grid.get_at(insert.0, insert.1) else { continue };
        if EntityCategory::from_prototype_name(belt.prototype_name) != EntityCategory::Belt {
            continue;
        }
        let inserter_cell = factorio_grid::GridPos { x: inserter.top_left.x, y: inserter.top_left.y };
        let insert_cell = factorio_grid::GridPos { x: insert.0, y: insert.1 };
        let Some(lane) = factorio_solver::layout::drop_lane(inserter_cell, insert_cell) else {
            continue;
        };
        lanes.insert((belt_run_anchor(grid, belt), lane));
        belt_ids.insert(belt.id);
    }
    (lanes, belt_ids.into_iter().collect())
}

/// `(pickup_cell, insert_cell)` for a placed inserter, from its own
/// prototype's North-orientation offsets rotated by its placed direction —
/// the same derivation `validate.rs::inserter_cells` uses internally, redone
/// here so the test doesn't reach into a private module to get it.
fn inserter_reach(inserter: &factorio_grid::PlacedEntity) -> Option<((i32, i32), (i32, i32))> {
    let proto = prototype::lookup(inserter.prototype_name)?;
    let pickup = proto.pickup_position?;
    let insert = proto.insert_position?;
    let turns = inserter.direction.as_u8() / 4;
    let rotate = |(x, y): (f64, f64)| (0..turns).fold((x, y), |(x, y), _| (-y, x));
    let delta = |offset: (f64, f64)| {
        let (dx, dy) = rotate(offset);
        (dx.round() as i32, dy.round() as i32)
    };
    let (px, py) = delta(pickup);
    let (ix, iy) = delta(insert);
    let (x, y) = (inserter.top_left.x, inserter.top_left.y);
    Some(((x + px, y + py), (x + ix, y + iy)))
}

/// The canonical anchor cell of the belt run `belt` belongs to — walking
/// backwards along its own flow axis while the neighbour matches prototype
/// and direction. See `validate/runs.rs::run_anchor`, whose logic this
/// mirrors independently for the same reason `inserter_reach` does.
fn belt_run_anchor(grid: &Grid, belt: &factorio_grid::PlacedEntity) -> (i32, i32) {
    use factorio_blueprint::Direction;
    let (dx, dy) = match belt.direction {
        Direction::North | Direction::South => (0, -1),
        _ => (-1, 0),
    };
    let mut cur = (belt.top_left.x, belt.top_left.y);
    loop {
        let next = (cur.0 + dx, cur.1 + dy);
        match grid.get_at(next.0, next.1) {
            Some(e) if e.prototype_name == belt.prototype_name && e.direction == belt.direction => {
                cur = next;
            }
            _ => return cur,
        }
    }
}

#[test]
fn emitted_blueprint_string_starts_with_the_version_byte_and_decodes() {
    let grid = generate(&green_circuit_plan(), &default_cfg()).unwrap();
    let data = BlueprintData {
        blueprint: Some(factorio_grid::to_blueprint(&grid, Some("green circuits".into()), BLUEPRINT_VERSION)),
        blueprint_book: None,
    };
    let s = factorio_blueprint::encode(&data).unwrap();
    assert!(s.starts_with('0'), "blueprint strings are version-byte prefixed: {}", &s[..1.min(s.len())]);

    let back = factorio_blueprint::decode(&s).unwrap();
    let blueprint = back.blueprint.expect("a blueprint (not a book) went in");
    assert_eq!(blueprint.entities.len(), grid.entity_count());
}

/// The version stamped into every generated blueprint must read back as
/// Factorio 2.0: a lower major would make `from_blueprint` treat our own
/// 2.0-encoded directions as 1.x cardinals and rewrite them on import.
#[test]
fn blueprint_version_reads_back_as_factorio_2() {
    assert_eq!(factorio_major_version(BLUEPRINT_VERSION), 2);

    let grid = generate(&green_circuit_plan(), &default_cfg()).unwrap();
    let blueprint = factorio_grid::to_blueprint(&grid, None, BLUEPRINT_VERSION);
    assert_eq!(blueprint.version, BLUEPRINT_VERSION);
    assert_eq!(factorio_major_version(blueprint.version), 2);
}
