//! Integration tests: importing a 1.x blueprint rewrites its 8-way directions to the
//! 2.0 16-way scheme, and a 2.0 blueprint is left alone.

use factorio_blueprint::{Direction, Position};
use factorio_grid::import::from_blueprint;

/// Factorio 1.1.x packed version (major 1) — 1.x cardinal encoding.
const VERSION_1_1: u64 = 281479275675648;
/// Factorio 2.0 packed version (major 2) — 16-direction scheme.
const VERSION_2_0: u64 = 2u64 << 48;

fn entity(
    entity_number: u32,
    name: &str,
    x: f64,
    y: f64,
    direction: Direction,
) -> factorio_blueprint::Entity {
    factorio_blueprint::Entity {
        entity_number,
        name: name.to_string(),
        position: Position { x, y },
        direction,
        ..Default::default()
    }
}

fn bp_with(entities: Vec<factorio_blueprint::Entity>, version: u64) -> factorio_blueprint::Blueprint {
    factorio_blueprint::Blueprint {
        item: "blueprint".to_string(),
        entities,
        version,
        ..Default::default()
    }
}

/// 1.x pure-South belts encode as raw 4. After naive 2.0 decode that is East;
/// version major < 2 must upgrade them to Direction::South (finding #6177/#6179).
#[test]
fn test_import_legacy_pure_south_becomes_south() {
    let bp = bp_with(
        vec![
            entity(1, "transport-belt", 0.5, 0.5, Direction::East), // raw 4
            entity(2, "transport-belt", 0.5, 1.5, Direction::East),
        ],
        VERSION_1_1,
    );
    let result = from_blueprint(&bp);
    assert!(result.skipped.is_empty(), "skipped: {:?}", result.skipped);
    assert_eq!(result.grid.entity_count(), 2);
    for e in result.grid.entities() {
        assert_eq!(
            e.direction,
            Direction::South,
            "1.x pure-South belt must upgrade raw 4 → South"
        );
    }
}

/// 1.x North+South-only sets are {0,4} after decode — no East/West marker.
/// Version major < 2 must still upgrade 4 → South (finding #6177/#6179).
#[test]
fn test_import_legacy_north_south_only_upgrades_south() {
    let bp = bp_with(
        vec![
            entity(1, "transport-belt", 0.5, 0.5, Direction::North), // raw 0
            entity(2, "transport-belt", 0.5, 1.5, Direction::East),  // raw 4 → South
            entity(3, "inserter", 1.5, 0.5, Direction::East),        // raw 4 → South
        ],
        VERSION_1_1,
    );
    let result = from_blueprint(&bp);
    assert!(result.skipped.is_empty(), "skipped: {:?}", result.skipped);

    let mut north_belts = 0;
    let mut south_belts = 0;
    let mut south_inserters = 0;
    for e in result.grid.entities() {
        match (e.prototype_name, e.direction) {
            ("transport-belt", Direction::North) => north_belts += 1,
            ("transport-belt", Direction::South) => south_belts += 1,
            ("inserter", Direction::South) => south_inserters += 1,
            other => panic!("unexpected entity after 1.x N+S upgrade: {other:?}"),
        }
    }
    assert_eq!(north_belts, 1);
    assert_eq!(south_belts, 1);
    assert_eq!(south_inserters, 1);
}

/// Parallel 2.0 North+East fixture: same raw {0,4} set must NOT rewrite.
#[test]
fn test_import_modern_north_east_not_rewritten() {
    let bp = bp_with(
        vec![
            entity(1, "transport-belt", 0.5, 0.5, Direction::North),
            entity(2, "transport-belt", 1.5, 0.5, Direction::East),
        ],
        VERSION_2_0,
    );
    let result = from_blueprint(&bp);
    let dirs: Vec<_> = result.grid.entities().map(|e| e.direction).collect();
    assert!(dirs.contains(&Direction::North), "{dirs:?}");
    assert!(dirs.contains(&Direction::East), "{dirs:?}");
    assert!(
        !dirs.contains(&Direction::South),
        "must not upgrade 4→South on 2.0: {dirs:?}"
    );
}
