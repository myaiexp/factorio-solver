// Geometry tests for `place_cell`: the far-lane invariant `lane.rs` states
// but Phase 6 never checked, shared-belt lane ownership between adjacent
// cells, inserter-tier selection by reach, containment/no-overlap at a
// negative origin, both named refusals, a partly-filled trailing cell, and
// the topology flip that moves products onto the spine.
use std::collections::HashSet;

use super::helpers::{rotate, to_delta};
use super::*;
use factorio_grid::prototype;

use crate::layout::{drop_lane, size_step, LaneSide};
use crate::testsupport::{default_cfg, green_circuit_plan, hand_step, rate, step};

/// Every placed inserter's own cell, its pickup cell, its insert cell — all
/// three derived from the PLACED entity's own prototype data and direction,
/// never from what the code intended to place — plus its prototype name.
fn placed_inserters(grid: &Grid) -> Vec<(GridPos, GridPos, GridPos, &'static str)> {
    grid.entities()
        .filter_map(|e| {
            let proto = prototype::lookup(e.prototype_name)?;
            let pickup = proto.pickup_position?;
            let insert = proto.insert_position?;
            let turns = e.direction.as_u8() / 4;
            let pickup_delta = to_delta(rotate(pickup, turns));
            let insert_delta = to_delta(rotate(insert, turns));
            let cell = e.top_left;
            Some((
                cell,
                GridPos { x: cell.x + pickup_delta.0, y: cell.y + pickup_delta.1 },
                GridPos { x: cell.x + insert_delta.0, y: cell.y + insert_delta.1 },
                e.prototype_name,
            ))
        })
        .collect()
}

/// Every (belt cell, lane) an output inserter claims, asserting on the way
/// that the far lane exists and that no two inserters claim the same one.
/// Two inserters dropping on one lane is the half-rate bug this whole
/// topology exists to prevent, so it is checked from the placed grid rather
/// than trusted from the sizing that decided the placement.
fn claimed_lanes(grid: &Grid, belt_name: &str) -> HashSet<(GridPos, LaneSide)> {
    let mut claimed: HashSet<(GridPos, LaneSide)> = HashSet::new();
    for (cell, _pickup, insert, _name) in placed_inserters(grid) {
        if grid.get_at(insert.x, insert.y).map(|p| p.prototype_name) != Some(belt_name) {
            continue; // an ingredient inserter: it inserts into the machine
        }
        let lane = drop_lane(cell, insert).expect("an inserter dropping onto a belt has a far lane");
        assert!(claimed.insert((insert, lane)), "lane {lane:?} of belt {insert:?} claimed twice");
    }
    claimed
}

/// The assertion Phase 6 lacked, and the reason this module exists.
#[test]
fn every_output_inserter_claims_the_far_lane_and_no_lane_is_claimed_twice() {
    let plan = green_circuit_plan();
    let topo = CellTopology::default();
    let cfg = default_cfg().resolve().unwrap();
    let circuit = step(&plan, "electronic-circuit");
    let sized = size_step(circuit, cfg.belt, &topo).unwrap();

    let mut grid = Grid::new();
    place_cell(&mut grid, circuit, &sized, &cfg, &topo, GridPos { x: 0, y: 0 }, true).unwrap();

    let claimed = claimed_lanes(&grid, &cfg.belt.name);
    // One product lane per machine on the default topology's single edge belt.
    assert_eq!(claimed.len() as u32, sized.columns.0 + sized.columns.1);
}

