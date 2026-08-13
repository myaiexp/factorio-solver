// Cell placement: turns a sized `CellPlan` into real entities on a `Grid` —
// two machine columns, their inserters, and the belts they reach. `cell.rs`
// already decided how many machines and lanes a cell gets; this module
// decides where they go. Belts run vertically here (`Direction::South`)
// where `rows.rs` ran them horizontally — nothing else in the codebase
// assumes a belt's direction.
use factorio_blueprint::Direction;
use factorio_grid::prototype::effective_size;
use factorio_grid::{Grid, GridPos};

use crate::chain::ProductionStep;
use crate::layout::{CellPlan, CellTopology, LayoutError, ResolvedConfig, Side};

mod helpers;
use helpers::{place_at, place_belt_column, place_gutter_inserter, Role};

mod machine_rows;
use machine_rows::{column_height, machine_row_offset, pole_period};

mod xlayout;
use xlayout::x_layout;
pub use xlayout::cell_width;

/// Cells a placed cell occupies, measured from `origin`. Mirrors `tile::StepExtent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellExtent {
    pub width: u32,
    pub height: u32,
}

/// A machine column's belt-facing gutter: `x` is the gutter tile's own
/// column, `dir` is which way its belts sit from it (`+1` = higher x,
/// `-1` = lower).
#[derive(Clone, Copy)]
struct Gutter {
    x: i32,
    dir: i32,
}

/// One machine column: its own machine count, and its two gutters already
/// resolved to which stream (ingredient/product) each one carries.
struct Column {
    x0: i32,
    machines: u32,
    ingredient: Gutter,
    product: Gutter,
}

