// Power pole placement: enough poles, placed only into free cells, that
// every powered entity's footprint overlaps some pole's supply area. Greedy
// set cover from the first (topmost-leftmost) uncovered entity, so every
// pass makes progress and the same grid always yields the same poles.
use std::collections::HashSet;

use factorio_blueprint::{Direction, Position};
use factorio_grid::prototype::{self, EntityPrototype};
use factorio_grid::{EntityCategory, EntityId, Grid, GridPos, PlacedEntity};
use crate::layout::{LayoutError, ResolvedConfig};

/// Entities Factorio requires an electrical connection for (belts, pipes,
/// splitters and poles draw no power). Known gap: a burner machine (e.g.
/// `stone-furnace`) is classified `Furnace` just like an electric one — the
/// registry does not yet distinguish the energy source — so it is
/// (harmlessly) treated as needing power too.
fn needs_power(prototype_name: &str) -> bool {
    use EntityCategory::*;
    matches!(
        EntityCategory::from_prototype_name(prototype_name),
        Inserter | Assembler | Furnace | ChemicalPlant | Refinery | Beacon | Combinator | Lamp
    )
}

/// True iff a footprint at `top_left`/`size` overlaps supply rectangle
/// `area`. Strict: Factorio powers on overlap, not containment, and a
/// boundary that only grazes must not count.
fn overlaps_supply(top_left: (i32, i32), size: (u32, u32), area: (f64, f64, f64, f64)) -> bool {
    let (x, y) = (top_left.0 as f64, top_left.1 as f64);
    let (w, h) = (size.0 as f64, size.1 as f64);
    let (min_x, min_y, max_x, max_y) = area;
    x < max_x && x + w > min_x && y < max_y && y + h > min_y
}

/// The supply rectangle of every pole already on the grid.
///
/// Each pole's reach is read from its *own* prototype rather than from the
/// config's: a grid can arrive here with poles this module did not place —
/// an imported blueprint, or a second `place_poles` run under a different
/// config — and scoring those with the configured pole's reach would report
/// coverage the game will not give.
fn supply_areas(grid: &Grid) -> Vec<(f64, f64, f64, f64)> {
    grid.entities()
        .filter(|e| {
            EntityCategory::from_prototype_name(e.prototype_name) == EntityCategory::ElectricPole
        })
        .filter_map(|e| {
            prototype::lookup(e.prototype_name)?.supply_area((e.top_left.x, e.top_left.y))
        })
        .collect()
}

/// Whether `entity` sits in any of those supply areas.
fn is_covered(entity: &PlacedEntity, areas: &[(f64, f64, f64, f64)]) -> bool {
    areas
        .iter()
        .any(|&a| overlaps_supply((entity.top_left.x, entity.top_left.y), entity.size, a))
}

/// Powered entities not covered by any pole's supply area, by top-left cell.
///
/// Takes no config: reach is read from each pole already on the grid, so the
/// answer is about the grid as it stands rather than about what a subsequent
/// `place_poles` would use.
pub fn coverage_gaps(grid: &Grid) -> Vec<GridPos> {
    let areas = supply_areas(grid);
    grid.entities()
        .filter(|e| needs_power(e.prototype_name))
        .filter(|e| !is_covered(e, &areas))
        .map(|e| e.top_left)
        .collect()
}

/// Whether the pole's whole footprint is free at `top_left` — checked before
/// placing so a failed candidate is never mistaken for a real failure.
fn pole_fits(grid: &Grid, pole: &EntityPrototype, top_left: (i32, i32), size: (u32, u32)) -> bool {
    let center = Position { x: top_left.0 as f64 + size.0 as f64 / 2.0, y: top_left.1 as f64 + size.1 as f64 / 2.0 };
    matches!(grid.can_place(&pole.name, &center, Direction::North), Ok(true))
}

/// Places poles so every powered entity is within a pole's supply area.
/// Repeatedly takes the topmost-leftmost uncovered entity, searches a bounded
/// window around it for the free position covering the most currently-
/// uncovered entities, and places it. Ties break on `(y, x)` for determinism.
pub fn place_poles(grid: &mut Grid, cfg: &ResolvedConfig) -> Result<(), LayoutError> {
    let pole_size = prototype::effective_size(cfg.pole, Direction::North);
    let reach = cfg.pole.supply_area_distance.expect("ResolvedConfig guarantees a supply area");

    loop {
        let areas = supply_areas(grid);
        let mut uncovered: Vec<&PlacedEntity> = grid
            .entities()
            .filter(|e| needs_power(e.prototype_name))
            .filter(|e| !is_covered(e, &areas))
            .collect();
        if uncovered.is_empty() {
            return Ok(());
        }
        uncovered.sort_by_key(|e| (e.top_left.y, e.top_left.x));
        let target = uncovered[0];
        let target_tl = (target.top_left.x, target.top_left.y);
        let uncovered_ids: HashSet<EntityId> = uncovered.iter().map(|e| e.id).collect();

        // Any covering pole position lies within reach + pole size + target
        // size of the target on each axis, so this window stays bounded.
        let span = reach.ceil() as i32
            + pole_size.0.max(pole_size.1) as i32
            + target.size.0.max(target.size.1) as i32;

        let mut best: Option<((i32, i32), usize)> = None;
        for dy in -span..=span {
            for dx in -span..=span {
                let pos = (target_tl.0 + dx, target_tl.1 + dy);
                if !pole_fits(grid, cfg.pole, pos, pole_size) {
                    continue;
                }
                let Some(area) = cfg.pole.supply_area(pos) else { continue };
                if !overlaps_supply(target_tl, target.size, area) {
                    continue;
                }
                let (min_x, min_y, max_x, max_y) = area;
                let score = grid
                    .query_rect(min_x.floor() as i32, min_y.floor() as i32, max_x.ceil() as i32, max_y.ceil() as i32)
                    .iter()
                    .filter(|e| {
                        uncovered_ids.contains(&e.id)
                            && overlaps_supply((e.top_left.x, e.top_left.y), e.size, area)
                    })
                    .count();
                let better = best
                    .is_none_or(|(bp, bs)| score > bs || (score == bs && (pos.1, pos.0) < (bp.1, bp.0)));
                if better {
                    best = Some((pos, score));
                }
            }
        }

        let Some((pos, _)) = best else {
            return Err(LayoutError::NoRoomForPole {
                step: target.recipe.clone().unwrap_or_else(|| target.prototype_name.to_string()),
                pole: cfg.pole.name.clone(),
            });
        };
        let center = Position {
            x: pos.0 as f64 + pole_size.0 as f64 / 2.0,
            y: pos.1 as f64 + pole_size.1 as f64 / 2.0,
        };
        grid.place(&cfg.pole.name, &center, Direction::North, None, None)?;
    }
}

#[cfg(test)]
mod tests;
