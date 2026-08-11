// Pre-emit checks on a generated block.
//
// The row and pole passes are *supposed* to make every hard error below
// impossible. Running the same guarantee again here, independently, is what
// actually proves that rather than trusting it — in particular the
// connectivity check re-derives each inserter's reach from its own
// prototype data and placed direction, rather than asking `rows` what it
// intended to place.
use std::collections::HashSet;

use factorio_grid::prototype::{self, EntityPrototype};
use factorio_grid::{EntityCategory, Grid, GridPos, PlacedEntity};

use crate::chain::{ItemRate, ProductionPlan};
use crate::layout::{lane_throughput, pack_lanes, LayoutConfig, LayoutError};

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
    let resolved = cfg.resolve()?;

    check_machine_connectivity(grid)?;
    check_no_overlaps(grid)?;
    check_pole_coverage(grid)?;

    let mut warnings = belt_capacity_warnings(plan, resolved.belt);
    warnings.extend(plan.warnings.iter().cloned());
    Ok(Validation { warnings })
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
/// Reimplemented rather than imported from `rows`: sharing the helper would
/// mean a bug in it passes both the placement and the check that is
/// supposed to catch placement bugs.
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

// ── Warnings ────────────────────────────────────────────────────────

/// How many of a belt's two lanes `item` gets under `pack_lanes`'s own rule:
/// alone in its pair it takes both, sharing a pair it takes one. Mirrors
/// that rule directly rather than building `BeltAssignment`s, since only the
/// lane *count* is needed here.
fn lanes_for_item(rates: &[ItemRate], item: &str) -> u32 {
    rates
        .chunks(2)
        .find(|chunk| chunk.iter().any(|r| r.item == item))
        .map_or(0, |chunk| if chunk.len() == 1 { 2 } else { 1 })
}

/// The slowest belt in the registry whose throughput at `lanes` lanes covers
/// `rate`, or `None` if nothing registered is fast enough.
///
/// Restricted to `EntityCategory::Belt`: loaders, splitters and underground
/// belts also carry a `belt_throughput`, but every mainline speed has one of
/// each sharing its exact number, which would make "the slowest sufficient
/// one" an arbitrary pick among ties — and none of those is a tier a player
/// actually swaps a straight belt run into.
fn fixing_tier(rate: f64, lanes: u32) -> Option<String> {
    let mut belts: Vec<&'static EntityPrototype> = prototype::all_names()
        .into_iter()
        .filter_map(prototype::lookup)
        .filter(|p| EntityCategory::from_prototype_name(&p.name) == EntityCategory::Belt)
        .filter(|p| lane_throughput(p) * f64::from(lanes) >= rate)
        .collect();
    belts.sort_by(|a, b| a.belt_throughput.partial_cmp(&b.belt_throughput).unwrap());
    belts.first().map(|p| p.name.clone())
}

fn over_capacity_warning(
    recipe: &str,
    item: &str,
    actual: f64,
    available: f64,
    belt: &str,
    lanes: u32,
) -> String {
    match fixing_tier(actual, lanes) {
        Some(tier) => format!(
            "recipe `{recipe}`: `{item}` needs {actual:.2}/s but one `{belt}` segment here \
             carries only {available:.2}/s — `{tier}` would be enough"
        ),
        None => format!(
            "recipe `{recipe}`: `{item}` needs {actual:.2}/s but one `{belt}` segment here \
             carries only {available:.2}/s — no belt tier in the registry is fast enough"
        ),
    }
}

/// One warning per ingredient/product rate whose share of a step's belts
/// exceeds what those belts carry.
///
/// The sub-row count a step will actually place is clamped to
/// `machines_needed` (`rows::place_step`), which is exactly why this is
/// reachable rather than theoretical: a step needing many belts but few
/// machines cannot spread its load across as many belts as `pack_lanes`
/// alone would suggest.
fn belt_capacity_warnings(plan: &ProductionPlan, belt: &EntityPrototype) -> Vec<String> {
    let mut warnings = Vec::new();
    for step in &plan.steps {
        let ins: Vec<ItemRate> = step.inputs.iter().filter(|r| r.per_sec > 0.0).cloned().collect();
        let outs: Vec<ItemRate> = step.outputs.iter().filter(|r| r.per_sec > 0.0).cloned().collect();

        let sub_rows = pack_lanes(&ins, belt)
            .len()
            .max(pack_lanes(&outs, belt).len())
            .max(1)
            .min(step.machines_needed as usize);
        if sub_rows == 0 {
            continue; // degenerate step (no machines): nothing is placed to warn about
        }

        for rates in [&ins, &outs] {
            for r in rates {
                let lanes = lanes_for_item(rates, &r.item);
                let available = lane_throughput(belt) * f64::from(lanes);
                let actual = r.per_sec / sub_rows as f64;
                if actual > available {
                    warnings.push(over_capacity_warning(
                        &step.recipe.name,
                        &r.item,
                        actual,
                        available,
                        &belt.name,
                        lanes,
                    ));
                }
            }
        }
    }
    warnings
}

#[cfg(test)]
mod tests;
