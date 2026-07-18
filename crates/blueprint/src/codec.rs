use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

use crate::error::BlueprintError;
use crate::types::BlueprintData;

/// Upper bound on decompressed blueprint JSON. Zlib compresses ~1000:1, so a
/// tiny pasted string can expand to gigabytes (a "zip bomb"). The decoder is
/// capped at this many bytes so a malicious or corrupt blueprint can't OOM or
/// freeze the app — decode fails fast with `DecompressedTooLarge` instead. Real
/// blueprints decompress to a few MB at most; 64 MiB is comfortably generous.
pub const DECOMPRESSED_LIMIT: usize = 64 * 1024 * 1024;

/// Decode a Factorio blueprint string into a `BlueprintData` struct.
///
/// Pipeline: strip version byte → base64 decode → zlib decompress → JSON parse.
pub fn decode(blueprint_string: &str) -> Result<BlueprintData, BlueprintError> {
    let json = decode_to_json(blueprint_string)?;
    let data: BlueprintData = serde_json::from_str(&json)?;
    Ok(data)
}

/// Decode a Factorio blueprint string into raw JSON (for debugging/pretty-printing).
///
/// Pipeline: strip version byte → base64 decode → zlib decompress → return JSON string.
pub fn decode_to_json(blueprint_string: &str) -> Result<String, BlueprintError> {
    if blueprint_string.is_empty() {
        return Err(BlueprintError::MissingVersionByte);
    }

    let version_byte = blueprint_string.chars().next().unwrap();
    if version_byte != '0' {
        return Err(BlueprintError::UnsupportedVersion(version_byte));
    }

    let encoded = &blueprint_string[1..];
    let compressed = STANDARD.decode(encoded)?;

    // Cap decompression at DECOMPRESSED_LIMIT: read one byte past the limit and,
    // if the reader still had data, reject as a zip bomb. `take` bounds the work
    // to LIMIT+1 bytes regardless of how large the stream would expand to, so
    // memory stays bounded even for adversarial input. Read into a byte buffer
    // (not read_to_string) so the size check runs before UTF-8 validation and an
    // oversized paste reports DecompressedTooLarge rather than a spurious
    // invalid-UTF-8 error from a mid-character truncation.
    let mut decoder = ZlibDecoder::new(&compressed[..]).take(DECOMPRESSED_LIMIT as u64 + 1);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf)?;
    if buf.len() > DECOMPRESSED_LIMIT {
        return Err(BlueprintError::DecompressedTooLarge {
            limit: DECOMPRESSED_LIMIT,
        });
    }

    let json = String::from_utf8(buf).map_err(|e| {
        BlueprintError::Zlib(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })?;

    Ok(json)
}

