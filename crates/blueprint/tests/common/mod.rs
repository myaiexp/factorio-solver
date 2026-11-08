//! Round-trip assertions shared by the blueprint integration tests.

use factorio_blueprint::{decode, decode_to_json, encode};

/// Decode -> encode -> decode again, assert struct equality.
pub fn assert_roundtrip(s: &str) {
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
pub fn assert_json_equivalent(s: &str) {
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
