// Placing, colliding, removing and rotating an entity, plus the
// center-to-top-left mapping every one of those rests on.
use super::*;

// ── Placement tests ─────────────────────────────────────────────

#[test]
fn test_place_1x1_entity() {
    let mut grid = Grid::new();
    let id = grid
        .place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();

    assert_eq!(grid.entity_count(), 1);
    assert_eq!(grid.cell_count(), 1);

    let entity = grid.get_entity(id).unwrap();
    assert_eq!(entity.prototype_name, "transport-belt");
    assert_eq!(entity.top_left, GridPos { x: 0, y: 0 });
    assert_eq!(entity.size, (1, 1));

    // Cell (0,0) should be occupied
    assert!(grid.get_at(0, 0).is_some());
    assert!(grid.get_at(1, 0).is_none());
}

#[test]
fn test_place_3x3_entity() {
    let mut grid = Grid::new();
    let id = grid
        .place(
            "assembling-machine-2",
            &pos(0.5, 0.5),
            Direction::North,
            Some("iron-gear-wheel".to_string()),
            None,
        )
        .unwrap();

    assert_eq!(grid.entity_count(), 1);
    assert_eq!(grid.cell_count(), 9);

    let entity = grid.get_entity(id).unwrap();
    assert_eq!(entity.top_left, GridPos { x: -1, y: -1 });
    assert_eq!(entity.size, (3, 3));

    // All 9 cells should be occupied
    for dy in -1..=1 {
        for dx in -1..=1 {
            let found = grid.get_at(dx, dy);
            assert!(found.is_some(), "expected cell ({dx}, {dy}) to be occupied");
            assert_eq!(found.unwrap().id, id);
        }
    }

    // Outside should be empty
    assert!(grid.get_at(-2, -1).is_none());
    assert!(grid.get_at(2, 0).is_none());
}

#[test]
fn test_place_2x2_entity() {
    let mut grid = Grid::new();
    let id = grid
        .place("stone-furnace", &pos(1.0, 1.0), Direction::North, None, None)
        .unwrap();

    assert_eq!(grid.cell_count(), 4);

    let entity = grid.get_entity(id).unwrap();
    assert_eq!(entity.top_left, GridPos { x: 0, y: 0 });
    assert_eq!(entity.size, (2, 2));

    // All 4 cells occupied
    assert!(grid.get_at(0, 0).is_some());
    assert!(grid.get_at(1, 0).is_some());
    assert!(grid.get_at(0, 1).is_some());
    assert!(grid.get_at(1, 1).is_some());

    // Outside
    assert!(grid.get_at(-1, 0).is_none());
    assert!(grid.get_at(2, 0).is_none());
}

// ── Collision tests ─────────────────────────────────────────────

#[test]
fn test_can_place_collision() {
    let mut grid = Grid::new();
    grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();

    // Same cell — should report collision (Ok(false))
    let result = grid.can_place("transport-belt", &pos(0.5, 0.5), Direction::North);
    assert!(!result.unwrap());

    // Adjacent cell — should be fine
    let result = grid.can_place("transport-belt", &pos(1.5, 0.5), Direction::North);
    assert!(result.unwrap());
}

#[test]
fn test_place_collision_error() {
    let mut grid = Grid::new();
    let id = grid
        .place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();

    let err = grid
        .place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap_err();

    match err {
        GridError::Collision { x, y, occupant } => {
            assert_eq!(x, 0);
            assert_eq!(y, 0);
            assert_eq!(occupant, id);
        }
        other => panic!("expected Collision, got: {other:?}"),
    }
}

// ── Removal tests ───────────────────────────────────────────────

#[test]
fn test_remove_frees_cells() {
    let mut grid = Grid::new();
    let id = grid
        .place("stone-furnace", &pos(1.0, 1.0), Direction::North, None, None)
        .unwrap();

    assert_eq!(grid.cell_count(), 4);
    assert_eq!(grid.entity_count(), 1);

    let removed = grid.remove(id).unwrap();
    assert_eq!(removed.id, id);
    assert_eq!(removed.prototype_name, "stone-furnace");

    assert_eq!(grid.cell_count(), 0);
    assert_eq!(grid.entity_count(), 0);

    // Can place in the now-free area
    let can = grid.can_place("stone-furnace", &pos(1.0, 1.0), Direction::North);
    assert!(can.unwrap());
}

// ── Rotation tests ──────────────────────────────────────────────