/// Places one cell — two machine columns, their inserters, and the belts
/// they reach — with its top-left at `origin`. `edge_left` is false when the
/// cell to the left already placed the shared edge belt.
///
/// `plan.columns` gives the machine count per column. A partly-filled
/// trailing cell is expressed by the caller passing a `CellPlan` whose
/// `columns` it has already reduced; a column of 0 machines places its belts
/// but no machines and no inserters.
pub fn place_cell(
    grid: &mut Grid,
    step: &ProductionStep,
    plan: &CellPlan,
    cfg: &ResolvedConfig,
    topo: &CellTopology,
    origin: GridPos,
    edge_left: bool,
) -> Result<CellExtent, LayoutError> {
    let (mw, mh) = effective_size(step.machine, Direction::North);
    let (e, s) = (topo.edge_belts as i32, topo.spine_belts as i32);

    // X layout, left to right. Each `gutter_*`/`col_*_x0` names a single
    // column; the belt groups span `e` or `s` columns. Computed once by
    // `x_layout` and shifted by `origin.x` here, the same computation
    // `cell_width` uses for its prediction.
    let layout = x_layout(mw, topo, edge_left);
    let gutter_a_left = origin.x + layout.gutter_a_left;
    let col_a_x0 = origin.x + layout.col_a_x0;
    let gutter_a_right = origin.x + layout.gutter_a_right;
    let spine_x0 = origin.x + layout.spine_x0;
    let gutter_b_left = origin.x + layout.gutter_b_left;
    let col_b_x0 = origin.x + layout.col_b_x0;
    let gutter_b_right = origin.x + layout.gutter_b_right;
    let right_edge_x0 = origin.x + layout.right_edge_x0;
    let width = layout.width as u32;
    let d = cfg.pole.supply_area_distance.expect("ResolvedConfig guarantees a supply area");
    let period = pole_period(d, mh);
    let height = column_height(plan.columns.0.max(plan.columns.1), mh, period).max(1);

    // Column A takes from / drops to whichever gutter `ingredients_on`
    // names; column B is the mirror image — spelled out once and
    // parameterised by gutter rather than branched per column.
    let (a_ing, a_prod, b_ing, b_prod) = match topo.ingredients_on {
        Side::Spine => (
            Gutter { x: gutter_a_right, dir: 1 },
            Gutter { x: gutter_a_left, dir: -1 },
            Gutter { x: gutter_b_left, dir: -1 },
            Gutter { x: gutter_b_right, dir: 1 },
        ),
        Side::Edge => (
            Gutter { x: gutter_a_left, dir: -1 },
            Gutter { x: gutter_a_right, dir: 1 },
            Gutter { x: gutter_b_right, dir: 1 },
            Gutter { x: gutter_b_left, dir: -1 },
        ),
    };
    let col_a = Column { x0: col_a_x0, machines: plan.columns.0, ingredient: a_ing, product: a_prod };
    let col_b = Column { x0: col_b_x0, machines: plan.columns.1, ingredient: b_ing, product: b_prod };

    // Ingredient (item, slot) pairs — identical for both columns, since they
    // share the same belts (`cell.rs`: "picking shares a lane"). Order is
    // load-bearing: `lane_allocation` order, then `ingredient_belts`'s own
    // ascending slot order, because it fixes which gutter row each pair
    // lands in.
    let ingredient_slots: Vec<u32> = plan
        .lane_allocation
        .iter()
        .flat_map(|(item, _)| plan.ingredient_belts(item))
        .collect();
    // (physical belt index, filter). `CellPlan::product_belts` addresses the
    // product side by PHYSICAL belt (ascending x across the group), not by
    // either column's own distance-from-gutter — the two are the same only
    // for column A. The filter is `Some(item)` only when the step has two or
    // more products — a single-product step places an unfiltered inserter
    // exactly as before this feature existed, so its blueprint is unchanged
    // to the byte. Two or more products need it: an unfiltered inserter
    // takes whatever sits in the machine's output slot, so two of them on
    // two belts would put a random mix on both (`lane.rs`'s far-lane rule is
    // what makes a *whole* belt per product possible at all, but only a
    // filter makes it actually carry one item).
    let filtered = plan.product_allocation.len() >= 2;
    let product_slots: Vec<(u32, Option<String>)> = plan
        .product_allocation
        .iter()
        .flat_map(|(item, _)| {
            let filter = filtered.then(|| item.clone());
            plan.product_belts(item).into_iter().map(move |p| (p, filter.clone()))
        })
        .collect();
    let n_product_belts = product_slots.len() as u32;

    // Refusals, checked before anything is placed. Both columns need the
    // same room (they place the same pairs, mirrored), so one check covers
    // both; skipped entirely for a cell with no machines in either column.
    if col_a.machines > 0 || col_b.machines > 0 {
        if ingredient_slots.len() as u32 > mh {
            return Err(no_room(step, ingredient_slots.len(), mh));
        }
        if product_slots.len() as u32 > mh {
            return Err(no_room(step, product_slots.len(), mh));
        }
        // A long inserter's insert offset lands 2 tiles into the machine
        // it's mirrored against, so a machine narrower than that would have
        // it inserting into empty ground. No vanilla crafting machine is 1
        // wide — this guards a case the registry never actually reaches.
        let needs_long =
            ingredient_slots.iter().any(|&k| k >= 1) || product_slots.iter().any(|(k, _)| *k >= 1);
        if needs_long && mw < 2 {
            return Err(LayoutError::MachineTooNarrowForLongInserter {
                recipe: step.recipe.name.clone(),
                machine: step.machine.name.clone(),
                width: mw,
            });
        }
    }

    // Belts: the left edge only when this cell owns it, the spine and right
    // edge always — the next cell in a band reuses this one's right edge as
    // its own (unplaced) left edge.
    if edge_left {
        for k in 0..e {
            place_belt_column(grid, origin.x + k, origin.y, height, cfg.belt)?;
        }
    }
    for k in 0..s {
        place_belt_column(grid, spine_x0 + k, origin.y, height, cfg.belt)?;
    }
    for k in 0..e {
        place_belt_column(grid, right_edge_x0 + k, origin.y, height, cfg.belt)?;
    }

    // Machines, then their inserters: one machine per `mh`-row band, one
    // inserter per ingredient/product slot in that band's gutters.
    // `machine_row_offset` folds in a reserved, machine-free row before every
    // `period` machines (see its own doc comment for why: it's the only
    // place a pole can ever land in this topology), so `y` already skips
    // those rows — belts above still run through them uninterrupted.
    for col in [&col_a, &col_b] {
        for i in 0..col.machines {
            let y = origin.y + machine_row_offset(i, mh, period);
            place_at(grid, step.machine, (col.x0, y), Direction::North, Some(step.recipe.name.clone()))?;
            for (j, &slot) in ingredient_slots.iter().enumerate() {
                let at = (col.ingredient.x, y + j as i32);
                place_gutter_inserter(grid, cfg, at, col.ingredient.dir, slot, Role::Ingredient)?;
            }
            for (j, (p, filter)) in product_slots.iter().enumerate() {
                // A column's product gutter faces its belts from one side or
                // the other, so the same physical belt is a different
                // distance-from-gutter for each column: ascending physical x
                // for the `dir > 0` column, descending for `dir < 0`.
                // Left untranslated for an unfiltered (single-product) step —
                // every belt in the group gets one inserter from each column
                // regardless of pairing, so the placement is exactly what
                // this code has always produced, byte for byte. A filtered
                // step cannot afford that: without the translation, the two
                // columns would filter the *same* physical belt for two
                // *different* items — the mixed-belt bug `validate.rs`'s
                // `check_no_mixed_product_belts` exists to catch, caught here
                // instead by construction.
                let slot = if filtered && col.product.dir < 0 { n_product_belts - 1 - *p } else { *p };
                let at = (col.product.x, y + j as i32);
                let id = place_gutter_inserter(grid, cfg, at, col.product.dir, slot, Role::Product)?;
                if let Some(item) = filter {
                    grid.set_filters(id, vec![item.clone()])?;
                }
            }
        }
    }

    Ok(CellExtent { width, height })
}

/// `LayoutError::NoRoomForInserters` against this cell's step, for either
/// refusal `place_cell` can hit — too many inserters for the gutter, or a
/// long inserter with nowhere real to reach.
fn no_room(step: &ProductionStep, needed: usize, edge_tiles: u32) -> LayoutError {
    LayoutError::NoRoomForInserters {
        recipe: step.recipe.name.clone(),
        machine: step.machine.name.clone(),
        needed,
        edge_tiles,
    }
}

#[cfg(test)]
mod tests;
