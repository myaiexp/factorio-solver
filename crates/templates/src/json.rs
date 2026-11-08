// JSON persistence for a template library: a top-level array of templates.
use crate::model::Template;

// ── JSON persistence ──────────────────────────────────────────────────

/// Serialize a slice of templates to a JSON string (top-level array).
///
/// # Errors
/// Returns `serde_json::Error` if serialization fails (practically infallible
/// for well-formed `Template` values, but the `Result` matches the serde API).
pub fn save_to_json(templates: &[Template]) -> Result<String, serde_json::Error> {
    serde_json::to_string(templates)
}

/// Deserialize a slice of templates from a JSON string.
///
/// The JSON must be a top-level array produced by [`save_to_json`].
///
/// # Errors
/// Returns `serde_json::Error` if the JSON is malformed or doesn't match the
/// `Template` schema.
pub fn load_from_json(json: &str) -> Result<Vec<Template>, serde_json::Error> {
    serde_json::from_str(json)
}

#[cfg(test)]
mod tests {
    use crate::*;
    use factorio_grid::factorio_blueprint::Direction;
    use factorio_grid::GridPos;
    use crate::testsupport::make_entity;

    // ── save_to_json / load_from_json tests ───────────────────────────

    /// An empty template library serializes and deserializes back to an empty
    /// Vec, and the JSON is a top-level array.
    #[test]
    fn json_persistence_empty_library_round_trip() {
        let templates: Vec<Template> = vec![];
        let json = save_to_json(&templates).expect("save empty library");

        // JSON must be a top-level array
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("parse json");
        assert!(value.is_array(), "expected JSON array, got {json}");
        assert_eq!(value.as_array().unwrap().len(), 0);

        // Round-trip back to typed Vec
        let reloaded = load_from_json(&json).expect("load empty library");
        assert!(reloaded.is_empty());
    }

    /// A library with a single template round-trips through save/load with all
    /// fields intact.
    #[test]
    fn json_persistence_single_template_round_trip() {
        let t = Template {
            name: "single-belt".to_string(),
            width: 3,
            height: 1,
            entities: vec![
                make_entity(0, 0),
                make_entity(1, 0),
                make_entity(2, 0),
            ],
            io_points: vec![
                IoPoint {
                    name: "in".to_string(),
                    relative_pos: GridPos { x: 0, y: 0 },
                    role: IoRole::BeltInput,
                },
                IoPoint {
                    name: "out".to_string(),
                    relative_pos: GridPos { x: 2, y: 0 },
                    role: IoRole::BeltOutput,
                },
            ],
        };

        let json = save_to_json(std::slice::from_ref(&t)).expect("save single template");

        // Top-level must be an array with exactly one element
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("parse json");
        assert!(value.is_array());
        assert_eq!(value.as_array().unwrap().len(), 1);

        let reloaded = load_from_json(&json).expect("load single template");
        assert_eq!(reloaded.len(), 1);

        let back = &reloaded[0];
        assert_eq!(back.name, "single-belt");
        assert_eq!(back.width, 3);
        assert_eq!(back.height, 1);
        assert_eq!(back.entities.len(), 3);
        assert_eq!(back.io_points.len(), 2);
        assert_eq!(back.io_points[0].role, IoRole::BeltInput);
        assert_eq!(back.io_points[1].name, "out");
    }

    /// A library with multiple templates round-trips completely, preserving
    /// ordering and all per-template fields.
    #[test]
    fn json_persistence_multiple_templates_round_trip() {
        let t1 = Template {
            name: "small-furnace".to_string(),
            width: 4,
            height: 4,
            entities: vec![TemplateEntity {
                prototype_name: "stone-furnace".to_string(),
                relative_pos: GridPos { x: 1, y: 1 },
                direction: Direction::North,
                size: (2, 2),
                recipe: None,
                entity_type: None,
            }],
            io_points: vec![IoPoint {
                name: "ore-in".to_string(),
                relative_pos: GridPos { x: 0, y: 1 },
                role: IoRole::BeltInput,
            }],
        };

        let t2 = Template {
            name: "gear-assembler".to_string(),
            width: 5,
            height: 5,
            entities: vec![TemplateEntity {
                prototype_name: "assembling-machine-2".to_string(),
                relative_pos: GridPos { x: 1, y: 1 },
                direction: Direction::South,
                size: (3, 3),
                recipe: Some("iron-gear-wheel".to_string()),
                entity_type: None,
            }],
            io_points: vec![
                IoPoint {
                    name: "plate-in".to_string(),
                    relative_pos: GridPos { x: 0, y: 2 },
                    role: IoRole::BeltInput,
                },
                IoPoint {
                    name: "gear-out".to_string(),
                    relative_pos: GridPos { x: 4, y: 2 },
                    role: IoRole::BeltOutput,
                },
            ],
        };

        let t3 = Template {
            name: "fluid-mixer".to_string(),
            width: 6,
            height: 3,
            entities: vec![],
            io_points: vec![
                IoPoint {
                    name: "water-in".to_string(),
                    relative_pos: GridPos { x: 0, y: 1 },
                    role: IoRole::FluidInput,
                },
                IoPoint {
                    name: "oil-out".to_string(),
                    relative_pos: GridPos { x: 5, y: 1 },
                    role: IoRole::FluidOutput,
                },
            ],
        };

        let library = vec![t1, t2, t3];
        let json = save_to_json(&library).expect("save multiple templates");

        // Verify JSON structure: top-level array with 3 elements
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("parse json");
        assert!(value.is_array(), "expected top-level JSON array");
        let arr = value.as_array().unwrap();
        assert_eq!(arr.len(), 3);

        // Each element should be a JSON object with a "name" field
        assert_eq!(arr[0]["name"], "small-furnace");
        assert_eq!(arr[1]["name"], "gear-assembler");
        assert_eq!(arr[2]["name"], "fluid-mixer");

        // Full round-trip
        let reloaded = load_from_json(&json).expect("load multiple templates");
        assert_eq!(reloaded.len(), 3);

        // Verify first template
        assert_eq!(reloaded[0].name, "small-furnace");
        assert_eq!(reloaded[0].entities.len(), 1);
        assert_eq!(reloaded[0].io_points[0].role, IoRole::BeltInput);

        // Verify second template — recipe preserved
        assert_eq!(reloaded[1].name, "gear-assembler");
        assert_eq!(
            reloaded[1].entities[0].recipe,
            Some("iron-gear-wheel".to_string())
        );
        assert_eq!(reloaded[1].io_points.len(), 2);

        // Verify third template — fluid roles preserved
        assert_eq!(reloaded[2].name, "fluid-mixer");
        assert!(reloaded[2].entities.is_empty());
        assert_eq!(reloaded[2].io_points[0].role, IoRole::FluidInput);
        assert_eq!(reloaded[2].io_points[1].role, IoRole::FluidOutput);
    }

    /// load_from_json returns an error (not a panic) for malformed JSON.
    #[test]
    fn json_persistence_load_malformed_json_returns_error() {
        let result = load_from_json("{ not valid json [[[");
        assert!(result.is_err(), "expected Err for malformed JSON");
    }

    /// load_from_json returns an error for valid JSON that isn't a template
    /// array (e.g. a plain object instead of an array).
    #[test]
    fn json_persistence_load_wrong_shape_returns_error() {
        // A JSON object at the top level is not a Vec<Template>
        let result = load_from_json(r#"{"name": "not-an-array"}"#);
        assert!(result.is_err(), "expected Err for non-array JSON");
    }
}
