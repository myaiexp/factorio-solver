//! Integration tests for blueprint import.
//!
//! Uses real Factorio blueprint strings (same as in `crates/blueprint/tests/roundtrip.rs`)
//! decoded via `factorio_blueprint::decode()` and imported into the grid engine.

use factorio_blueprint::{decode, encode, Direction, Position};
use factorio_grid::import::from_blueprint;
use factorio_grid::{to_blueprint, Grid};

// -- Test blueprint strings ---------------------------------------------------

/// Minimal blueprint: a single transport belt facing east.
const SINGLE_BELT: &str = concat!(
    "0eNptzcEKgzAMgOF3ybkbKrZiX2WMUTWMgKbSVplI332tveywU0nC9/eEYd5wdcQB",
    "9AkUcAH9sxOAHCgQetCPswzHi7dlQAe6FsBmwSSCM+xX68JtwDmz1frELOfqB3R1lw",
    "KO640CJnI4lmsTnwJotFw+8PRmM2f0P5ww8YSpWGe4o/NXRqqmb/tetlWnVNfE+AWu",
    "hUgS",
);

/// Assembler with inserters and belts, tests recipe & direction fields.
const ASSEMBLER_SETUP: &str = concat!(
    "0eNqNkcFqwzAMhl9l6GxD6yYpyW3P0OMow0lFKrCVYDvrSsi7T1kgDJptvRgs8X2/",
    "kEao3YB9IE5QjUAJPVQ/agqcrdFJ7TVG9LXD8HLCNPTSQU6UCCNUb+Pyub/z4GsMUO",
    "0VsPUonF044lZ721yJURuB+y4K3PGc+gnVTsFd3klBwIb6GaTQsW7RBn27ooyg4ELSX",
    "CAzqYdMs2YSRwxJag852qxB/9gOT9g2ZcWGLFtlKViOfReSlrWmjQEPW85sw5k/7fx",
    "FeVZATcfL/SK1bN0M/HU3URBfUJz7Gf/AEL9leWHKrCzzbHcsiqOZpi/Cqcff",
);

/// Underground belt pair with "type": "input"/"output".
const UNDERGROUND_BELTS: &str = concat!(
    "0eNqVkVGKwjAQQO8y31G021aaq4gsbR1koJ2EZCKW0rubWBBBw65fQxLeeySZoRsCWk",
    "csoGcgwRH0y54CZCEh9KCP87qYfjmMHTrQewXcjhiJwGd0F2fi3HQ4JNAaH0HDyXsD",
    "vdtWCqbHXBScyWG/nhYKZLJJQmyDwKLeOsUXnfLvjgmSCf08Q+Ja9tY4yWQ2+ft88J",
    "b/9lZ57UkB9YbXn/B04XZIUO5lIk5xLzr3Cb2i8w9RVRdN2TRVuTvU9aFYljv3gKqg",
);

/// Complex blueprint with circuit connections, combinators, modules, and control_behavior.
const COMPLEX_CIRCUIT: &str = concat!(
    "0eNqtVNuK2zAQ/ZWi19oldpws8UOh9BP6uAQj25NkQDdkKW0I/veOpF0nWyfLpq0NQr",
    "c5OnN0NGfWCg/GonKsPjN0IFl9NZcxwVsQNPcdbefRffoBzhuaB+XQIQysfj6nwalR",
    "XrZgWV1kTHEJFMUtuoMEh13eadmi4k5bijZ6oGitwqG/WL3I2InaL6sxYz1a6NJamb",
    "FOK2e1aFo48CNSLAVcQBta7iPQEBZ2aAfXDLhXXISxO5lA4ojWeZqZWKUd+TdGxw0Q",
    "MALQ4HhQgchrA5YnCuwzhWnvjH8MmL5spks5beyhwx7sB0QpPyjKC+J/UORKisUiDK",
    "XhNlKs2de/UyOgmBNx88o1O6tlg4owWO2sh5tSLSeUVzrvakUqPWShHQoHNpn30UT8",
    "i0tQ9UBHF4H9DCO+pAkArVa5EdzBFURVThjluL0lQnVhILkQueDS3M89OeVWtl16ux",
    "dv/Ls15sagZT8AnSF0EDbd7Dyn1aU2DAPIVqDa55J3B1SQL+fZ5ct0sQRPl4qRIwi6X",
    "pI0VJWUGsUFxaPnBwPQ51L3XkBekoq3aKwnGhJ69DJPmIRotIA5jeqNwUgHlRwWTyx",
    "CY6G/roXYR5O8Sh+H4zZjewug/ty4DPcfVn+Sc4Mrn4uspL/YZtSr6C+pF2aWsUdtt",
    "qYeRWAXSbzx8bvFd7zn3Pulabw2asaO9HKiMqt1uak2m1W1eFqvn8px/A0lpiJv",
);

