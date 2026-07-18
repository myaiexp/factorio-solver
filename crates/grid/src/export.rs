// Export a populated Grid back into a Factorio Blueprint.

use std::collections::HashMap;

use factorio_blueprint::{Blueprint, Entity};

use crate::grid::Grid;

/// Build a `Blueprint` from every live entity on `grid`.
///
/// Each entity's stored center position, direction, recipe, and type are copied
/// straight back, so a `from_blueprint` → `to_blueprint` round-trip preserves
/// placement. `entity_number` is derived from the grid's 1-based entity id order;
/// ids are stable and never reused, so numbers are unique within the blueprint.
pub fn to_blueprint(grid: &Grid, label: Option<String>, version: u64) -> Blueprint {
    let entities = grid
        .entities()
        .map(|e| Entity {
            entity_number: e.id.0 as u32 + 1,
            name: e.prototype_name.to_string(),
            position: e.center.clone(),
            direction: e.direction,
            entity_type: e.entity_type.clone(),
            recipe: e.recipe.clone(),
            connections: None,
            control_behavior: None,
            items: None,
            wires: None,
            tags: None,
            extra: HashMap::new(),
        })
        .collect();

    Blueprint {
        item: "blueprint".to_string(),
        label,
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
        version,
        extra: HashMap::new(),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use factorio_blueprint::{Direction, Position};

    fn pos(x: f64, y: f64) -> Position {
        Position { x, y }
    }

    #[test]
    fn test_to_blueprint_preserves_entities() {
        let mut grid = Grid::new();
        grid.place("transport-belt", &pos(0.5, 0.5), Direction::East, None, None)
            .unwrap();
        grid.place(
            "assembling-machine-2",
            &pos(3.5, 1.5),
            Direction::North,
            Some("iron-gear-wheel".to_string()),
            None,
        )
        .unwrap();

        let bp = to_blueprint(&grid, Some("test".to_string()), 42);

        assert_eq!(bp.label.as_deref(), Some("test"));
        assert_eq!(bp.version, 42);
        assert_eq!(bp.entities.len(), 2);

        let belt = bp
            .entities
            .iter()
            .find(|e| e.name == "transport-belt")
            .unwrap();
        assert_eq!(belt.position, pos(0.5, 0.5));
        assert_eq!(belt.direction, Direction::East);

        let asm = bp
            .entities
            .iter()
            .find(|e| e.name == "assembling-machine-2")
            .unwrap();
        assert_eq!(asm.recipe.as_deref(), Some("iron-gear-wheel"));

        // entity_numbers are unique.
        assert_ne!(bp.entities[0].entity_number, bp.entities[1].entity_number);
    }

    #[test]
    fn test_to_blueprint_empty_grid() {
        let grid = Grid::new();
        let bp = to_blueprint(&grid, None, 1);
        assert!(bp.entities.is_empty());
        assert!(bp.label.is_none());
    }
}
