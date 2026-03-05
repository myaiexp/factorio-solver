use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

use crate::error::BlueprintError;
use crate::types::BlueprintData;

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

    let mut decoder = ZlibDecoder::new(&compressed[..]);
    let mut json = String::new();
    decoder.read_to_string(&mut json)?;

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
    use std::collections::HashMap;

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

    // ── Happy path tests ──────────────────────────────────────────────

    #[test]
    fn test_manual_encode_decode_roundtrip() {
        let data = BlueprintData {
            blueprint: Some(Blueprint {
                item: "blueprint".to_string(),
                label: Some("Test Blueprint".to_string()),
                label_color: None,
                description: None,
                icons: None,
                entities: vec![Entity {
                    entity_number: 1,
                    name: "transport-belt".to_string(),
                    position: Position { x: 0.5, y: 0.5 },
                    direction: crate::types::Direction::East,
                    entity_type: None,
                    recipe: None,
                    connections: None,
                    control_behavior: None,
                    items: None,
                    wires: None,
                    tags: None,
                    extra: HashMap::new(),
                }],
                tiles: vec![],
                wires: None,
                schedules: None,
                snap_to_grid: None,
                absolute_snapping: None,
                position_relative_to_grid: None,
                version: 281479275675648,
                extra: HashMap::new(),
            }),
            blueprint_book: None,
        };

        let encoded = encode(&data).unwrap();
        assert!(encoded.starts_with('0'));

        let decoded = decode(&encoded).unwrap();
        assert_eq!(data, decoded);
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
