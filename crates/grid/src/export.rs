// Export a populated Grid back into a Factorio Blueprint.

use std::collections::HashMap;

use factorio_blueprint::{Blueprint, Entity, ItemFilter};

use crate::grid::Grid;

/// A placed entity's filter names as Factorio's `(use_filters, filters)` pair.
///
/// `use_filters` is emitted alongside — and is **not** optional decoration:
/// the game defaults it to `false`, so a `filters` array on its own is stored
/// and ignored, giving an inserter that reads as filtered in the blueprint and
/// grabs anything in game. `index` is 1-based. `quality` and `comparator` are
/// deliberately left off: absent means the filter matches an item of *any*
/// quality, where pinning `"normal"` would have the inserter refuse a
/// legendary output the moment quality modules are in the machine.
///
/// The single place the wire format is written, so a correction is one edit.
fn entity_filters(names: &[String]) -> (Option<bool>, Option<Vec<ItemFilter>>) {
    if names.is_empty() {
        return (None, None);
    }
    let filters = names
        .iter()
        .enumerate()
        .map(|(i, name)| ItemFilter {
            index: i as u32 + 1,
            name: Some(name.clone()),
            quality: None,
            comparator: None,
        })
        .collect();
    (Some(true), Some(filters))
}

/// Build a `Blueprint` from every live entity on `grid`.
///
/// Each entity's stored center position, direction, recipe, and type are copied
/// straight back, so a `from_blueprint` → `to_blueprint` round-trip preserves
/// placement. `entity_number` is derived from the grid's 1-based entity id order;
/// ids are stable and never reused, so numbers are unique within the blueprint.
pub fn to_blueprint(grid: &Grid, label: Option<String>, version: u64) -> Blueprint {
    let entities = grid
        .entities()
        .map(|e| {
            let (use_filters, filters) = entity_filters(&e.filters);
            Entity {
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
                use_filters,
                filters,
                extra: HashMap::new(),
            }
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

    /// The contract with Factorio, pinned key for key. If this test is ever
    /// "fixed" to match new code rather than the game, filtered inserters
    /// silently start grabbing both products again.
    #[test]
    fn test_a_filtered_inserter_emits_use_filters_and_a_one_based_index() {
        let mut grid = Grid::new();
        let id = grid
            .place("inserter", &pos(0.5, 0.5), Direction::North, None, None)
            .unwrap();
        grid.set_filters(id, vec!["uranium-238".to_string()]).unwrap();

        let e = &to_blueprint(&grid, None, 1).entities[0];
        assert_eq!(
            e.use_filters,
            Some(true),
            "the game defaults use_filters to false, so filters alone are inert"
        );
        let json = serde_json::to_value(e).unwrap();
        assert_eq!(
            json["filters"],
            serde_json::json!([{"index": 1, "name": "uranium-238"}]),
            "index is 1-based; quality/comparator are omitted so any quality passes"
        );
    }

    #[test]
    fn test_several_filters_are_indexed_in_slot_order() {
        let mut grid = Grid::new();
        let id = grid
            .place("inserter", &pos(0.5, 0.5), Direction::North, None, None)
            .unwrap();
        grid.set_filters(id, vec!["jelly".to_string(), "jellynut-seed".to_string()])
            .unwrap();

        let filters = to_blueprint(&grid, None, 1).entities[0].filters.clone().unwrap();
        let pairs: Vec<(u32, Option<String>)> =
            filters.into_iter().map(|f| (f.index, f.name)).collect();
        assert_eq!(
            pairs,
            vec![
                (1, Some("jelly".to_string())),
                (2, Some("jellynut-seed".to_string()))
            ]
        );
    }

    /// Everything the generator emitted before filters existed must serialize
    /// byte-identically — an absent key, never `null` or an empty array.
    #[test]
    fn test_an_unfiltered_entity_emits_neither_filter_key() {
        let mut grid = Grid::new();
        grid.place("transport-belt", &pos(0.5, 0.5), Direction::South, None, None)
            .unwrap();

        let e = &to_blueprint(&grid, None, 1).entities[0];
        assert_eq!(e.use_filters, None);
        assert_eq!(e.filters, None);
        let json = serde_json::to_value(e).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("use_filters") && !obj.contains_key("filters"));
    }
}