/// Two adjacent cells share an edge belt column; both its lanes must be
/// claimed, by inserters sitting on opposite sides of it.
#[test]
fn both_lanes_of_a_shared_belt_are_claimed_by_opposite_columns() {
    let plan = green_circuit_plan();
    let topo = CellTopology::default();
    let cfg = default_cfg().resolve().unwrap();
    let circuit = step(&plan, "electronic-circuit");
    let sized = size_step(circuit, cfg.belt, &topo).unwrap();

    let mut grid = Grid::new();
    let first =
        place_cell(&mut grid, circuit, &sized, &cfg, &topo, GridPos { x: 0, y: 0 }, true).unwrap();
    place_cell(&mut grid, circuit, &sized, &cfg, &topo, GridPos { x: first.width as i32, y: 0 }, false)
        .unwrap();

    let shared_x = first.width as i32 - topo.edge_belts as i32;
    let mut lanes: HashSet<LaneSide> = HashSet::new();
    let mut sides: HashSet<i32> = HashSet::new();
    for (cell, _pickup, insert, _name) in placed_inserters(&grid) {
        if insert.x != shared_x {
            continue;
        }
        if grid.get_at(insert.x, insert.y).map(|p| p.prototype_name) != Some(cfg.belt.name.as_str()) {
            continue;
        }
        lanes.insert(drop_lane(cell, insert).expect("far lane must exist"));
        sides.insert(cell.x);
    }
    assert_eq!(lanes.len(), 2, "both lanes of the shared belt must be claimed");
    assert_eq!(sides.len(), 2, "the two claims must come from inserters on opposite sides");
}

/// Default topology's 2-belt spine: the nearer belt is served by the short
/// inserter, the farther one needs the long-handed inserter.
#[test]
fn a_long_inserter_serves_the_far_belt_of_a_pair() {
    let plan = green_circuit_plan();
    let topo = CellTopology::default();
    assert_eq!(topo.spine_belts, 2, "this test exercises the spine pair");
    let cfg = default_cfg().resolve().unwrap();
    let circuit = step(&plan, "electronic-circuit");
    let sized = size_step(circuit, cfg.belt, &topo).unwrap();

    let mut grid = Grid::new();
    place_cell(&mut grid, circuit, &sized, &cfg, &topo, GridPos { x: 0, y: 0 }, true).unwrap();

    let (mut short_seen, mut long_seen) = (false, false);
    for (cell, pickup, _insert, name) in placed_inserters(&grid) {
        if grid.get_at(pickup.x, pickup.y).map(|p| p.prototype_name) != Some(cfg.belt.name.as_str()) {
            continue; // a product inserter: it picks up from the machine, not a belt
        }
        match (pickup.x - cell.x).abs() {
            1 => {
                assert_eq!(name, cfg.inserter.name, "the inner belt should use the short inserter");
                short_seen = true;
            }
            2 => {
                assert_eq!(name, cfg.long_inserter.name, "the outer belt should use the long inserter");
                long_seen = true;
            }
            other => panic!("unexpected inserter reach {other}"),
        }
    }
    assert!(short_seen && long_seen, "both inserter tiers should appear on copper-cable's shared belt");
}

/// Every placed entity must fall inside `[origin, origin + extent)`, with
/// `get_at` agreeing on who claims every cell, at both a positive and a
/// negative origin — the grid is unbounded and signed.
#[test]
fn machines_do_not_overlap_and_the_extent_bounds_them() {
    let plan = green_circuit_plan();
    let topo = CellTopology::default();
    let cfg = default_cfg().resolve().unwrap();
    let circuit = step(&plan, "electronic-circuit");
    let sized = size_step(circuit, cfg.belt, &topo).unwrap();

    for (origin, edge_left) in [(GridPos { x: 0, y: 0 }, true), (GridPos { x: -41, y: -23 }, false)] {
        let mut grid = Grid::new();
        let extent = place_cell(&mut grid, circuit, &sized, &cfg, &topo, origin, edge_left).unwrap();

        for e in grid.entities() {
            for (cx, cy) in e.cells() {
                assert!(
                    cx >= origin.x && cx < origin.x + extent.width as i32,
                    "{} cell ({cx},{cy}) falls outside the extent from {origin:?}",
                    e.prototype_name
                );
                assert!(
                    cy >= origin.y && cy < origin.y + extent.height as i32,
                    "{} cell ({cx},{cy}) falls outside the extent from {origin:?}",
                    e.prototype_name
                );
                assert_eq!(
                    grid.get_at(cx, cy).map(|p| p.id),
                    Some(e.id),
                    "get_at disagrees with the entity claiming ({cx},{cy})"
                );
            }
        }

        let machines = grid.entities().filter(|e| e.prototype_name == "assembling-machine-2").count();
        assert_eq!(machines as u32, sized.columns.0 + sized.columns.1);
    }
}

