// Cell placement: turns a sized `CellPlan` into real entities on a `Grid` —
// two machine columns, their inserters, and the belts they reach. `cell.rs`
// already decided how many machines and lanes a cell gets; this module
// decides where they go. Belts run vertically here (`Direction::South`)
// where `rows.rs` ran them horizontally — nothing else in the codebase
// assumes a belt's direction.
use factorio_blueprint::Direction;
use factorio_grid::prototype::{effective_size, EntityPrototype};
use factorio_grid::{Grid, GridPos};

use crate::chain::ProductionStep;
use crate::layout::{CellPlan, CellTopology, LayoutError, ResolvedConfig, Side};

mod helpers;
use helpers::{place_at, place_belt_column, place_gutter_inserter, Role};

/// Cells a placed cell occupies, measured from `origin`. Mirrors `tile::StepExtent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellExtent {
    pub width: u32,
    pub height: u32,
}

/// Every x boundary of a cell's column layout, relative to the cell's own
/// `x = 0` (before `origin.x` is added). `cell_width` and `place_cell` both
/// derive from this single computation, so a tiler's width prediction and
/// the placement it predicts can never drift apart.
struct XLayout {
    gutter_a_left: i32,
    col_a_x0: i32,
    gutter_a_right: i32,
    spine_x0: i32,
    gutter_b_left: i32,
    col_b_x0: i32,
    gutter_b_right: i32,
    right_edge_x0: i32,
    width: i32,
}

fn x_layout(mw: u32, topo: &CellTopology, edge_left: bool) -> XLayout {
    let (e, s) = (topo.edge_belts as i32, topo.spine_belts as i32);
    let gutter_a_left = if edge_left { e } else { 0 };
    let col_a_x0 = gutter_a_left + 1;
    let gutter_a_right = col_a_x0 + mw as i32;
    let spine_x0 = gutter_a_right + 1;
    let gutter_b_left = spine_x0 + s;
    let col_b_x0 = gutter_b_left + 1;
    let gutter_b_right = col_b_x0 + mw as i32;
    let right_edge_x0 = gutter_b_right + 1;
    let width = right_edge_x0 + e;
    XLayout {
        gutter_a_left,
        col_a_x0,
        gutter_a_right,
        spine_x0,
        gutter_b_left,
        col_b_x0,
        gutter_b_right,
        right_edge_x0,
        width,
    }
}

/// The width `place_cell` will occupy, before it places anything — so a
/// tiler can decide where a band wraps without a trial placement.
pub fn cell_width(machine: &EntityPrototype, topo: &CellTopology, edge_left: bool) -> u32 {
    let (mw, _) = effective_size(machine, Direction::North);
    x_layout(mw, topo, edge_left).width as u32
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

/// Machines between two consecutive reserved pole rows, derived from the
/// configured pole rather than hardcoded: a 1x1 pole at row `p` covers rows
/// `p - floor(d) ..= p + floor(d)` (`d` = `supply_area_distance`, the supply
/// *half*-width), so poles `2 * floor(d)` rows apart have abutting — not
/// gapped — coverage. Dividing that span by `mh` gives how many machine rows
/// fit between them. `.max(1)` keeps a pole reaching less than one machine's
/// height from producing a zero period and a division by it below.
fn pole_period(supply_area_distance: f64, mh: u32) -> u32 {
    (2 * supply_area_distance.floor() as u32 / mh).max(1)
}

/// Row (from a column's own y = 0) where machine `i` (0-based) starts: `mh`
/// rows per machine, plus one reserved, machine-free, inserter-free row
/// before every group of `period` machines — including the very first.
///
/// The reserved row exists because a *vertical* column of poles cannot be
/// inserted anywhere in this cell: every column is load-bearing. An
/// inserter must sit directly beside the machine it serves, and a belt must
/// sit at exactly slot-0 or slot-1 distance from its gutter — nothing can be
/// shifted aside to free a column for a pole without breaking reach. A
/// horizontal row cut straight through the machine columns is the only
/// place `power::place_poles` can ever stand one, so this function reserves
/// it up front rather than leaving placement to hope one turns up later.
fn machine_row_offset(i: u32, mh: u32, period: u32) -> i32 {
    (i * mh + 1 + i / period) as i32
}

/// The column height that fits `machines` machines laid out by
/// `machine_row_offset` — computed from that same expression (the last
/// machine's own offset, plus its height) so sizing and placement read from
/// one definition and can never drift apart. Zero machines need zero rows;
/// the caller still applies its own `.max(1)` floor for an entirely empty cell.
fn column_height(machines: u32, mh: u32, period: u32) -> u32 {
    match machines.checked_sub(1) {
        Some(last) => (machine_row_offset(last, mh, period) + mh as i32) as u32,
        None => 0,
    }
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
    let product_slots = topo.product_belts();

    // Refusals, checked before anything is placed. Both columns need the
    // same room (they place the same pairs, mirrored), so one check covers
    // both; skipped entirely for a cell with no machines in either column.
    if col_a.machines > 0 || col_b.machines > 0 {
        if ingredient_slots.len() as u32 > mh {
            return Err(no_room(step, ingredient_slots.len(), mh));
        }
        if product_slots > mh {
            return Err(no_room(step, product_slots as usize, mh));
        }
        // A long inserter's insert offset lands 2 tiles into the machine
        // it's mirrored against, so a machine narrower than that would have
        // it inserting into empty ground. No vanilla crafting machine is 1
        // wide — this guards a case the registry never actually reaches.
        let needs_long = ingredient_slots.iter().any(|&k| k >= 1) || product_slots >= 2;
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
            for slot in 0..product_slots {
                let at = (col.product.x, y + slot as i32);
                place_gutter_inserter(grid, cfg, at, col.product.dir, slot, Role::Product)?;
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
