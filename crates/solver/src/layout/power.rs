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
mod tests {
    use super::*;
    use crate::layout::LayoutConfig;
    use crate::testsupport::default_cfg;

    /// A realistic machine band: input belt/inserter, 3 rows of side-by-side
    /// assembling-machine-2 (3 wide each), output inserter/belt — the shape
    /// `layout::rows::place_step` will produce; built directly since that
    /// module doesn't exist in this worktree yet.
    fn build_band(n_machines: i32, belt: &str, inserter: &str) -> Grid {
        let mut grid = Grid::new();
        let width = n_machines * 3;
        let belt_row = |grid: &mut Grid, y: f64| {
            for x in 0..width {
                grid.place(belt, &Position { x: x as f64 + 0.5, y }, Direction::East, None, None).unwrap();
            }
        };
        let inserter_row = |grid: &mut Grid, y: f64, dir: Direction| {
            for m in 0..n_machines {
                let cx = (m * 3 + 1) as f64 + 0.5;
                grid.place(inserter, &Position { x: cx, y }, dir, None, None).unwrap();
            }
        };
        belt_row(&mut grid, 0.5);
        inserter_row(&mut grid, 1.5, Direction::South);
        for m in 0..n_machines {
            let left = (m * 3) as f64;
            let center = Position { x: left + 1.5, y: 3.5 };
            let recipe = Some(format!("recipe-{m}"));
            grid.place("assembling-machine-2", &center, Direction::North, recipe, None).unwrap();
        }
        inserter_row(&mut grid, 5.5, Direction::North);
        belt_row(&mut grid, 6.5);
        grid
    }

    /// `default_cfg()` gives an unresolved `LayoutConfig`; tests need the
    /// `&'static EntityPrototype`s a `ResolvedConfig` carries.
    fn resolved_default() -> ResolvedConfig {
        default_cfg().resolve().unwrap()
    }

    #[test]
    fn coverage_gaps_is_nonempty_before_placing_poles() {
        let grid = build_band(4, "express-transport-belt", "fast-inserter");
        assert!(!coverage_gaps(&grid).is_empty(), "unpowered band must report gaps");
    }

    #[test]
    fn every_machine_is_powered() {
        let mut grid = build_band(4, "express-transport-belt", "fast-inserter");
        let cfg = resolved_default();
        place_poles(&mut grid, &cfg).unwrap();
        assert!(coverage_gaps(&grid).is_empty());
    }

    #[test]
    fn belts_and_pipes_do_not_need_power_but_inserters_and_machines_do() {
        assert!(!needs_power("express-transport-belt"));
        assert!(!needs_power("pipe"));
        assert!(!needs_power("pipe-to-ground"));
        assert!(!needs_power("splitter"));
        assert!(!needs_power("medium-electric-pole"));
        assert!(needs_power("fast-inserter"));
        assert!(needs_power("assembling-machine-2"));
    }

    #[test]
    fn pole_reach_comes_from_prototype_data() {
        // Same band, two pole tiers: smaller reach must never need fewer
        // poles, and an 8-machine band is long enough to need strictly more
        // — proving the count tracks the registry value, not a constant.
        let cfg_small = LayoutConfig::new("express-transport-belt", "small-electric-pole", "fast-inserter")
            .resolve()
            .unwrap();
        let cfg_medium = resolved_default();
        assert_eq!(cfg_medium.pole.name, "medium-electric-pole");
        let mut small_grid = build_band(8, "express-transport-belt", "fast-inserter");
        place_poles(&mut small_grid, &cfg_small).unwrap();
        let small_poles = small_grid.entities().filter(|e| e.prototype_name == "small-electric-pole").count();
        let mut medium_grid = build_band(8, "express-transport-belt", "fast-inserter");
        place_poles(&mut medium_grid, &cfg_medium).unwrap();
        let medium_poles = medium_grid.entities().filter(|e| e.prototype_name == "medium-electric-pole").count();
        assert!(small_poles >= medium_poles, "small={small_poles} medium={medium_poles}");
        assert!(small_poles > medium_poles, "an 8-machine band must need more small poles: small={small_poles} medium={medium_poles}");
    }

    #[test]
    fn poles_displace_nothing() {
        let mut grid = build_band(4, "express-transport-belt", "fast-inserter");
        let before: Vec<(EntityId, i32, i32)> = grid.entities().map(|e| (e.id, e.top_left.x, e.top_left.y)).collect();
        let occupied_before: HashSet<(i32, i32)> = grid.entities().flat_map(|e| e.cells()).collect();
        place_poles(&mut grid, &resolved_default()).unwrap();
        for (id, x, y) in &before {
            let e = grid.get_entity(*id).expect("original entity must still be present");
            assert_eq!((e.top_left.x, e.top_left.y), (*x, *y), "original entity moved");
        }
        let is_pole = |e: &&PlacedEntity| EntityCategory::from_prototype_name(e.prototype_name) == EntityCategory::ElectricPole;
        for pole in grid.entities().filter(is_pole) {
            for cell in pole.cells() {
                assert!(!occupied_before.contains(&cell), "pole placed on a previously-occupied cell {cell:?}");
            }
        }
    }

    #[test]
    fn place_poles_on_a_fully_covered_grid_is_a_no_op() {
        let mut grid = build_band(3, "express-transport-belt", "fast-inserter");
        let cfg = resolved_default();
        place_poles(&mut grid, &cfg).unwrap();
        let count_after_first = grid.entity_count();
        place_poles(&mut grid, &cfg).unwrap();
        assert_eq!(grid.entity_count(), count_after_first, "fully covered grid gets no extra poles");
    }

    #[test]
    fn pole_placement_is_deterministic() {
        let cfg = resolved_default();
        let mut a = build_band(5, "express-transport-belt", "fast-inserter");
        let mut b = build_band(5, "express-transport-belt", "fast-inserter");
        place_poles(&mut a, &cfg).unwrap();
        place_poles(&mut b, &cfg).unwrap();
        let positions = |g: &Grid| -> Vec<(i32, i32)> {
            g.entities().filter(|e| e.prototype_name == cfg.pole.name).map(|e| (e.top_left.x, e.top_left.y)).collect()
        };
        assert_eq!(positions(&a), positions(&b), "same grid must yield the same poles every run");
    }

    #[test]
    fn no_room_for_pole_names_the_walled_in_recipe() {
        let cfg = LayoutConfig::new("express-transport-belt", "small-electric-pole", "fast-inserter")
            .resolve()
            .unwrap();
        let mut grid = Grid::new();
        // stone-furnace: 2x2 at top-left (0,0); the documented burner gap
        // counts it as needing power.
        grid.place("stone-furnace", &Position { x: 1.0, y: 1.0 }, Direction::North, Some("iron-plate".to_string()), None)
            .unwrap();
        // Wall off every other cell in a radius well beyond any reach + pole
        // + target size window, so no legal pole position remains.
        let radius = 10;
        for y in -radius..=radius {
            for x in -radius..=radius {
                if (0..2).contains(&x) && (0..2).contains(&y) {
                    continue; // the furnace's own footprint
                }
                grid.place("transport-belt", &Position { x: x as f64 + 0.5, y: y as f64 + 0.5 }, Direction::North, None, None)
                    .unwrap();
            }
        }
        match place_poles(&mut grid, &cfg) {
            Err(LayoutError::NoRoomForPole { step, pole }) => {
                assert_eq!(step, "iron-plate");
                assert_eq!(pole, "small-electric-pole");
            }
            other => panic!("expected NoRoomForPole, got {other:?}"),
        }
    }
}