/// A hand-built recipe whose 4 ingredients fit the topology's 4 lanes but
/// need more gutter tiles than `assembling-machine-2`'s 3-tile edge has.
#[test]
fn a_machine_too_narrow_for_its_streams_is_a_named_error() {
    let topo = CellTopology { spine_belts: 2, edge_belts: 2, ..CellTopology::default() };
    let cfg = default_cfg().resolve().unwrap();
    let hstep = hand_step(
        "iron-plate",
        1,
        vec![rate("a", 1.0), rate("b", 1.0), rate("c", 1.0), rate("d", 1.0)],
        vec![rate("electronic-circuit", 1.0)],
    );
    let sized = size_step(&hstep, cfg.belt, &topo).expect("4 ingredients fit in 4 lanes");
    assert!(sized.columns.0 + sized.columns.1 >= 1, "the case only bites once a machine exists");

    let mut grid = Grid::new();
    match place_cell(&mut grid, &hstep, &sized, &cfg, &topo, GridPos { x: 0, y: 0 }, true) {
        Err(LayoutError::NoRoomForInserters { recipe, machine, needed, edge_tiles }) => {
            assert_eq!(recipe, "iron-plate");
            assert_eq!(machine, "assembling-machine-2");
            assert!(needed as u32 > edge_tiles, "{needed} inserters needed but only {edge_tiles} tiles");
        }
        other => panic!("expected NoRoomForInserters, got {other:?}"),
    }
    assert_eq!(grid.entity_count(), 0, "nothing may be placed before the refusal");
}

/// `columns: (n, 0)` — a trailing cell with only its first column filled —
/// places cleanly: belts and column A only, no panic on the empty column B.
#[test]
fn a_trailing_cell_with_an_empty_second_column_still_places() {
    let plan = green_circuit_plan();
    let topo = CellTopology::default();
    let cfg = default_cfg().resolve().unwrap();
    let circuit = step(&plan, "electronic-circuit");
    let mut sized = size_step(circuit, cfg.belt, &topo).unwrap();
    sized.columns = (3, 0);

    let mut grid = Grid::new();
    let extent =
        place_cell(&mut grid, circuit, &sized, &cfg, &topo, GridPos { x: 0, y: 0 }, true).unwrap();

    let machines = grid.entities().filter(|e| e.prototype_name == "assembling-machine-2").count();
    assert_eq!(machines, 3);
    let (_, mh) = effective_size(circuit.machine, Direction::North);
    assert_eq!(extent.height, 3 * mh);
}

/// `Side::Edge` swaps which gutter carries which stream: products move from
/// the edge onto the spine, and both spine lanes end up claimed by the two
/// columns dropping onto them from opposite sides.
#[test]
fn flipping_ingredients_to_the_edge_moves_the_products_to_the_spine() {
    let plan = green_circuit_plan();
    let topo = CellTopology { edge_belts: 1, ingredients_on: Side::Edge, ..CellTopology::default() };
    let cfg = default_cfg().resolve().unwrap();
    let circuit = step(&plan, "electronic-circuit");
    let sized = size_step(circuit, cfg.belt, &topo).unwrap();

    let mut grid = Grid::new();
    let extent =
        place_cell(&mut grid, circuit, &sized, &cfg, &topo, GridPos { x: 0, y: 0 }, true).unwrap();

    // Uniqueness holds for the flipped topology too, and here it is the
    // sharper claim: both columns drop onto the *same* spine belts, so a
    // mirroring slip would have them fighting over one lane.
    let claimed = claimed_lanes(&grid, &cfg.belt.name);
    let e = topo.edge_belts as i32;
    let mut spine_lanes: HashSet<LaneSide> = HashSet::new();
    let mut edge_products = 0;
    for (belt, lane) in &claimed {
        if belt.x < e || belt.x >= extent.width as i32 - e {
            edge_products += 1;
        } else {
            spine_lanes.insert(*lane);
        }
    }
    assert_eq!(edge_products, 0, "products should land on the spine, not the edge, once flipped");
    assert_eq!(spine_lanes.len(), 2, "both spine lanes should be claimed");
    // Each machine owns one lane on each of the two spine belts.
    assert_eq!(claimed.len() as u32, 2 * (sized.columns.0 + sized.columns.1));
}
