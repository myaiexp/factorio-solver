// Multi-product filtering: that the two machine columns' independent
// physical-belt addressing agrees rather than mirrors, both within one cell
// and across the edge belt two neighbouring cells share.
use std::collections::{HashMap, HashSet};

use super::*;

use crate::layout::size_step;
use crate::testsupport::{default_cfg, green_circuit_plan, plan_containing_kovarex, step};

/// A two-product cell's output inserters, filter included, keyed by which
/// physical belt **column** (`insert.x`) they insert into — a cell's belts
/// run vertically, so one x is one physical run.
///
/// Keyed on the column and NOT on the insert `GridPos`, and not on the drop
/// lane. Both narrower keys were measured letting the mirror bug through:
/// the two columns' inserters sit in different *rows* of the same gutter, so
/// they drop their different products onto the same belt at different tiles
/// and on different lanes. A per-tile or per-lane key therefore sees one
/// filter everywhere and the whole suite passes green with the bug present.
/// Same trap as #3364 — capacity per tile instead of per run.
fn product_filters_by_belt(grid: &Grid, machine: &str) -> HashMap<i32, HashSet<String>> {
    let mut by_belt: HashMap<i32, HashSet<String>> = HashMap::new();
    for (cell, pickup, insert, _name) in placed_inserters(grid) {
        if grid.get_at(pickup.x, pickup.y).map(|p| p.prototype_name) != Some(machine) {
            continue; // an ingredient inserter: it picks up from the belt, not the machine
        }
        let filters = &grid.get_at(cell.x, cell.y).unwrap().filters;
        assert_eq!(filters.len(), 1, "a two-product cell's output inserter must carry exactly one filter");
        by_belt.entry(insert.x).or_default().insert(filters[0].clone());
    }
    by_belt
}

/// The property the whole filtered-belt design rests on: every PHYSICAL
/// product belt cell carries exactly one item's filter, even though both
/// machine columns place inserters into the same belt group. A design
/// review caught a version of this placement code that translated
/// slot-to-belt addressing for one column but not its mirror, which this
/// test would have caught directly — it would have found one physical belt
/// claimed by two different filters. `sized.columns` is forced to `(2, 2)`
/// rather than trusted from `plan_containing_kovarex()`'s own arithmetic, so
/// the test deterministically exercises both columns regardless of what
/// rate that fixture happens to produce.
#[test]
fn each_physical_product_belt_carries_exactly_one_filter() {
    let plan = plan_containing_kovarex();
    let processing = step(&plan, "uranium-processing");
    let topo = CellTopology { ingredients_on: Side::Edge, ..CellTopology::default() };
    let cfg = default_cfg().resolve().unwrap();
    let mut sized = size_step(processing, cfg.belt, &topo).unwrap();
    sized.columns = (2, 2);

    let mut grid = Grid::new();
    place_cell(&mut grid, processing, &sized, &cfg, &topo, GridPos { x: 0, y: 0 }, true).unwrap();

    let by_belt = product_filters_by_belt(&grid, "centrifuge");
    assert!(!by_belt.is_empty(), "the fixture must actually place output inserters");
    for (belt, items) in &by_belt {
        assert_eq!(items.len(), 1, "belt {belt:?} is claimed by more than one filter: {items:?}");
    }
    let all_items: HashSet<&String> = by_belt.values().flatten().collect();
    assert_eq!(all_items.len(), 2, "both products should appear, on different belts: {by_belt:?}");
}

/// The shared-edge variant of the same mirror: with products on the edge
/// side, cell N's column B and cell N+1's column A face the SAME shared
/// edge belt group from opposite directions — the physical belts a
/// two-cell step's neighbouring cells hand off to each other, not just the
/// two columns of a single cell. `sized.columns` is again forced to
/// `(2, 2)` so both cells exercise both columns deterministically.
#[test]
fn a_shared_edge_belt_between_two_product_cells_is_not_mixed() {
    let plan = plan_containing_kovarex();
    let processing = step(&plan, "uranium-processing");
    let topo = CellTopology { spine_belts: 1, edge_belts: 2, ingredients_on: Side::Spine, target_width: None };
    let cfg = default_cfg().resolve().unwrap();
    let mut sized = size_step(processing, cfg.belt, &topo).unwrap();
    sized.columns = (2, 2);

    let mut grid = Grid::new();
    let first =
        place_cell(&mut grid, processing, &sized, &cfg, &topo, GridPos { x: 0, y: 0 }, true).unwrap();
    place_cell(&mut grid, processing, &sized, &cfg, &topo, GridPos { x: first.width as i32, y: 0 }, false)
        .unwrap();

    let by_belt = product_filters_by_belt(&grid, "centrifuge");
    assert!(!by_belt.is_empty(), "the fixture must actually place output inserters");
    for (belt, items) in &by_belt {
        assert_eq!(items.len(), 1, "belt {belt:?} is claimed by more than one filter: {items:?}");
    }
}

/// A single-product cell places every inserter unfiltered — byte-identical
/// to the placement this code produced before multi-product cells existed.
#[test]
fn a_single_product_cell_places_no_filters() {
    let plan = green_circuit_plan();
    let topo = CellTopology::default();
    let cfg = default_cfg().resolve().unwrap();
    let circuit = step(&plan, "electronic-circuit");
    let sized = size_step(circuit, cfg.belt, &topo).unwrap();

    let mut grid = Grid::new();
    place_cell(&mut grid, circuit, &sized, &cfg, &topo, GridPos { x: 0, y: 0 }, true).unwrap();

    assert!(
        grid.entities().all(|e| e.filters.is_empty()),
        "a single-product cell must place every inserter unfiltered"
    );
}
