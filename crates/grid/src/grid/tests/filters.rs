// `set_filters` — the one mutation allowed on an already-placed entity.
use super::*;

#[test]
fn test_set_filters_replaces_and_defaults_empty() {
    let mut grid = Grid::new();
    let id = grid
        .place("inserter", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    assert!(grid.get_entity(id).unwrap().filters.is_empty(), "unfiltered by default");

    grid.set_filters(id, vec!["iron-plate".to_string()]).unwrap();
    assert_eq!(grid.get_entity(id).unwrap().filters, vec!["iron-plate".to_string()]);

    grid.set_filters(id, vec!["copper-plate".to_string()]).unwrap();
    assert_eq!(
        grid.get_entity(id).unwrap().filters,
        vec!["copper-plate".to_string()],
        "replaces rather than appends"
    );
}

#[test]
fn test_set_filters_on_a_missing_entity() {
    let mut grid = Grid::new();
    assert!(matches!(
        grid.set_filters(EntityId(0), vec!["iron-plate".to_string()]),
        Err(GridError::EntityNotFound(_))
    ));

    let id = grid
        .place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    grid.remove(id).unwrap();
    assert!(
        matches!(grid.set_filters(id, vec![]), Err(GridError::EntityNotFound(_))),
        "a tombstoned slot is not a live entity"
    );
}

/// A filter is not geometry, which is the whole reason `set_filters` may
/// mutate an already-placed entity: the cell map, the spatial index and
/// the bbox cache all key on the footprint, and none of them move.
#[test]
fn test_set_filters_leaves_lookups_intact() {
    let mut grid = Grid::new();
    let id = grid
        .place("inserter", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    let before = grid.bounding_box();

    grid.set_filters(id, vec!["iron-plate".to_string()]).unwrap();

    assert_eq!(grid.get_at(0, 0).map(|e| e.id), Some(id));
    assert_eq!(grid.query_rect(0, 0, 0, 0).len(), 1);
    assert_eq!(grid.bounding_box(), before);
    assert_eq!(grid.entity_count(), 1);
}
