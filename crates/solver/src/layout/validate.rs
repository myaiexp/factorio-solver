// Pre-emit checks on a generated block.
//
// The cell and pole passes are *supposed* to make every hard error below
// impossible. Running the same guarantee again here, independently, is what
// actually proves that rather than trusting it — in particular the
// connectivity check re-derives each inserter's reach from its own
// prototype data and placed direction, rather than asking `place_cell` what
// it intended to place.
//
// There is no belt-capacity warning here: sizing a cell from the belt's own
// throughput (`cell::size_step`) makes an over-rating segment structurally
// impossible, unlike the old row topology's fixed-lane-count belts. A
// delivered-rate check measured from the placed grid is a later task.
use std::collections::HashSet;

use factorio_grid::prototype;
use factorio_grid::{EntityCategory, Grid, GridPos, PlacedEntity};

use crate::chain::ProductionPlan;
use crate::layout::{LayoutConfig, LayoutError};

/// Soft findings. Warnings never block emission; errors always do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Validation {
    pub warnings: Vec<String>,
}

pub fn validate(
    grid: &Grid,
    plan: &ProductionPlan,
    cfg: &LayoutConfig,
) -> Result<Validation, LayoutError> {
    cfg.resolve()?;

    check_machine_connectivity(grid)?;
    check_no_overlaps(grid)?;
    check_pole_coverage(grid)?;

    Ok(Validation { warnings: plan.warnings.iter().cloned().collect() })
}

// ── Hard errors ─────────────────────────────────────────────────────

fn is_machine(category: EntityCategory) -> bool {
    matches!(
        category,
        EntityCategory::Assembler
            | EntityCategory::Furnace
            | EntityCategory::ChemicalPlant
            | EntityCategory::Refinery
    )
}

/// Whether a machine's own recipe declares ingredients/results, so an
/// input/output inserter is actually required. A machine with no resolvable
/// recipe (name absent, or not in the registry) cannot prove it needs
/// nothing, so it is treated as needing both — the conservative default a
/// real `generate()` grid never actually exercises, since every machine it
/// places carries the step's own (real) recipe name.
fn recipe_needs(machine: &PlacedEntity) -> (bool, bool) {
    match machine.recipe.as_deref().and_then(crate::recipe::get) {
        Some(r) => (!r.ingredients.is_empty(), !r.results.is_empty()),
        None => (true, true),
    }
}

/// Rotate a centre-relative offset clockwise by `turns` quarter turns.
/// Reimplemented rather than imported from `place::helpers`: sharing the
/// helper would mean a bug in it passes both the placement and the check
/// that is supposed to catch placement bugs.
fn rotate(offset: (f64, f64), turns: u8) -> (f64, f64) {
    (0..turns).fold(offset, |(x, y), _| (-y, x))
}

fn to_delta(offset: (f64, f64)) -> (i32, i32) {
    (offset.0.round() as i32, offset.1.round() as i32)
}

/// `(pickup_cell, insert_cell)` for a placed inserter, derived from its own
/// prototype's North-orientation offsets rotated by its placed direction.
/// `None` when the prototype is unknown or lacks pickup/insert data — never
/// true for an inserter `generate` itself placed, since `LayoutConfig::resolve`
/// checks both fields before anything is placed.
fn inserter_cells(inserter: &PlacedEntity) -> Option<((i32, i32), (i32, i32))> {
    let proto = prototype::lookup(inserter.prototype_name)?;
    let pickup = proto.pickup_position?;
    let insert = proto.insert_position?;
    let turns = inserter.direction.as_u8() / 4;
    let (x, y) = (inserter.top_left.x, inserter.top_left.y);
    let pickup_delta = to_delta(rotate(pickup, turns));
    let insert_delta = to_delta(rotate(insert, turns));
    Some((
        (x + pickup_delta.0, y + pickup_delta.1),
        (x + insert_delta.0, y + insert_delta.1),
    ))
}

/// Every machine with ingredients has an inserter inserting into one of its
/// cells; every machine with results has one picking up from one of its
/// cells. Built from a single pass over every inserter's derived reach
/// rather than a per-machine scan, so the check stays cheap at block scale.
fn check_machine_connectivity(grid: &Grid) -> Result<(), LayoutError> {
    let mut pickup_sources: HashSet<(i32, i32)> = HashSet::new();
    let mut insert_targets: HashSet<(i32, i32)> = HashSet::new();
    for e in grid.entities() {
        if EntityCategory::from_prototype_name(e.prototype_name) != EntityCategory::Inserter {
            continue;
        }
        if let Some((pickup_cell, insert_cell)) = inserter_cells(e) {
            pickup_sources.insert(pickup_cell);
            insert_targets.insert(insert_cell);
        }
    }

    for machine in grid
        .entities()
        .filter(|e| is_machine(EntityCategory::from_prototype_name(e.prototype_name)))
    {
        let (needs_input, needs_output) = recipe_needs(machine);
        let cells: Vec<(i32, i32)> = machine.cells().collect();
        let not_connected = |missing: &str| LayoutError::MachineNotConnected {
            recipe: machine.recipe.clone().unwrap_or_else(|| machine.prototype_name.to_string()),
            x: machine.top_left.x,
            y: machine.top_left.y,
            missing: missing.to_string(),
        };
        if needs_input && !cells.iter().any(|c| insert_targets.contains(c)) {
            return Err(not_connected("input"));
        }
        if needs_output && !cells.iter().any(|c| pickup_sources.contains(c)) {
            return Err(not_connected("output"));
        }
    }
    Ok(())
}

/// `Grid::place` already makes overlapping footprints impossible, so this is
/// a cheap invariant re-check rather than a control-flow path any current
/// generator bug can reach — insurance against a *future* one.
fn check_no_overlaps(grid: &Grid) -> Result<(), LayoutError> {
    for e in grid.entities() {
        for (x, y) in e.cells() {
            match grid.get_at(x, y) {
                Some(found) if found.id == e.id => {}
                _ => return Err(LayoutError::Overlap { x, y }),
            }
        }
    }
    Ok(())
}

fn check_pole_coverage(grid: &Grid) -> Result<(), LayoutError> {
    let Some(GridPos { x, y }) = crate::layout::coverage_gaps(grid).into_iter().next() else {
        return Ok(());
    };
    let recipe = grid
        .get_at(x, y)
        .map(|e| e.recipe.clone().unwrap_or_else(|| e.prototype_name.to_string()))
        .unwrap_or_else(|| "?".to_string());
    Err(LayoutError::Unpowered { recipe, x, y })
}

#[cfg(test)]
mod tests;
