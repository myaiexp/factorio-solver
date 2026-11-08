// Ingredient-side addressing: both machine columns of a cell must put their
// ingredient arms on the same PHYSICAL belts, even though they face the
// ingredient group from opposite sides.
use std::collections::{BTreeMap, HashSet};

use super::*;

use crate::layout::size_step;
use crate::testsupport::{default_cfg, green_circuit_plan, step};

/// Ingredient inserters (those picking up from a belt, not from `machine`),
/// grouped by the machine column they insert into (`insert.x`'s machine's
/// top-left x), each as a count of arms per physical belt column
/// (`pickup.x`). Keyed by belt column, not pickup tile, for the reason
/// `filters.rs` gives: the two columns' arms sit in different rows.
fn ingredient_arms_per_belt(grid: &Grid, machine: &str) -> Vec<BTreeMap<i32, u32>> {
    let mut by_column: BTreeMap<i32, BTreeMap<i32, u32>> = BTreeMap::new();
    for (_cell, pickup, insert, _name) in placed_inserters(grid) {
        let Some(target) = grid.get_at(insert.x, insert.y) else { continue };
        if target.prototype_name != machine {
            continue; // an output inserter: it inserts into a belt
        }
        *by_column.entry(target.top_left.x).or_default().entry(pickup.x).or_default() += 1;
    }
    by_column.into_values().collect()
}

/// Green circuits on the default topology spill copper cable across both
/// spine belts (3 lanes: belt 0 whole, belt 1 shared with iron plate). The
/// sizing assumes both columns draw identically, so both must place the same
/// number of arms on each physical belt. Before physical addressing, column
/// B read the slot list mirrored and put 2 arms on the belt column A gave 1.
#[test]
fn both_columns_put_the_same_arms_on_each_ingredient_belt() {
    let plan = green_circuit_plan();
    let circuit = step(&plan, "electronic-circuit");
    let cfg = default_cfg().resolve().unwrap();
    for topo in [
        CellTopology::default(),
        CellTopology { spine_belts: 1, edge_belts: 2, ingredients_on: Side::Edge, target_width: None },
    ] {
        let mut sized = size_step(circuit, cfg.belt, &topo).unwrap();
        sized.columns = (2, 2);
        let mut grid = Grid::new();
        place_cell(&mut grid, circuit, &sized, &cfg, &topo, GridPos { x: 0, y: 0 }, true).unwrap();

        let per_column = ingredient_arms_per_belt(&grid, circuit.machine.name.as_str());
        assert_eq!(per_column.len(), 2, "{topo:?}: expected two machine columns, got {per_column:?}");
        // On the spine both columns pick from the very same belts; on the
        // edge each picks from its own edge group, so only the per-belt
        // counts in ascending x can be compared.
        if topo.ingredients_on == Side::Spine {
            let belts: Vec<HashSet<i32>> =
                per_column.iter().map(|m| m.keys().copied().collect()).collect();
            assert_eq!(belts[0], belts[1], "{topo:?}: the columns reach different belts");
        }
        let counts: Vec<Vec<u32>> = per_column.iter().map(|m| m.values().copied().collect()).collect();
        assert_eq!(counts[0], counts[1], "{topo:?}: arms per physical belt differ: {per_column:?}");
    }
}
