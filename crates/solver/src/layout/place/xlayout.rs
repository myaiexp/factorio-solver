// X-axis geometry for one cell: gutter/spine/edge column boundaries, and the
// width they add up to. Split out of `place.rs` purely to keep that file to
// entity placement; `cell_width` and `place_cell` both derive from the same
// `x_layout` computation so a tiler's width prediction and the placement it
// predicts can never drift apart.
use factorio_blueprint::Direction;
use factorio_grid::prototype::{effective_size, EntityPrototype};

use crate::layout::CellTopology;

/// Every x boundary of a cell's column layout, relative to the cell's own
/// `x = 0` (before `origin.x` is added).
pub(super) struct XLayout {
    pub(super) gutter_a_left: i32,
    pub(super) col_a_x0: i32,
    pub(super) gutter_a_right: i32,
    pub(super) spine_x0: i32,
    pub(super) gutter_b_left: i32,
    pub(super) col_b_x0: i32,
    pub(super) gutter_b_right: i32,
    pub(super) right_edge_x0: i32,
    pub(super) width: i32,
}

pub(super) fn x_layout(mw: u32, topo: &CellTopology, edge_left: bool) -> XLayout {
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