#[test]
fn test_splitter_north_vs_east() {
    // Splitter is 2x1. North → (2, 1), East → (1, 2)
    let mut grid = Grid::new();

    // North: 2 wide, 1 tall at center (0.0, 0.5)
    let id_n = grid
        .place("splitter", &pos(0.0, 0.5), Direction::North, None, None)
        .unwrap();
    let e_n = grid.get_entity(id_n).unwrap();
    assert_eq!(e_n.size, (2, 1));
    assert_eq!(e_n.top_left, GridPos { x: -1, y: 0 });
    assert!(grid.get_at(-1, 0).is_some());
    assert!(grid.get_at(0, 0).is_some());
    // Height is 1, so y=1 should be empty
    assert!(grid.get_at(-1, 1).is_none());

    // East: 1 wide, 2 tall at center (5.5, 0.0)
    let id_e = grid
        .place("splitter", &pos(5.5, 0.0), Direction::East, None, None)
        .unwrap();
    let e_e = grid.get_entity(id_e).unwrap();
    assert_eq!(e_e.size, (1, 2));
    assert_eq!(e_e.top_left, GridPos { x: 5, y: -1 });
    assert!(grid.get_at(5, -1).is_some());
    assert!(grid.get_at(5, 0).is_some());
    // Width is 1, so x=6 should be empty
    assert!(grid.get_at(6, -1).is_none());
}

#[test]
fn test_combinator_rotation() {
    // Arithmetic combinator is 1x2. North → (1, 2), East → (2, 1)
    let mut grid = Grid::new();

    // North: 1 wide, 2 tall at center (0.5, 0.0)
    let id_n = grid
        .place(
            "arithmetic-combinator",
            &pos(0.5, 0.0),
            Direction::North,
            None,
            None,
        )
        .unwrap();
    let e_n = grid.get_entity(id_n).unwrap();
    assert_eq!(e_n.size, (1, 2));
    assert_eq!(e_n.top_left, GridPos { x: 0, y: -1 });
    assert!(grid.get_at(0, -1).is_some());
    assert!(grid.get_at(0, 0).is_some());
    assert!(grid.get_at(1, -1).is_none()); // only 1 wide

    // East: 2 wide, 1 tall at center (5.0, 5.5)
    let id_e = grid
        .place(
            "arithmetic-combinator",
            &pos(5.0, 5.5),
            Direction::East,
            None,
            None,
        )
        .unwrap();
    let e_e = grid.get_entity(id_e).unwrap();
    assert_eq!(e_e.size, (2, 1));
    assert_eq!(e_e.top_left, GridPos { x: 4, y: 5 });
    assert!(grid.get_at(4, 5).is_some());
    assert!(grid.get_at(5, 5).is_some());
    assert!(grid.get_at(4, 6).is_none()); // only 1 tall
}

// ── Position mapping parity tests ───────────────────────────────

#[test]
fn test_center_to_topleft_all_parities() {
    // 1x1 at (0.5, 0.5) → top_left (0, 0)
    assert_eq!(center_to_topleft(&pos(0.5, 0.5), 1, 1), (0, 0));

    // 3x3 at (0.5, 0.5) → top_left (-1, -1)
    assert_eq!(center_to_topleft(&pos(0.5, 0.5), 3, 3), (-1, -1));

    // 2x2 at (1.0, 1.0) → top_left (0, 0)
    assert_eq!(center_to_topleft(&pos(1.0, 1.0), 2, 2), (0, 0));

    // 2x1 splitter at (0.0, 0.5) → top_left (-1, 0)
    assert_eq!(center_to_topleft(&pos(0.0, 0.5), 2, 1), (-1, 0));

    // 1x2 (rotated combinator) at (0.5, 0.0) → top_left (0, -1)
    assert_eq!(center_to_topleft(&pos(0.5, 0.0), 1, 2), (0, -1));

    // 5x5 at (0.5, 0.5) → top_left (-2, -2)
    assert_eq!(center_to_topleft(&pos(0.5, 0.5), 5, 5), (-2, -2));
}

// ── Error case tests ────────────────────────────────────────────

#[test]
fn test_unknown_prototype() {
    let grid = Grid::new();
    let result = grid.can_place("modded-thing", &pos(0.5, 0.5), Direction::North);
    assert!(matches!(result, Err(GridError::UnknownPrototype(_))));

    let mut grid = Grid::new();
    let result = grid.place("modded-thing", &pos(0.5, 0.5), Direction::North, None, None);
    assert!(matches!(result, Err(GridError::UnknownPrototype(_))));
}

#[test]
fn test_remove_nonexistent_entity() {
    let mut grid = Grid::new();
    let result = grid.remove(EntityId(0));
    assert!(matches!(result, Err(GridError::EntityNotFound(_))));
}

#[test]
fn test_remove_already_removed() {
    let mut grid = Grid::new();
    let id = grid
        .place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    grid.remove(id).unwrap();

    // Double remove
    let result = grid.remove(id);
    assert!(matches!(result, Err(GridError::EntityNotFound(_))));
}
