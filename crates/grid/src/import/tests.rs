// Tests for blueprint import: known/unknown entities, filters, recipe/type, legacy directions.
use super::*;
use factorio_blueprint::{Blueprint, Direction, Entity, Position};

/// Helper to build a minimal Blueprint with the given entities.
fn make_blueprint(entities: Vec<Entity>) -> Blueprint {
    Blueprint {
        item: "blueprint".to_string(),
        entities,
        version: 281479275675648,
        ..Default::default()
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
        ..Default::default()
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
    // Use 2.0 version so East (value 4) is not upgraded as 1.x South.
    let bp = Blueprint {
        item: "blueprint".to_string(),
        entities: vec![make_entity(1, "transport-belt", 0.5, 0.5, Direction::East)],
        version: 2u64 << 48,
        ..Default::default()
    };
    let result = from_blueprint(&bp);
    assert_eq!(result.grid.entity_count(), 1);
    assert!(result.skipped.is_empty());

    let entity = result.grid.entities().next().unwrap();
    assert_eq!(entity.prototype_name, "transport-belt");
    assert_eq!(entity.direction, Direction::East);
}

#[test]
fn test_import_reads_filter_names() {
    let mut inserter = make_entity(1, "inserter", 0.5, 0.5, Direction::North);
    inserter.use_filters = Some(true);
    inserter.filters = Some(vec![
        factorio_blueprint::ItemFilter {
            index: 1,
            name: Some("uranium-235".to_string()),
            quality: None,
            comparator: None,
        },
        // A quality-only slot carries no item name, so it contributes
        // nothing rather than an empty string.
        factorio_blueprint::ItemFilter {
            index: 2,
            name: None,
            quality: Some("legendary".to_string()),
            comparator: Some("=".to_string()),
        },
    ]);

    let result = from_blueprint(&make_blueprint(vec![inserter]));
    let placed = result.grid.entities().next().unwrap();
    assert_eq!(placed.filters, vec!["uranium-235".to_string()]);
}

#[test]
fn test_import_leaves_an_unfiltered_entity_unfiltered() {
    let bp = make_blueprint(vec![make_entity(1, "inserter", 0.5, 0.5, Direction::North)]);
    let result = from_blueprint(&bp);
    assert!(result.grid.entities().next().unwrap().filters.is_empty());
}

/// grid -> blueprint -> grid is stable for filters, so a block that is
/// exported and re-imported still separates its products.
#[test]
fn test_filters_survive_a_grid_blueprint_grid_round_trip() {
    let mut grid = Grid::new();
    let id = grid
        .place(
            "inserter",
            &Position { x: 0.5, y: 0.5 },
            Direction::North,
            None,
            None,
        )
        .unwrap();
    grid.set_filters(id, vec!["jelly".to_string(), "jellynut-seed".to_string()])
        .unwrap();

    let bp = crate::export::to_blueprint(&grid, None, 2u64 << 48);
    let back = from_blueprint(&bp);
    assert_eq!(
        back.grid.entities().next().unwrap().filters,
        vec!["jelly".to_string(), "jellynut-seed".to_string()]
    );
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

/// 1.x East is raw byte 2 → decoded as NorthEast. After import upgrade it
/// must be East with the combinator's E/W footprint (2, 1), not N/S (1, 2).
#[test]
fn test_import_legacy_east_combinator_has_swapped_footprint() {
    // NorthEast alone is a definitive 1.x East marker (raw 2).
    let bp = make_blueprint(vec![make_entity(
        1,
        "arithmetic-combinator",
        0.0,
        0.5,
        Direction::NorthEast,
    )]);
    let result = from_blueprint(&bp);
    assert!(result.skipped.is_empty(), "skipped: {:?}", result.skipped);
    assert_eq!(result.grid.entity_count(), 1);

    let entity = result.grid.entities().next().unwrap();
    assert_eq!(entity.direction, Direction::East);
    assert_eq!(
        entity.size,
        (2, 1),
        "1.x East combinator must place as 2×1, not 1×2"
    );
}

/// Pure 2.0 East (value 4) with no 1.x markers must not be rewritten to South.
/// Covered further in `tests/blueprint_import.rs` for pure-South / N+S cases.
#[test]
fn test_import_modern_east_direction_not_rewritten() {
    let bp = Blueprint {
        item: "blueprint".to_string(),
        entities: vec![make_entity(
            1,
            "arithmetic-combinator",
            0.0,
            0.5,
            Direction::East,
        )],
        version: 2u64 << 48,
        ..Default::default()
    };
    let result = from_blueprint(&bp);
    let entity = result.grid.entities().next().unwrap();
    assert_eq!(entity.direction, Direction::East);
    assert_eq!(entity.size, (2, 1));
}

