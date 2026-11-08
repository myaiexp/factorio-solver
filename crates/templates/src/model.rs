// The template model: entities relative to an origin, boundary I/O points, and the template itself.
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
    /// Factorio entity JSON `type` field (not a prototype category).
    /// Used by underground belts (`"input"` / `"output"`) and similar
    /// dual-role entities; `None` for ordinary belts, assemblers, etc.
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

#[cfg(test)]
mod tests {
    use crate::*;
    use factorio_grid::factorio_blueprint::Direction;
    use factorio_grid::GridPos;
    use crate::testsupport::make_entity;

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
                    entity_type: None,
                },
                TemplateEntity {
                    prototype_name: "underground-belt".to_string(),
                    relative_pos: GridPos { x: 4, y: 1 },
                    direction: Direction::East,
                    size: (1, 1),
                    recipe: None,
                    // Factorio `type` for underground belts is input/output.
                    entity_type: Some("input".to_string()),
                },
                TemplateEntity {
                    prototype_name: "inserter".to_string(),
                    relative_pos: GridPos { x: 5, y: 1 },
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
        assert_eq!(back.entities.len(), 4);
        assert_eq!(back.io_points.len(), 2);

        // Spot-check entity fields
        let furnace = &back.entities[1];
        assert_eq!(furnace.prototype_name, "stone-furnace");
        assert_eq!(furnace.relative_pos.x, 2);
        assert_eq!(furnace.relative_pos.y, 1);
        assert_eq!(furnace.size, (2, 2));
        assert!(furnace.entity_type.is_none());

        // Factorio `type` (input/output), not a prototype category tag
        let ug_belt = &back.entities[2];
        assert_eq!(ug_belt.prototype_name, "underground-belt");
        assert_eq!(ug_belt.entity_type.as_deref(), Some("input"));

        // Spot-check with recipe
        let inserter = &back.entities[3];
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
}
