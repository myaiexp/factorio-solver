use factorio_blueprint::Blueprint;

use crate::grid::Grid;
use crate::prototype::lookup;

// ── Import result types ─────────────────────────────────────────────

/// Result of importing a blueprint into a grid.
pub struct ImportResult {
    /// The populated grid with all recognized entities placed.
    pub grid: Grid,
    /// Entities that could not be placed (unknown prototype, etc.).
    pub skipped: Vec<SkippedEntity>,
}

/// An entity from the blueprint that was skipped during import.
pub struct SkippedEntity {
    /// The entity_number from the blueprint.
    pub entity_number: u32,
    /// The entity name (prototype name).
    pub name: String,
    /// Why the entity was skipped.
    pub reason: String,
}

// ── Import function ─────────────────────────────────────────────────

/// Build a Grid from a decoded Blueprint.
///
/// Iterates all entities in the blueprint, looks up each prototype via
/// `crate::prototype::lookup()`, and calls `grid.place()`. Unknown
/// prototypes are gracefully skipped and collected in `ImportResult.skipped`.
///
/// This function never panics on valid blueprints — real Factorio blueprints
/// contain non-overlapping entities, and unknown entities are simply skipped.
pub fn from_blueprint(blueprint: &Blueprint) -> ImportResult {
    let mut grid = Grid::new();
    let mut skipped = Vec::new();

    for entity in &blueprint.entities {
        // Check if we know this prototype
        if lookup(&entity.name).is_none() {
            skipped.push(SkippedEntity {
                entity_number: entity.entity_number,
                name: entity.name.clone(),
                reason: format!("unknown prototype: {}", entity.name),
            });
            continue;
        }

        // Attempt placement
        match grid.place(
            &entity.name,
            &entity.position,
            entity.direction,
            entity.recipe.clone(),
            entity.entity_type.clone(),
        ) {
            Ok(_) => {}
            Err(e) => {
                skipped.push(SkippedEntity {
                    entity_number: entity.entity_number,
                    name: entity.name.clone(),
                    reason: format!("{}", e),
                });
            }
        }
    }

    ImportResult { grid, skipped }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use factorio_blueprint::{
        Blueprint, Direction, Entity, Position,
    };
    use std::collections::HashMap;

    /// Helper to build a minimal Blueprint with the given entities.
    fn make_blueprint(entities: Vec<Entity>) -> Blueprint {
        Blueprint {
            item: "blueprint".to_string(),
            label: None,
            label_color: None,
            description: None,
            icons: None,
            entities,
            tiles: vec![],
            wires: None,
            schedules: None,
            snap_to_grid: None,
            absolute_snapping: None,
            position_relative_to_grid: None,
            version: 281479275675648,
            extra: HashMap::new(),
        }
    }

    /// Helper to build an Entity with minimal required fields.
    fn make_entity(
        entity_number: u32,
        name: &str,
        x: f64,
        y: f64,
        direction: Direction,
    ) -> Entity {
        Entity {
            entity_number,
            name: name.to_string(),
            position: Position { x, y },
            direction,
            entity_type: None,
            recipe: None,
            connections: None,
            control_behavior: None,
            items: None,
            wires: None,
            tags: None,
            extra: HashMap::new(),
        }
    }

    #[test]
    fn test_import_empty_blueprint() {
        let bp = make_blueprint(vec![]);
        let result = from_blueprint(&bp);
        assert_eq!(result.grid.entity_count(), 0);
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn test_import_single_known_entity() {
        let bp = make_blueprint(vec![
            make_entity(1, "transport-belt", 0.5, 0.5, Direction::East),
        ]);
        let result = from_blueprint(&bp);
        assert_eq!(result.grid.entity_count(), 1);
        assert!(result.skipped.is_empty());

        let entity = result.grid.entities().next().unwrap();
        assert_eq!(entity.prototype_name, "transport-belt");
        assert_eq!(entity.direction, Direction::East);
    }

    #[test]
    fn test_import_unknown_entity_skipped() {
        let bp = make_blueprint(vec![
            make_entity(1, "modded-turret", 0.5, 0.5, Direction::North),
        ]);
        let result = from_blueprint(&bp);
        assert_eq!(result.grid.entity_count(), 0);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].entity_number, 1);
        assert_eq!(result.skipped[0].name, "modded-turret");
        assert!(result.skipped[0].reason.contains("unknown prototype"));
    }

    #[test]
    fn test_import_mixed_known_and_unknown() {
        let bp = make_blueprint(vec![
            make_entity(1, "transport-belt", 0.5, 0.5, Direction::North),
            make_entity(2, "alien-artifact", 1.5, 0.5, Direction::North),
            make_entity(3, "inserter", 2.5, 0.5, Direction::North),
        ]);
        let result = from_blueprint(&bp);
        assert_eq!(result.grid.entity_count(), 2);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].name, "alien-artifact");
    }

    #[test]
    fn test_import_preserves_recipe() {
        let mut entity = make_entity(1, "assembling-machine-2", 0.5, 0.5, Direction::North);
        entity.recipe = Some("iron-gear-wheel".to_string());
        let bp = make_blueprint(vec![entity]);

        let result = from_blueprint(&bp);
        assert_eq!(result.grid.entity_count(), 1);
        let placed = result.grid.entities().next().unwrap();
        assert_eq!(placed.recipe.as_deref(), Some("iron-gear-wheel"));
    }

    #[test]
    fn test_import_preserves_entity_type() {
        let mut e1 = make_entity(1, "underground-belt", 0.5, 0.5, Direction::North);
        e1.entity_type = Some("input".to_string());
        let mut e2 = make_entity(2, "underground-belt", 0.5, 5.5, Direction::North);
        e2.entity_type = Some("output".to_string());
        let bp = make_blueprint(vec![e1, e2]);

        let result = from_blueprint(&bp);
        assert_eq!(result.grid.entity_count(), 2);

        let types: Vec<_> = result
            .grid
            .entities()
            .map(|e| e.entity_type.as_deref().unwrap().to_string())
            .collect();
        assert!(types.contains(&"input".to_string()));
        assert!(types.contains(&"output".to_string()));
    }
}
