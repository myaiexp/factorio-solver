//! Integration tests with real Factorio blueprint strings.
//!
//! Validates that decode -> encode -> decode round-trips preserve all data,
//! using both struct equality and JSON value equality.

use factorio_blueprint::fixtures::{
    ASSEMBLER_SETUP, BLUEPRINT_BOOK, COMPLEX_CIRCUIT, SINGLE_BELT, TILES_BLUEPRINT,
    UNDERGROUND_BELTS,
};
use factorio_blueprint::decode;

mod common;

use common::{assert_json_equivalent, assert_roundtrip};

// -- Test cases ------------------------------------------------------------

/// Verifies that encode→decode is idempotent: decode(encode(decode(s))) == decode(s).
/// This is the core property required for blueprints to survive the
/// import → export → re-import workflow without any data loss.
#[test]
fn test_encode_decode_idempotent() {
    // Use a real-world blueprint that exercises all interesting fields:
    // connections, control_behavior, items, wires, and blueprints-level wires.
    assert_roundtrip(COMPLEX_CIRCUIT);

    // Also verify a tiles blueprint and a blueprint book survive idempotently.
    assert_roundtrip(TILES_BLUEPRINT);
    assert_roundtrip(BLUEPRINT_BOOK);
}

#[test]
fn test_single_belt() {
    assert_roundtrip(SINGLE_BELT);
    assert_json_equivalent(SINGLE_BELT);

    // Verify expected structure
    let data = decode(SINGLE_BELT).unwrap();
    let bp = data.blueprint.expect("should be a blueprint");
    assert_eq!(bp.entities.len(), 1);
    assert_eq!(bp.entities[0].name, "transport-belt");
}

#[test]
fn test_assembler_setup() {
    assert_roundtrip(ASSEMBLER_SETUP);
    assert_json_equivalent(ASSEMBLER_SETUP);

    // Verify recipe and direction fields survive
    let data = decode(ASSEMBLER_SETUP).unwrap();
    let bp = data.blueprint.expect("should be a blueprint");
    let assembler = bp
        .entities
        .iter()
        .find(|e| e.name == "assembling-machine-2")
        .expect("should have an assembler");
    assert_eq!(assembler.recipe.as_deref(), Some("iron-gear-wheel"));
    // This test blueprint was created with Factorio 1.x direction encoding
    // (2 = East). In Factorio 2.0's 16-direction scheme, value 2 = NorthEast.
    // The round-trip is faithful — the raw value is preserved.
    assert_eq!(assembler.direction, factorio_blueprint::Direction::NorthEast);

    // Verify inserters are present with correct directions
    let inserters: Vec<_> = bp
        .entities
        .iter()
        .filter(|e| e.name == "inserter")
        .collect();
    assert_eq!(inserters.len(), 2);
}

#[test]
fn test_underground_belts() {
    assert_roundtrip(UNDERGROUND_BELTS);
    assert_json_equivalent(UNDERGROUND_BELTS);

    // Verify entity "type": "input"/"output"
    let data = decode(UNDERGROUND_BELTS).unwrap();
    let bp = data.blueprint.expect("should be a blueprint");
    let undergrounds: Vec<_> = bp
        .entities
        .iter()
        .filter(|e| e.name == "underground-belt")
        .collect();
    assert_eq!(undergrounds.len(), 2);

    let types: Vec<_> = undergrounds
        .iter()
        .map(|e| e.entity_type.as_deref().unwrap())
        .collect();
    assert!(types.contains(&"input"));
    assert!(types.contains(&"output"));
}

#[test]
fn test_blueprint_with_tiles() {
    assert_roundtrip(TILES_BLUEPRINT);
    assert_json_equivalent(TILES_BLUEPRINT);

    // Verify tiles are present
    let data = decode(TILES_BLUEPRINT).unwrap();
    let bp = data.blueprint.expect("should be a blueprint");
    assert!(!bp.tiles.is_empty(), "tiles should not be empty");
    assert_eq!(bp.tiles.len(), 8);

    let tile_names: Vec<_> = bp.tiles.iter().map(|t| t.name.as_str()).collect();
    assert!(tile_names.contains(&"stone-path"));
    assert!(tile_names.contains(&"concrete"));
    assert!(tile_names.contains(&"refined-concrete"));
}

#[test]
fn test_complex_with_circuits() {
    assert_roundtrip(COMPLEX_CIRCUIT);
    assert_json_equivalent(COMPLEX_CIRCUIT);

    // Verify circuit connections, modules, and control_behavior survive
    let data = decode(COMPLEX_CIRCUIT).unwrap();
    let bp = data.blueprint.expect("should be a blueprint");

    // Combinators should have control_behavior
    let arithmetic = bp
        .entities
        .iter()
        .find(|e| e.name == "arithmetic-combinator")
        .expect("should have arithmetic combinator");
    assert!(
        arithmetic.control_behavior.is_some(),
        "arithmetic combinator should have control_behavior"
    );

    let decider = bp
        .entities
        .iter()
        .find(|e| e.name == "decider-combinator")
        .expect("should have decider combinator");
    assert!(
        decider.control_behavior.is_some(),
        "decider combinator should have control_behavior"
    );

    // Constant combinator should have control_behavior
    let constant = bp
        .entities
        .iter()
        .find(|e| e.name == "constant-combinator")
        .expect("should have constant combinator");
    assert!(
        constant.control_behavior.is_some(),
        "constant combinator should have control_behavior"
    );

    // Assembler should have recipe and modules
    let assembler = bp
        .entities
        .iter()
        .find(|e| e.name == "assembling-machine-3")
        .expect("should have assembler");
    assert_eq!(assembler.recipe.as_deref(), Some("electronic-circuit"));
    assert!(
        assembler.items.is_some(),
        "assembler should have module items"
    );

    // Electric pole should have connections
    let pole = bp
        .entities
        .iter()
        .find(|e| e.name == "medium-electric-pole")
        .expect("should have electric pole");
    assert!(
        pole.connections.is_some(),
        "electric pole should have connections"
    );

    // Blueprint-level wires should survive
    assert!(bp.wires.is_some(), "blueprint should have wires");
}
