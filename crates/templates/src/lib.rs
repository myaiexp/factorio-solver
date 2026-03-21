pub use factorio_grid;
pub use factorio_grid::factorio_blueprint;

use factorio_grid::factorio_blueprint::Direction;
use factorio_grid::GridPos;
use serde::{Deserialize, Serialize};

// ── TemplateEntity ────────────────────────────────────────────────────

/// One entity within a template, with its position expressed relative to the
/// template's top-left origin (0, 0).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateEntity {
    /// Prototype name from the Factorio registry (e.g. "transport-belt").
    pub prototype_name: String,
    /// Top-left cell of the entity's footprint, relative to the template origin.
    pub relative_pos: GridPos,
    /// Facing direction (Factorio 2.0 16-direction scheme).
    pub direction: Direction,
    /// Footprint size in cells: (width, height).
    pub size: (u32, u32),
    /// Assembler/furnace recipe, if any.
    pub recipe: Option<String>,
    /// Entity type tag (e.g. "assembling-machine"), if available.
    pub entity_type: Option<String>,
}

// ── IoRole ────────────────────────────────────────────────────────────

/// The role of a connection point on the template boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IoRole {
    BeltInput,
    BeltOutput,
    FluidInput,
    FluidOutput,
}

// ── IoPoint ───────────────────────────────────────────────────────────

/// A named connection point on the template boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoPoint {
    /// Human-readable name (e.g. "iron-in", "output-north").
    pub name: String,
    /// Cell position relative to the template origin.
    pub relative_pos: GridPos,
    /// Whether this point is an input or output, and whether belt or fluid.
    pub role: IoRole,
}

// ── Template ──────────────────────────────────────────────────────────

/// A named, reusable layout fragment that can be placed by the solver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    /// Human-readable identifier.
    pub name: String,
    /// Bounding-box width in cells.
    pub width: u32,
    /// Bounding-box height in cells.
    pub height: u32,
    /// All entities that make up this template.
    pub entities: Vec<TemplateEntity>,
    /// Connection points on the template boundary (filled in by the user via UI).
    pub io_points: Vec<IoPoint>,
}

// ── Template extraction ───────────────────────────────────────────────