/// Encode a `BlueprintData` struct into a Factorio blueprint string.
///
/// Pipeline: JSON serialize → zlib compress (level 9) → base64 encode → prepend '0'.
pub fn encode(data: &BlueprintData) -> Result<String, BlueprintError> {
    let json = serde_json::to_string(data)?;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(json.as_bytes())?;
    let compressed = encoder.finish()?;

    let encoded = STANDARD.encode(&compressed);

    Ok(format!("0{encoded}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Blueprint, Entity, Position};

    // ── Error path tests ──────────────────────────────────────────────

    #[test]
    fn test_decode_empty_string() {
        let result = decode("");
        assert!(matches!(result, Err(BlueprintError::MissingVersionByte)));
    }

    #[test]
    fn test_decode_bad_version() {
        let result = decode("1eJxLZmBgYAIAAAoABQ==");
        assert!(matches!(result, Err(BlueprintError::UnsupportedVersion('1'))));
    }

    #[test]
    fn test_decode_invalid_base64() {
        let result = decode("0!!!not-base64!!!");
        assert!(matches!(result, Err(BlueprintError::Base64(_))));
    }

    #[test]
    fn test_decode_invalid_zlib() {
        // Valid base64 of garbage bytes (not valid zlib)
        let result = decode("0aGVsbG8=");
        assert!(matches!(result, Err(BlueprintError::Zlib(_))));
    }

    #[test]
    fn test_decode_invalid_json() {
        // Manually create: valid base64 of valid zlib of "not json"
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"not json").unwrap();
        let compressed = encoder.finish().unwrap();
        let encoded = STANDARD.encode(&compressed);
        let blueprint_string = format!("0{encoded}");

        let result = decode(&blueprint_string);
        assert!(matches!(result, Err(BlueprintError::Json(_))));
    }

    #[test]
    fn test_decode_rejects_zip_bomb() {
        // A tiny pasted string that decompresses past the limit must be rejected
        // before it can exhaust memory. Highly-repetitive data compresses ~1000:1,
        // so this "bomb" is only a few KB encoded but expands past 64 MiB.
        let bomb = vec![b' '; DECOMPRESSED_LIMIT + 1];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&bomb).unwrap();
        let compressed = encoder.finish().unwrap();
        let encoded = STANDARD.encode(&compressed);
        // Sanity: the compressed paste is tiny relative to what it expands to.
        assert!(encoded.len() < 1024 * 1024, "compressed bomb should stay small");
        let blueprint_string = format!("0{encoded}");

        let result = decode_to_json(&blueprint_string);
        assert!(matches!(
            result,
            Err(BlueprintError::DecompressedTooLarge { limit }) if limit == DECOMPRESSED_LIMIT
        ));

        // decode() (which also parses JSON) must reject it too.
        assert!(matches!(
            decode(&blueprint_string),
            Err(BlueprintError::DecompressedTooLarge { .. })
        ));
    }

    #[test]
    fn test_decode_accepts_at_limit() {
        // Content that decompresses to exactly the limit must still succeed —
        // the guard rejects only strictly-larger payloads (LIMIT is inclusive).
        // Use valid JSON padded to exactly DECOMPRESSED_LIMIT bytes.
        let padding = DECOMPRESSED_LIMIT - r#"{"blueprint":{"item":"blueprint","version":0,"_pad":""}}"#.len();
        let json = format!(
            r#"{{"blueprint":{{"item":"blueprint","version":0,"_pad":"{}"}}}}"#,
            " ".repeat(padding)
        );
        assert_eq!(json.len(), DECOMPRESSED_LIMIT);

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(json.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        let blueprint_string = format!("0{}", STANDARD.encode(&compressed));

        let data = decode(&blueprint_string).expect("exactly-at-limit blueprint should decode");
        assert_eq!(data.blueprint.unwrap().item, "blueprint");
    }

    // ── Happy path tests ──────────────────────────────────────────────

    #[test]
    fn test_manual_encode_decode_roundtrip() {
        let data = BlueprintData {
            blueprint: Some(Blueprint {
                item: "blueprint".to_string(),
                label: Some("Test Blueprint".to_string()),
                entities: vec![Entity {
                    entity_number: 1,
                    name: "transport-belt".to_string(),
                    position: Position { x: 0.5, y: 0.5 },
                    direction: crate::types::Direction::East,
                    ..Default::default()
                }],
                version: 281479275675648,
                ..Default::default()
            }),
            ..Default::default()
        };

        let encoded = encode(&data).unwrap();
        assert!(encoded.starts_with('0'));

        let decoded = decode(&encoded).unwrap();
        assert_eq!(data, decoded);
    }

    // ── Regression tests: round-trip fidelity for circuit network fields ─

    #[test]
    fn test_roundtrip_entity_connections() {
        let connections_val =
            serde_json::json!({"1": {"red": [{"entity_id": 2, "circuit_id": 1}]}});

        let data = BlueprintData {
            blueprint: Some(Blueprint {
                item: "blueprint".to_string(),
                entities: vec![Entity {
                    entity_number: 1,
                    name: "arithmetic-combinator".to_string(),
                    position: Position { x: 0.5, y: 0.0 },
                    connections: Some(connections_val.clone()),
                    ..Default::default()
                }],
                version: 281479275675648,
                ..Default::default()
            }),
            ..Default::default()
        };

        let encoded = encode(&data).unwrap();
        let decoded = decode(&encoded).unwrap();

        let entity = &decoded.blueprint.unwrap().entities[0];
        assert_eq!(entity.connections, Some(connections_val));
    }

    #[test]
    fn test_roundtrip_blueprint_wires() {
        let wires_val = serde_json::json!([[1, 1, 2, 1, 1]]);

        let data = BlueprintData {
            blueprint: Some(Blueprint {
                item: "blueprint".to_string(),
                wires: Some(wires_val.clone()),
                version: 281479275675648,
                ..Default::default()
            }),
            ..Default::default()
        };

        let encoded = encode(&data).unwrap();
        let decoded = decode(&encoded).unwrap();

        assert_eq!(decoded.blueprint.unwrap().wires, Some(wires_val));
    }

    #[test]
    fn test_roundtrip_control_behavior() {
        let cb_val = serde_json::json!({
            "circuit_condition": {
                "first_signal": {"name": "iron-ore", "type": "item"},
                "comparator": ">",
                "constant": 100
            },
            "circuit_enable_disable": true
        });

        let data = BlueprintData {
            blueprint: Some(Blueprint {
                item: "blueprint".to_string(),
                entities: vec![Entity {
                    entity_number: 1,
                    name: "inserter".to_string(),
                    position: Position { x: 0.5, y: 0.5 },
                    control_behavior: Some(cb_val.clone()),
                    ..Default::default()
                }],
                version: 281479275675648,
                ..Default::default()
            }),
            ..Default::default()
        };

        let encoded = encode(&data).unwrap();
        let decoded = decode(&encoded).unwrap();

        let entity = &decoded.blueprint.unwrap().entities[0];
        assert_eq!(entity.control_behavior, Some(cb_val));
    }

    #[test]
    fn test_manual_minimal_blueprint() {
        // Hand-craft the JSON, compress, encode, then verify decode matches
        let json = r#"{"blueprint":{"item":"blueprint","entities":[],"version":0}}"#;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(json.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        let encoded = STANDARD.encode(&compressed);
        let blueprint_string = format!("0{encoded}");

        let data = decode(&blueprint_string).unwrap();
        let bp = data.blueprint.unwrap();
        assert_eq!(bp.item, "blueprint");
        assert!(bp.entities.is_empty());
        assert_eq!(bp.version, 0);
    }
}