// -- Helpers ------------------------------------------------------------------

/// Decode a blueprint string and extract the single Blueprint from it.
fn decode_blueprint(s: &str) -> factorio_blueprint::Blueprint {
    let data = decode(s).unwrap_or_else(|e| panic!("decode failed: {e}"));
    data.blueprint.expect("expected a single blueprint, not a book")
}

// -- Test cases ---------------------------------------------------------------

#[test]
fn test_import_single_belt() {
    let bp = decode_blueprint(SINGLE_BELT);
    let result = from_blueprint(&bp);

    // 1 entity placed, 0 skipped
    assert_eq!(result.grid.entity_count(), 1);
    assert!(result.skipped.is_empty());

    // 1x1 entity occupies exactly 1 cell
    assert_eq!(result.grid.cell_count(), 1);

    // Verify it's a transport belt
    let entity = result.grid.entities().next().unwrap();
    assert_eq!(entity.prototype_name, "transport-belt");
}

#[test]
fn test_import_assembler_setup() {
    let bp = decode_blueprint(ASSEMBLER_SETUP);
    let result = from_blueprint(&bp);

    // All entities in this blueprint should be known prototypes
    assert!(
        result.skipped.is_empty(),
        "expected no skipped entities, got: {:?}",
        result.skipped.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    // Verify total entity count matches blueprint
    assert_eq!(result.grid.entity_count(), bp.entities.len());

    // Verify the assembler is placed and has its recipe
    let assembler: Vec<_> = result
        .grid
        .entities()
        .filter(|e| e.prototype_name == "assembling-machine-2")
        .collect();
    assert_eq!(assembler.len(), 1);
    assert_eq!(assembler[0].recipe.as_deref(), Some("iron-gear-wheel"));

    // Verify inserters are placed
    let inserters: Vec<_> = result
        .grid
        .entities()
        .filter(|e| e.prototype_name == "inserter")
        .collect();
    assert_eq!(inserters.len(), 2);
}

#[test]
fn test_import_underground_belts() {
    let bp = decode_blueprint(UNDERGROUND_BELTS);
    let result = from_blueprint(&bp);

    // All entities should be known
    assert!(
        result.skipped.is_empty(),
        "expected no skipped entities, got: {:?}",
        result.skipped.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    // Verify underground belts have entity_type preserved
    let undergrounds: Vec<_> = result
        .grid
        .entities()
        .filter(|e| e.prototype_name == "underground-belt")
        .collect();
    assert_eq!(undergrounds.len(), 2);

    let types: Vec<&str> = undergrounds
        .iter()
        .map(|e| e.entity_type.as_deref().unwrap())
        .collect();
    assert!(types.contains(&"input"), "expected an 'input' underground belt");
    assert!(types.contains(&"output"), "expected an 'output' underground belt");
}

#[test]
fn test_import_complex_circuit() {
    let bp = decode_blueprint(COMPLEX_CIRCUIT);
    let result = from_blueprint(&bp);

    // Combinators should be placed
    let arithmetic: Vec<_> = result
        .grid
        .entities()
        .filter(|e| e.prototype_name == "arithmetic-combinator")
        .collect();
    assert!(!arithmetic.is_empty(), "expected at least one arithmetic combinator");

    let decider: Vec<_> = result
        .grid
        .entities()
        .filter(|e| e.prototype_name == "decider-combinator")
        .collect();
    assert!(!decider.is_empty(), "expected at least one decider combinator");

    let constant: Vec<_> = result
        .grid
        .entities()
        .filter(|e| e.prototype_name == "constant-combinator")
        .collect();
    assert!(!constant.is_empty(), "expected at least one constant combinator");

    // The total placed + skipped should equal the blueprint entity count
    let total = result.grid.entity_count() + result.skipped.len();
    assert_eq!(
        total,
        bp.entities.len(),
        "placed ({}) + skipped ({}) should equal blueprint entities ({})",
        result.grid.entity_count(),
        result.skipped.len(),
        bp.entities.len()
    );

    // If any entities were skipped, they should be unknown prototypes
    for skipped in &result.skipped {
        assert!(
            skipped.reason.contains("unknown prototype"),
            "skipped entity '{}' should be due to unknown prototype, but reason was: {}",
            skipped.name,
            skipped.reason
        );
    }
}

#[test]
fn test_import_all_unknown() {
    // Build a blueprint with only unknown entity names
    use std::collections::HashMap;
    let bp = factorio_blueprint::Blueprint {
        item: "blueprint".to_string(),
        label: None,
        label_color: None,
        description: None,
        icons: None,
        entities: vec![
            factorio_blueprint::Entity {
                entity_number: 1,
                name: "modded-laser-turret".to_string(),
                position: factorio_blueprint::Position { x: 0.5, y: 0.5 },
                direction: factorio_blueprint::Direction::North,
                entity_type: None,
                recipe: None,
                connections: None,
                control_behavior: None,
                items: None,
                wires: None,
                tags: None,
                extra: HashMap::new(),
            },
            factorio_blueprint::Entity {
                entity_number: 2,
                name: "alien-artifact-processor".to_string(),
                position: factorio_blueprint::Position { x: 1.5, y: 0.5 },
                direction: factorio_blueprint::Direction::North,
                entity_type: None,
                recipe: None,
                connections: None,
                control_behavior: None,
                items: None,
                wires: None,
                tags: None,
                extra: HashMap::new(),
            },
            factorio_blueprint::Entity {
                entity_number: 3,
                name: "space-science-lab".to_string(),
                position: factorio_blueprint::Position { x: 2.5, y: 0.5 },
                direction: factorio_blueprint::Direction::North,
                entity_type: None,
                recipe: None,
                connections: None,
                control_behavior: None,
                items: None,
                wires: None,
                tags: None,
                extra: HashMap::new(),
            },
        ],
        tiles: vec![],
        wires: None,
        schedules: None,
        snap_to_grid: None,
        absolute_snapping: None,
        position_relative_to_grid: None,
        version: 281479275675648,
        extra: HashMap::new(),
    };

    let result = from_blueprint(&bp);

    // Grid should be empty
    assert_eq!(result.grid.entity_count(), 0);
    assert_eq!(result.grid.cell_count(), 0);

    // All 3 entities should be skipped
    assert_eq!(result.skipped.len(), 3);

    // Verify each skipped entity has the right fields
    for skipped in &result.skipped {
        assert!(skipped.reason.contains("unknown prototype"));
    }
    assert_eq!(result.skipped[0].name, "modded-laser-turret");
    assert_eq!(result.skipped[1].name, "alien-artifact-processor");
    assert_eq!(result.skipped[2].name, "space-science-lab");
}

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

#[test]
fn test_no_collisions_in_real_blueprints() {
    // Real Factorio blueprints should never have overlapping entities.
    // Verify that all four real blueprint strings import with zero skipped
    // entities (no collisions, no unknown prototypes).
    let blueprints = [
        ("SINGLE_BELT", SINGLE_BELT),
        ("ASSEMBLER_SETUP", ASSEMBLER_SETUP),
        ("UNDERGROUND_BELTS", UNDERGROUND_BELTS),
        ("COMPLEX_CIRCUIT", COMPLEX_CIRCUIT),
    ];

    for (name, bp_string) in &blueprints {
        let bp = decode_blueprint(bp_string);
        let result = from_blueprint(&bp);

        assert!(
            result.skipped.is_empty(),
            "blueprint '{}' had {} skipped entities: {:?}",
            name,
            result.skipped.len(),
            result.skipped.iter().map(|s| format!("{} ({})", s.name, s.reason)).collect::<Vec<_>>()
        );

        assert_eq!(
            result.grid.entity_count(),
            bp.entities.len(),
            "blueprint '{}': placed {} entities but expected {}",
            name,
            result.grid.entity_count(),
            bp.entities.len()
        );
    }
}