/// Extract a `Template` from a rectangular region of a `Grid`.
///
/// All entities whose footprint overlaps `[min_x, max_x] × [min_y, max_y]`
/// are included. Their positions are remapped so that `(min_x, min_y)` maps
/// to `(0, 0)` in the template coordinate space.
///
/// `io_points` is left empty — the user assigns connection points in the UI.
pub fn extract_template(
    grid: &factorio_grid::Grid,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    name: &str,
) -> Template {
    let entities = grid.entities_in_region(min_x, min_y, max_x, max_y);

    let template_entities = entities
        .into_iter()
        .map(|e| TemplateEntity {
            prototype_name: e.prototype_name.to_string(),
            relative_pos: GridPos {
                x: e.position.x - min_x,
                y: e.position.y - min_y,
            },
            direction: e.direction,
            size: e.size,
            recipe: e.recipe.clone(),
            entity_type: e.entity_type.clone(),
        })
        .collect();

    Template {
        name: name.to_string(),
        width: (max_x - min_x + 1) as u32,
        height: (max_y - min_y + 1) as u32,
        entities: template_entities,
        io_points: vec![],
    }
}

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

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entity(x: i32, y: i32) -> TemplateEntity {
        TemplateEntity {
            prototype_name: "transport-belt".to_string(),
            relative_pos: GridPos { x, y },
            direction: Direction::North,
            size: (1, 1),
            recipe: None,
            entity_type: Some("transport-belt".to_string()),
        }
    }

    // ── IoRole variant coverage ───────────────────────────────────────

    #[test]
    fn io_role_variants_serialize() {
        let roles = [
            IoRole::BeltInput,
            IoRole::BeltOutput,
            IoRole::FluidInput,
            IoRole::FluidOutput,
        ];
        for role in &roles {
            let json = serde_json::to_string(role).expect("IoRole serialize");
            let back: IoRole = serde_json::from_str(&json).expect("IoRole deserialize");
            assert_eq!(role, &back);
        }
    }

    // ── Empty template round-trip ─────────────────────────────────────

    #[test]
    fn empty_template_round_trip() {
        let t = Template {
            name: "empty".to_string(),
            width: 0,
            height: 0,
            entities: vec![],
            io_points: vec![],
        };
        let json = serde_json::to_string(&t).expect("serialize");
        let back: Template = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, "empty");
        assert_eq!(back.width, 0);
        assert_eq!(back.height, 0);
        assert!(back.entities.is_empty());
        assert!(back.io_points.is_empty());
    }

    // ── Template with entities and io_points round-trip ───────────────

    #[test]
    fn template_with_entities_and_io_round_trip() {
        let t = Template {
            name: "iron-smelter".to_string(),
            width: 6,
            height: 4,
            entities: vec![
                make_entity(0, 0),
                TemplateEntity {
                    prototype_name: "stone-furnace".to_string(),
                    relative_pos: GridPos { x: 2, y: 1 },
                    direction: Direction::South,
                    size: (2, 2),
                    recipe: None,
                    entity_type: Some("furnace".to_string()),
                },
                TemplateEntity {
                    prototype_name: "inserter".to_string(),
                    relative_pos: GridPos { x: 4, y: 1 },
                    direction: Direction::East,
                    size: (1, 1),
                    recipe: Some("iron-plate".to_string()),
                    entity_type: None,
                },
            ],
            io_points: vec![
                IoPoint {
                    name: "iron-ore-in".to_string(),
                    relative_pos: GridPos { x: 0, y: 0 },
                    role: IoRole::BeltInput,
                },
                IoPoint {
                    name: "iron-plate-out".to_string(),
                    relative_pos: GridPos { x: 5, y: 0 },
                    role: IoRole::BeltOutput,
                },
            ],
        };

        let json = serde_json::to_string_pretty(&t).expect("serialize");
        let back: Template = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.name, "iron-smelter");
        assert_eq!(back.width, 6);
        assert_eq!(back.height, 4);
        assert_eq!(back.entities.len(), 3);
        assert_eq!(back.io_points.len(), 2);

        // Spot-check entity fields
        let furnace = &back.entities[1];
        assert_eq!(furnace.prototype_name, "stone-furnace");
        assert_eq!(furnace.relative_pos.x, 2);
        assert_eq!(furnace.relative_pos.y, 1);
        assert_eq!(furnace.size, (2, 2));

        // Spot-check with recipe
        let inserter = &back.entities[2];
        assert_eq!(inserter.recipe, Some("iron-plate".to_string()));
        assert!(inserter.entity_type.is_none());

        // Spot-check io_points
        assert_eq!(back.io_points[0].role, IoRole::BeltInput);
        assert_eq!(back.io_points[1].name, "iron-plate-out");
    }

    // ── Direction survives round-trip ─────────────────────────────────

    #[test]
    fn template_entity_direction_round_trip() {
        let directions = [
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
        ];
        for dir in directions {
            let e = TemplateEntity {
                prototype_name: "transport-belt".to_string(),
                relative_pos: GridPos { x: 0, y: 0 },
                direction: dir,
                size: (1, 1),
                recipe: None,
                entity_type: None,
            };
            let json = serde_json::to_string(&e).expect("serialize");
            let back: TemplateEntity = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back.direction, dir);
        }
    }

    // ── extract_template tests ─────────────────────────────────────────

    /// Center position for a 1×1 entity placed at the given top-left grid cell.
    /// Factorio uses center-based coordinates: for a 1×1 entity, center = top_left + 0.5.
    fn center_1x1(top_left_x: i32, top_left_y: i32) -> factorio_blueprint::Position {
        factorio_blueprint::Position {
            x: top_left_x as f64 + 0.5,
            y: top_left_y as f64 + 0.5,
        }
    }

    /// Positions are remapped so that (min_x, min_y) in world space becomes (0, 0)
    /// in the resulting template.
    #[test]
    fn extract_template_remaps_positions_to_region_origin() {
        let mut grid = factorio_grid::Grid::new();
        // Belt at world top-left (5, 3)
        grid.place("transport-belt", &center_1x1(5, 3), Direction::North, None, None)
            .unwrap();

        // Extract region starting at (3, 1) → origin becomes (0, 0)
        let t = extract_template(&grid, 3, 1, 8, 5, "remap-test");

        assert_eq!(t.entities.len(), 1);
        // Belt world top-left = (5, 3); relative = (5 - 3, 3 - 1) = (2, 2)
        assert_eq!(t.entities[0].relative_pos, GridPos { x: 2, y: 2 });
    }

    /// width = max_x - min_x + 1, height = max_y - min_y + 1 (inclusive region).
    #[test]
    fn extract_template_width_and_height_equal_region_extents() {
        // No entities needed — dimensions come entirely from the region coordinates.
        let grid = factorio_grid::Grid::new();
        // Region (2, 4) → (8, 11): width = 7, height = 8
        let t = extract_template(&grid, 2, 4, 8, 11, "dims-test");
        assert_eq!(t.width, 7); // 8 - 2 + 1
        assert_eq!(t.height, 8); // 11 - 4 + 1
    }

    /// prototype_name, direction, size, and recipe are all faithfully copied from
    /// the PlacedEntity into the TemplateEntity.
    #[test]
    fn extract_template_copies_entity_fields() {
        let mut grid = factorio_grid::Grid::new();

        // Inserter (1×1) at world top-left (1, 0), facing East, no recipe
        grid.place(
            "inserter",
            &center_1x1(1, 0),
            Direction::East,
            None,
            Some("inserter".to_string()),
        )
        .unwrap();

        // Assembler-2 (3×3) at world top-left (4, 0):
        //   center = (4 + 1.5, 0 + 1.5) = (5.5, 1.5)
        grid.place(
            "assembling-machine-2",
            &factorio_blueprint::Position { x: 5.5, y: 1.5 },
            Direction::South,
            Some("iron-gear-wheel".to_string()),
            None,
        )
        .unwrap();

        // Extract region (0, 0) → (9, 2) — both entities fit inside
        let t = extract_template(&grid, 0, 0, 9, 2, "fields-test");

        assert_eq!(t.entities.len(), 2);

        let inserter = t
            .entities
            .iter()
            .find(|e| e.prototype_name == "inserter")
            .expect("inserter not found");
        assert_eq!(inserter.direction, Direction::East);
        assert_eq!(inserter.size, (1, 1));
        assert!(inserter.recipe.is_none());
        // Inserter world top-left (1, 0), region origin (0, 0) → relative (1, 0)
        assert_eq!(inserter.relative_pos, GridPos { x: 1, y: 0 });

        let asm = t
            .entities
            .iter()
            .find(|e| e.prototype_name == "assembling-machine-2")
            .expect("assembler not found");
        assert_eq!(asm.size, (3, 3));
        assert_eq!(asm.direction, Direction::South);
        assert_eq!(asm.recipe, Some("iron-gear-wheel".to_string()));
        // Assembler world top-left (4, 0), region origin (0, 0) → relative (4, 0)
        assert_eq!(asm.relative_pos, GridPos { x: 4, y: 0 });
    }

    /// Extracting a region with no entities produces an empty entity list,
    /// empty io_points, the given name, and correct dimensions.
    #[test]
    fn extract_template_empty_region_produces_no_entities() {
        let grid = factorio_grid::Grid::new();
        let t = extract_template(&grid, 0, 0, 3, 3, "empty-region");
        assert!(t.entities.is_empty());
        assert!(t.io_points.is_empty());
        assert_eq!(t.name, "empty-region");
        assert_eq!(t.width, 4); // 3 - 0 + 1
        assert_eq!(t.height, 4);
    }

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

        let json = save_to_json(&[t.clone()]).expect("save single template");

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
                entity_type: Some("furnace".to_string()),
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
