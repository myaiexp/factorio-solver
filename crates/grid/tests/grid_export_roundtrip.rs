//! Integration test: a Grid exported to a blueprint, encoded, decoded and re-imported
//! keeps its entities, positions and directions.

use factorio_blueprint::{decode, encode, Direction, Position};
use factorio_grid::{to_blueprint, Grid};

// -- Round-trip tests (Grid → Blueprint → encode → decode) --------------------

/// Place several entities in a Grid, export to a Blueprint, encode to a
/// blueprint string, decode it again, and verify that entity count,
/// names, positions, and directions are all preserved.
#[test]
fn test_grid_to_blueprint_round_trip() {
    let mut grid = Grid::new();

    let pos = |x: f64, y: f64| Position { x, y };

    // Deliberately varied: names, positions, directions, recipe, entity_type.
    grid.place("transport-belt", &pos(0.5, 0.5), Direction::East, None, None)
        .unwrap();
    grid.place("inserter", &pos(1.5, 0.5), Direction::South, None, None)
        .unwrap();
    grid.place(
        "assembling-machine-2",
        &pos(3.5, 1.5),
        Direction::North,
        Some("iron-gear-wheel".to_string()),
        None,
    )
    .unwrap();
    grid.place(
        "underground-belt",
        &pos(5.5, 0.5),
        Direction::West,
        None,
        Some("input".to_string()),
    )
    .unwrap();
    grid.place("small-electric-pole", &pos(6.5, 0.5), Direction::North, None, None)
        .unwrap();

    let version = 281479275675648_u64;
    let bp = to_blueprint(&grid, Some("Test Round-Trip".to_string()), version);

    // Encode to a blueprint string.
    let bp_data = factorio_blueprint::BlueprintData {
        blueprint: Some(bp),
        blueprint_book: None,
    };
    let encoded = encode(&bp_data).expect("encode should succeed");
    assert!(encoded.starts_with('0'), "Factorio blueprint strings start with '0'");

    // Decode back.
    let decoded_data = decode(&encoded).expect("decode should succeed");
    let decoded_bp = decoded_data.blueprint.expect("should decode to a single blueprint");

    // Entity count must match.
    assert_eq!(decoded_bp.entities.len(), grid.entity_count());

    // Sort decoded entities by entity_number for deterministic comparison.
    let mut decoded_entities = decoded_bp.entities.clone();
    decoded_entities.sort_by_key(|e| e.entity_number);

    assert_eq!(decoded_entities[0].name, "transport-belt");
    assert_eq!(decoded_entities[0].position.x, 0.5);
    assert_eq!(decoded_entities[0].position.y, 0.5);
    assert_eq!(decoded_entities[0].direction, Direction::East);

    assert_eq!(decoded_entities[1].name, "inserter");
    assert_eq!(decoded_entities[1].direction, Direction::South);

    assert_eq!(decoded_entities[2].name, "assembling-machine-2");
    assert_eq!(decoded_entities[2].recipe.as_deref(), Some("iron-gear-wheel"));
    assert_eq!(decoded_entities[2].direction, Direction::North);

    assert_eq!(decoded_entities[3].name, "underground-belt");
    assert_eq!(decoded_entities[3].entity_type.as_deref(), Some("input"));
    assert_eq!(decoded_entities[3].direction, Direction::West);

    assert_eq!(decoded_entities[4].name, "small-electric-pole");

    // Label and version survive the round-trip.
    assert_eq!(decoded_bp.label.as_deref(), Some("Test Round-Trip"));
    assert_eq!(decoded_bp.version, version);
}
