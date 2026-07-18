//! Integration tests with real Factorio blueprint strings.
//!
//! Validates that decode -> encode -> decode round-trips preserve all data,
//! using both struct equality and JSON value equality.

use factorio_blueprint::fixtures::{
    ASSEMBLER_SETUP, BLUEPRINT_BOOK, COMPLEX_CIRCUIT, SINGLE_BELT, TILES_BLUEPRINT,
    UNDERGROUND_BELTS,
};
use factorio_blueprint::{decode, decode_to_json, encode};

// -- Helpers ---------------------------------------------------------------

/// Decode -> encode -> decode again, assert struct equality.
fn assert_roundtrip(s: &str) {
    let first = decode(s).unwrap_or_else(|e| panic!("first decode failed: {e}"));
    let encoded = encode(&first).unwrap_or_else(|e| panic!("encode failed: {e}"));
    let second = decode(&encoded).unwrap_or_else(|e| panic!("second decode failed: {e}"));
    assert_eq!(first, second, "struct mismatch after round-trip");
}

/// Decode original to JSON, then decode -> encode -> decode to JSON.
/// Parse both as serde_json::Value, normalize numbers, and assert equality.
///
/// Number normalization is necessary because Factorio JSON uses integers for
/// whole-number positions (e.g. `"x": 0`) but our Position type uses f64,
/// so re-serialization produces `"x": 0.0`. Both are semantically identical.
fn assert_json_equivalent(s: &str) {
    let original_json = decode_to_json(s).unwrap_or_else(|e| panic!("decode_to_json failed: {e}"));
    let original_value: serde_json::Value =
        serde_json::from_str(&original_json).expect("original JSON parse failed");

    let data = decode(s).unwrap_or_else(|e| panic!("decode failed: {e}"));
    let re_encoded = encode(&data).unwrap_or_else(|e| panic!("encode failed: {e}"));
    let roundtrip_json =
        decode_to_json(&re_encoded).unwrap_or_else(|e| panic!("decode_to_json roundtrip failed: {e}"));
    let roundtrip_value: serde_json::Value =
        serde_json::from_str(&roundtrip_json).expect("roundtrip JSON parse failed");

    assert_eq!(
        normalize_numbers(&original_value),
        normalize_numbers(&roundtrip_value),
        "JSON mismatch after round-trip"
    );
}

/// Recursively normalize all JSON numbers to f64 so that `0` and `0.0`
/// compare as equal (they represent the same value in Factorio data).
fn normalize_numbers(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            serde_json::Value::Number(
                serde_json::Number::from_f64(f).unwrap_or_else(|| serde_json::Number::from(0)),
            )
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(normalize_numbers).collect())
        }
        serde_json::Value::Object(map) => {
            serde_json::Value::Object(map.iter().map(|(k, v)| (k.clone(), normalize_numbers(v))).collect())
        }
        other => other.clone(),
    }
}

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
fn test_blueprint_book() {
    assert_roundtrip(BLUEPRINT_BOOK);
    assert_json_equivalent(BLUEPRINT_BOOK);

    // Verify it's a book with 3 blueprints
    let data = decode(BLUEPRINT_BOOK).unwrap();
    assert!(data.blueprint.is_none(), "should not be a single blueprint");
    let book = data.blueprint_book.expect("should be a blueprint book");
    assert_eq!(book.blueprints.len(), 3);
    assert_eq!(book.label.as_deref(), Some("Test Book"));
    assert_eq!(book.active_index, 0);

    // Verify individual blueprint labels
    assert_eq!(
        book.blueprints[0].blueprint.label.as_deref(),
        Some("Belt 1")
    );
    assert_eq!(
        book.blueprints[1].blueprint.label.as_deref(),
        Some("Belt 2")
    );
    assert_eq!(
        book.blueprints[2].blueprint.label.as_deref(),
        Some("Underground")
    );
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
