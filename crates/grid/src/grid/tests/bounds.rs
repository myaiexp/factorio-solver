// The optional constraint rectangle: what `with_bounds` rejects, and which
// coordinate the refusal names.
use super::*;

#[test]
fn test_with_bounds_rejects_out_of_bounds() {
    let mut grid = Grid::with_bounds(0, 0, 9, 9);

    // Place inside bounds — should succeed
    let result = grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None);
    assert!(result.is_ok());

    // can_place inside bounds
    let result = grid.can_place("transport-belt", &pos(5.5, 5.5), Direction::North);
    assert!(result.unwrap());

    // Place 3x3 that extends outside bounds (center at 0.5,0.5 → cells -1..1)
    let result = grid.place(
        "assembling-machine-1",
        &pos(0.5, 0.5),
        Direction::North,
        None,
        None,
    );
    match result {
        Err(GridError::OutOfBounds { x, y, .. }) => {
            assert!(x < 0 || y < 0, "expected negative coords, got ({x}, {y})");
        }
        other => panic!("expected OutOfBounds, got: {other:?}"),
    }

    // can_place returns OutOfBounds error too
    let result = grid.can_place(
        "assembling-machine-1",
        &pos(0.5, 0.5),
        Direction::North,
    );
    assert!(matches!(result, Err(GridError::OutOfBounds { .. })));

    // Place at edge — 3x3 at (5.5, 5.5) → cells 4..6 — within 0..9
    let result = grid.place(
        "assembling-machine-1",
        &pos(5.5, 5.5),
        Direction::North,
        None,
        None,
    );
    assert!(result.is_ok());
}

/// The constraint check is an AABB containment test, so the reported
/// `OutOfBounds` coordinate is the footprint edge that actually pokes out —
/// not an arbitrary interior cell hit by a scan.
#[test]
fn test_out_of_bounds_reports_violating_edge() {
    let mut grid = Grid::with_bounds(0, 0, 9, 9);

    // Overhang on both max edges: 3×3 at (9.5, 9.5) → cells 8..=10.
    let err = grid
        .place(
            "assembling-machine-1",
            &pos(9.5, 9.5),
            Direction::North,
            None,
            None,
        )
        .unwrap_err();
    match err {
        GridError::OutOfBounds { x, y, max_x, max_y } => {
            assert_eq!((x, y), (10, 10), "should report the max-edge corner");
            assert_eq!((max_x, max_y), (9, 9));
        }
        other => panic!("expected OutOfBounds, got: {other:?}"),
    }

    // Overhang on both min edges: 3×3 at (0.5, 0.5) → cells -1..=1.
    let err = grid
        .place(
            "assembling-machine-1",
            &pos(0.5, 0.5),
            Direction::North,
            None,
            None,
        )
        .unwrap_err();
    match err {
        GridError::OutOfBounds { x, y, .. } => {
            assert_eq!((x, y), (-1, -1), "should report the min-edge corner");
        }
        other => panic!("expected OutOfBounds, got: {other:?}"),
    }

    // Out on y only: the in-bounds x axis still reports a real footprint cell.
    let err = grid
        .can_place("transport-belt", &pos(5.5, -0.5), Direction::North)
        .unwrap_err();
    match err {
        GridError::OutOfBounds { x, y, .. } => {
            assert_eq!((x, y), (5, -1));
        }
        other => panic!("expected OutOfBounds, got: {other:?}"),
    }

    // Footprint exactly filling the constraint corner is accepted — the
    // containment test is inclusive on both edges.
    assert!(grid
        .place(
            "assembling-machine-1",
            &pos(8.5, 8.5),
            Direction::North,
            None,
            None
        )
        .is_ok());
}
