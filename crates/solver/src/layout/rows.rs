// Machine rows: one step's machines, their inserters, and the belts those
// inserters reach.
//
// Geometry only — no search. Each sub-row is a fixed-height band (input
// belt, input inserters, machines, output inserters, output belt) and
// sub-rows stack downward separated by `STEP_GAP` blank rows. A step needs
// more than one sub-row exactly when its ingredient/product rate needs more
// belts than a single row's inserters can reach (see `lanes::pack_lanes`).
use factorio_grid::prototype::{effective_size, EntityPrototype};
use factorio_grid::{Grid, GridPos};

use factorio_blueprint::{Direction, Position};

use crate::chain::{ItemRate, ProductionStep};
use crate::layout::{lanes::pack_lanes, LayoutError, ResolvedConfig, STEP_GAP};

/// Cells a placed step occupies, measured from its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepExtent {
    pub width: u32,
    pub height: u32,
}

/// The four cardinals a layout ever rotates an inserter to. North first
/// purely so the common (unrotated) `inserter`/`fast-inserter` case is
/// found on the first try.
const CARDINALS: [Direction; 4] =
    [Direction::North, Direction::East, Direction::South, Direction::West];

/// Rotate a centre-relative offset clockwise by `turns` quarter turns.
/// `(x, y) -> (-y, x)` is one turn — check: North's `(0, -1)` becomes
/// East's `(1, 0)`, matching the dump's own direction convention.
fn rotate(offset: (f64, f64), turns: u8) -> (f64, f64) {
    (0..turns).fold(offset, |(x, y), _| (-y, x))
}

/// A rotated centre-relative offset, converted to an integer cell delta.
fn to_delta(offset: (f64, f64)) -> (i32, i32) {
    (offset.0.round() as i32, offset.1.round() as i32)
}

/// Which cardinal direction makes `proto`'s pickup/insert offsets land on
/// the two cell deltas asked for (offsets from the inserter's own cell).
///
/// Derived from the prototype's own North-orientation `pickup_position` /
/// `insert_position` rather than assumed, so swapping in a different
/// inserter prototype cannot silently produce a backwards inserter.
fn inserter_direction(
    proto: &EntityPrototype,
    pickup_delta: (i32, i32),
    insert_delta: (i32, i32),
) -> Result<Direction, LayoutError> {
    let unknown = || LayoutError::InserterUnknown(proto.name.clone());
    let pickup = proto.pickup_position.ok_or_else(unknown)?;
    let insert = proto.insert_position.ok_or_else(unknown)?;

    CARDINALS
        .into_iter()
        .find(|&dir| {
            let turns = dir.as_u8() / 4;
            to_delta(rotate(pickup, turns)) == pickup_delta
                && to_delta(rotate(insert, turns)) == insert_delta
        })
        .ok_or_else(unknown)
}

/// Place `proto` with its top-left at `top_left`. `Grid::place` wants a
/// centre; this is the one place that does the top-left -> centre
/// conversion, using the entity's size as rotated by `direction`.
fn place_at(
    grid: &mut Grid,
    proto: &EntityPrototype,
    top_left: (i32, i32),
    direction: Direction,
    recipe: Option<String>,
) -> Result<(), LayoutError> {
    let (w, h) = effective_size(proto, direction);
    let center = Position {
        x: top_left.0 as f64 + w as f64 / 2.0,
        y: top_left.1 as f64 + h as f64 / 2.0,
    };
    grid.place(&proto.name, &center, direction, recipe, None)?;
    Ok(())
}

/// One belt entity per cell from `left` to `left + width - 1` on row `y`,
/// all facing East.
fn place_belt_row(
    grid: &mut Grid,
    left: i32,
    y: i32,
    width: u32,
    belt: &EntityPrototype,
) -> Result<(), LayoutError> {
    for dx in 0..width as i32 {
        place_at(grid, belt, (left + dx, y), Direction::East, None)?;
    }
    Ok(())
}

