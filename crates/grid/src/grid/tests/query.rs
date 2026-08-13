// Reading the grid back: cell and id lookup, the bounding box, rectangle
// and neighbour queries, the live counters, and the tombstone-skipping
// iterator.
use super::*;

// ── Query tests ─────────────────────────────────────────────────

#[test]
fn test_get_at_occupied() {
    let mut grid = Grid::new();
    let id = grid
        .place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();

    let entity = grid.get_at(0, 0).unwrap();
    assert_eq!(entity.id, id);
    assert_eq!(entity.prototype_name, "transport-belt");
}

#[test]
fn test_get_at_empty() {
    let grid = Grid::new();
    assert!(grid.get_at(0, 0).is_none());
    assert!(grid.get_at(100, -50).is_none());
}

#[test]
fn test_get_entity_by_id() {
    let mut grid = Grid::new();
    let id0 = grid
        .place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    let id1 = grid
        .place("inserter", &pos(1.5, 0.5), Direction::North, None, None)
        .unwrap();

    let e0 = grid.get_entity(id0).unwrap();
    assert_eq!(e0.prototype_name, "transport-belt");

    let e1 = grid.get_entity(id1).unwrap();
    assert_eq!(e1.prototype_name, "inserter");

    // Non-existent ID
    assert!(grid.get_entity(EntityId(999)).is_none());
}

// ── Bounding box tests ──────────────────────────────────────────

#[test]
fn test_bounding_box_empty() {
    let grid = Grid::new();
    assert!(grid.bounding_box().is_none());
}

#[test]
fn test_bounding_box_single() {
    let mut grid = Grid::new();
    grid.place(
        "assembling-machine-1",
        &pos(0.5, 0.5),
        Direction::North,
        None,
        None,
    )
    .unwrap();

    let (tl, br) = grid.bounding_box().unwrap();
    assert_eq!(tl, GridPos { x: -1, y: -1 });
    assert_eq!(br, GridPos { x: 1, y: 1 });
}

#[test]
fn test_bounding_box_multiple() {
    let mut grid = Grid::new();
    // Belt at (0.5, 0.5) → cell (0, 0)
    grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    // Belt at (10.5, 5.5) → cell (10, 5)
    grid.place("transport-belt", &pos(10.5, 5.5), Direction::North, None, None)
        .unwrap();

    let (tl, br) = grid.bounding_box().unwrap();
    assert_eq!(tl, GridPos { x: 0, y: 0 });
    assert_eq!(br, GridPos { x: 10, y: 5 });
}

// ── query_rect tests ────────────────────────────────────────────

#[test]
fn test_query_rect_inverted_matches_ordered() {
    let mut grid = Grid::new();
    let id_in = grid
        .place("transport-belt", &pos(2.5, 2.5), Direction::North, None, None)
        .unwrap();
    let _id_out = grid
        .place("transport-belt", &pos(20.5, 20.5), Direction::North, None, None)
        .unwrap();

    let mut ordered: Vec<EntityId> = grid
        .query_rect(0, 0, 5, 5)
        .iter()
        .map(|e| e.id)
        .collect();
    ordered.sort_by_key(|id| id.0);
    let mut inverted: Vec<EntityId> = grid
        .query_rect(5, 5, 0, 0)
        .iter()
        .map(|e| e.id)
        .collect();
    inverted.sort_by_key(|id| id.0);

    assert_eq!(ordered, inverted);
    assert_eq!(ordered, vec![id_in]);
}

// ── Neighbor tests ──────────────────────────────────────────────

#[test]
fn test_get_neighbors() {
    let mut grid = Grid::new();
    // Place belt at (0,0)
    let id0 = grid
        .place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    // Place belt at (2,0) — 2 cells away
    let id1 = grid
        .place("transport-belt", &pos(2.5, 0.5), Direction::North, None, None)
        .unwrap();
    // Place belt at (5,5) — far away
    let _id2 = grid
        .place("transport-belt", &pos(5.5, 5.5), Direction::North, None, None)
        .unwrap();

    // Radius 2 around (0,0) should find id0 and id1
    let neighbors = grid.get_neighbors(GridPos { x: 0, y: 0 }, 2);
    let neighbor_ids: Vec<EntityId> = neighbors.iter().map(|e| e.id).collect();
    assert!(neighbor_ids.contains(&id0));
    assert!(neighbor_ids.contains(&id1));
    assert_eq!(neighbors.len(), 2);

    // Radius 0 around (0,0) should find only id0
    let neighbors = grid.get_neighbors(GridPos { x: 0, y: 0 }, 0);
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].id, id0);
}

// ── Count tracking tests ────────────────────────────────────────

#[test]
fn test_entity_count_and_cell_count() {
    let mut grid = Grid::new();
    assert_eq!(grid.entity_count(), 0);
    assert_eq!(grid.cell_count(), 0);

    // Place a 1x1
    let id0 = grid
        .place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    assert_eq!(grid.entity_count(), 1);
    assert_eq!(grid.cell_count(), 1);

    // Place a 3x3
    let id1 = grid
        .place(
            "assembling-machine-1",
            &pos(5.5, 5.5),
            Direction::North,
            None,
            None,
        )
        .unwrap();
    assert_eq!(grid.entity_count(), 2);
    assert_eq!(grid.cell_count(), 10); // 1 + 9

    // Remove the 1x1
    grid.remove(id0).unwrap();
    assert_eq!(grid.entity_count(), 1);
    assert_eq!(grid.cell_count(), 9);

    // Remove the 3x3
    grid.remove(id1).unwrap();
    assert_eq!(grid.entity_count(), 0);
    assert_eq!(grid.cell_count(), 0);
}

// ── Iterator test ───────────────────────────────────────────────

#[test]
fn test_entities_iterator() {
    let mut grid = Grid::new();
    grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    let id1 = grid
        .place("inserter", &pos(1.5, 0.5), Direction::North, None, None)
        .unwrap();
    grid.place("pipe", &pos(2.5, 0.5), Direction::North, None, None)
        .unwrap();

    // Remove middle entity
    grid.remove(id1).unwrap();

    // Iterator should skip the tombstone
    let names: Vec<&str> = grid.entities().map(|e| e.prototype_name).collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"transport-belt"));
    assert!(names.contains(&"pipe"));
}
