// Test-module root for `place_cell`, holding the one helper both halves
// need: `placed_inserters`, which re-derives every inserter's reach from the
// PLACED entity's own prototype data and direction rather than from what the
// code intended to place. Everything below is read back off the grid for
// that reason — a bug shared between placement and its check would pass both.
//
// `geometry` is the far-lane invariant and the block's shape; `filters` is
// the multi-product case, where two products share a belt group and the
// question is which physical belt each one lands on.
use super::helpers::{rotate, to_delta};
use super::*;
use factorio_grid::prototype;

mod filters;
mod geometry;

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