pub fn place_step(
    grid: &mut Grid,
    step: &ProductionStep,
    cfg: &ResolvedConfig,
    origin: GridPos,
) -> Result<StepExtent, LayoutError> {
    let (mw, mh) = effective_size(step.machine, Direction::North);

    let ins: Vec<ItemRate> = step.inputs.iter().filter(|r| r.per_sec > 0.0).cloned().collect();
    let outs: Vec<ItemRate> = step.outputs.iter().filter(|r| r.per_sec > 0.0).cloned().collect();

    // Refusals, checked before anything is placed.
    //
    // Fluids first: it is the more fundamental reason, and it is the one that
    // would otherwise pass silently. `chain::solve` only rejects a fluid
    // *ingredient* that is off the bus — a fluid the user declared available,
    // or any fluid a recipe produces, arrives here looking like a normal item
    // rate and would be belted.
    for (rates, amounts) in [(&ins, &step.recipe.ingredients), (&outs, &step.recipe.results)] {
        if let Some(fluid) = rates.iter().find(|r| {
            amounts.iter().any(|a| a.name == r.item && a.kind == crate::recipe::ItemKind::Fluid)
        }) {
            return Err(LayoutError::FluidOnBelt {
                recipe: step.recipe.name.clone(),
                item: fluid.item.clone(),
            });
        }
    }

    if ins.len() > 2 {
        return Err(LayoutError::TooManyIngredientsForLanes {
            recipe: step.recipe.name.clone(),
            ingredients: ins.iter().map(|r| r.item.clone()).collect(),
            lanes: 2,
        });
    }
    if outs.len() > 2 {
        return Err(LayoutError::MultipleProducts {
            recipe: step.recipe.name.clone(),
            products: outs.iter().map(|r| r.item.clone()).collect(),
        });
    }
    if ins.len() as u32 > mw {
        return Err(LayoutError::NoRoomForInserters {
            recipe: step.recipe.name.clone(),
            machine: step.machine.name.clone(),
            needed: ins.len(),
            width: mw,
        });
    }
    if outs.len() as u32 > mw {
        return Err(LayoutError::NoRoomForInserters {
            recipe: step.recipe.name.clone(),
            machine: step.machine.name.clone(),
            needed: outs.len(),
            width: mw,
        });
    }

    let in_belts = pack_lanes(&ins, cfg.belt);
    let out_belts = pack_lanes(&outs, cfg.belt);

    let machines = if step.machines_needed == 0 { 1 } else { step.machines_needed } as usize;
    let sub_rows = in_belts.len().max(out_belts.len()).max(1).min(machines);

    // Evenly split `machines` across `sub_rows`: the first `rem` rows carry
    // one extra machine so none are left over.
    let base = machines / sub_rows;
    let rem = machines % sub_rows;
    let per_sub_row: Vec<usize> = (0..sub_rows).map(|s| base + usize::from(s < rem)).collect();

    // Every belt spans the full step width so shorter sub-rows still line
    // up with the widest one.
    let step_width = per_sub_row.iter().map(|&m| m as u32 * mw).max().unwrap_or(0);

    // Both inserter rows pick up from the cell above and insert into the
    // cell below (see the doc comment on `inserter_direction`): the input
    // inserter sits under the belt and over the machine, the output
    // inserter sits under the machine and over the belt.
    let inserter_dir = inserter_direction(cfg.inserter, (0, -1), (0, 1))?;

    for (s, &count) in per_sub_row.iter().enumerate() {
        let band_top = origin.y + s as i32 * (mh as i32 + 4 + STEP_GAP);
        let in_row = band_top + 1;
        let machine_row = band_top + 2;
        let out_row = band_top + 2 + mh as i32;
        let out_belt_row = band_top + 3 + mh as i32;

        if !ins.is_empty() {
            place_belt_row(grid, origin.x, band_top, step_width, cfg.belt)?;
        }
        if !outs.is_empty() {
            place_belt_row(grid, origin.x, out_belt_row, step_width, cfg.belt)?;
        }

        for i in 0..count {
            let machine_left = origin.x + i as i32 * mw as i32;
            place_at(
                grid,
                step.machine,
                (machine_left, machine_row),
                Direction::North,
                Some(step.recipe.name.clone()),
            )?;

            for k in 0..ins.len() {
                place_at(grid, cfg.inserter, (machine_left + k as i32, in_row), inserter_dir, None)?;
            }
            for k in 0..outs.len() {
                place_at(grid, cfg.inserter, (machine_left + k as i32, out_row), inserter_dir, None)?;
            }
        }
    }

    let height = sub_rows as u32 * (mh + 4) + (sub_rows as u32 - 1) * STEP_GAP as u32;
    Ok(StepExtent { width: step_width, height })
}

#[cfg(test)]
mod tests;
