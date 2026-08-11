// Pole-placement tests. Split out of `power.rs` to keep both files readable;
// `super::*` still reaches the private predicates the coverage tests assert
// on directly.
use super::*;
use crate::layout::LayoutConfig;
use crate::testsupport::default_cfg;

/// A realistic machine band: input belt/inserter, 3 rows of side-by-side
/// assembling-machine-2 (3 wide each), output inserter/belt — hand-built
/// directly rather than generated, since these tests only care about pole
/// placement over a plausible machine footprint, not which layout pass
/// produced it.
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
