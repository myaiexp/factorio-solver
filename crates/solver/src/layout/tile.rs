// Cell tiling: turns one step's sized `CellPlan` into a left-to-right,
// top-to-bottom arrangement of `place_cell` calls. `cell::size_step` decides
// how many machines fit in one cell and how many cells the step needs; this
// module distributes those machines across cells, splits each cell's own
// count into two columns, and wraps into a new band once `topo.target_width`
// says the current one is full.
use factorio_grid::{Grid, GridPos};

use crate::chain::ProductionStep;
use crate::layout::{cell_width, place_cell, size_step, CellTopology, LayoutError, ResolvedConfig, STEP_GAP};

/// Cells a placed step occupies, measured from its origin. Mirrors the
/// deleted `rows::StepExtent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepExtent {
    pub width: u32,
    pub height: u32,
}

/// Sizes and places one whole step: `size_step` fixes how many machines a
/// cell holds and how many cells the step needs, then this splits that
/// machine count across cells and columns and tiles the cells from `origin`,
/// wrapping into a new band below whenever the next cell would push the
/// current one past `topo.target_width`.
///
/// Returns the binding stream(s) alongside the extent — `size_step` is
/// called exactly once here, so this is the single place that knows the
/// answer; a second caller re-deriving it (e.g. for display) could disagree
/// with what was actually placed.
pub fn place_step(
    grid: &mut Grid,
    step: &ProductionStep,
    cfg: &ResolvedConfig,
    topo: &CellTopology,
    origin: GridPos,
) -> Result<(StepExtent, String), LayoutError> {
    let sized = size_step(step, cfg.belt, topo)?;
    let machines = step.machines_needed.max(1);

    let mut x = origin.x;
    let mut y = origin.y;
    let mut edge_left = true;
    let mut band_width: u32 = 0;
    let mut band_height: u32 = 0;
    let mut total_width: u32 = 0;
    let mut total_height: u32 = 0;

    for i in 0..sized.cells {
        // Wrap before placing, never after — so a cell wider than
        // `target_width` still gets one band to itself instead of looping
        // forever trying to fit it into a band that can never hold it.
        if let Some(w) = topo.target_width {
            let predicted = cell_width(step.machine, topo, edge_left);
            if band_width > 0 && band_width + predicted > w {
                total_width = total_width.max(band_width);
                total_height += band_height + STEP_GAP as u32;
                x = origin.x;
                y += band_height as i32 + STEP_GAP;
                edge_left = true;
                band_width = 0;
                band_height = 0;
            }
        }

        let cell_machines = sized.machines_per_cell.min(machines - i * sized.machines_per_cell);
        let mut cell_plan = sized.clone();
        cell_plan.columns = (cell_machines.div_ceil(2), cell_machines / 2);

        let extent = place_cell(grid, step, &cell_plan, cfg, topo, GridPos { x, y }, edge_left)?;
        x += extent.width as i32;
        band_width += extent.width;
        band_height = band_height.max(extent.height);
        edge_left = false;
    }

    total_width = total_width.max(band_width);
    total_height += band_height;
    Ok((StepExtent { width: total_width, height: total_height }, sized.bound_by))
}
