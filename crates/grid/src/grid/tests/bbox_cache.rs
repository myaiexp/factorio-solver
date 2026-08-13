// The incremental bounding-box cache (subtask 2-5). `bounding_box` is a
// cached field rather than a scan, so what matters is invalidation: an
// interior removal must NOT recompute, an edge removal must.
use super::*;

/// (a) Cache is correct immediately after a single place call.
#[test]
fn test_bbox_cache_after_place() {
    let mut grid = Grid::new();
    // 1×1 belt at center (0.5, 0.5) → top-left (0, 0), max (0, 0)
    grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();

    let (tl, br) = grid.bounding_box().unwrap();
    assert_eq!(tl, GridPos { x: 0, y: 0 });
    assert_eq!(br, GridPos { x: 0, y: 0 });

    // Adding a second entity far away expands the cache correctly.
    grid.place("transport-belt", &pos(8.5, 5.5), Direction::North, None, None)
        .unwrap();

    let (tl, br) = grid.bounding_box().unwrap();
    assert_eq!(tl, GridPos { x: 0, y: 0 });
    assert_eq!(br, GridPos { x: 8, y: 5 });
}

/// (b) Cache remains valid after removing an entity that is interior
/// to the bounding box (no recompute needed).
#[test]
fn test_bbox_cache_unaffected_by_interior_removal() {
    let mut grid = Grid::new();
    // Corner entities define the bbox.
    grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    grid.place("transport-belt", &pos(10.5, 6.5), Direction::North, None, None)
        .unwrap();
    // Interior entity — entirely within (0,0)..(10,6).
    let interior_id = grid
        .place("transport-belt", &pos(4.5, 3.5), Direction::North, None, None)
        .unwrap();

    // Confirm full bbox before removal.
    let (tl, br) = grid.bounding_box().unwrap();
    assert_eq!(tl, GridPos { x: 0, y: 0 });
    assert_eq!(br, GridPos { x: 10, y: 6 });

    // Remove the interior entity — bbox must not change.
    grid.remove(interior_id).unwrap();

    let (tl, br) = grid.bounding_box().unwrap();
    assert_eq!(tl, GridPos { x: 0, y: 0 });
    assert_eq!(br, GridPos { x: 10, y: 6 });
}

/// (c) Cache is recomputed correctly when an edge entity is removed.
#[test]
fn test_bbox_cache_recomputed_after_edge_removal() {
    let mut grid = Grid::new();
    // Three entities: two establish the extreme edges, one is inward.
    //   (0,0) — top-left corner anchor
    //   (3,2) — interior
    //   (10,5) — bottom-right corner anchor
    grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    grid.place("transport-belt", &pos(3.5, 2.5), Direction::North, None, None)
        .unwrap();
    let edge_id = grid
        .place("transport-belt", &pos(10.5, 5.5), Direction::North, None, None)
        .unwrap();

    // Before removal: bbox is (0,0)..(10,5).
    let (tl, br) = grid.bounding_box().unwrap();
    assert_eq!(tl, GridPos { x: 0, y: 0 });
    assert_eq!(br, GridPos { x: 10, y: 5 });

    // Remove the entity that sits on the max_x / max_y edge.
    grid.remove(edge_id).unwrap();

    // bbox must recompute to the tightest box around the two remaining entities:
    // (0,0) and (3,2) → tl (0,0), br (3,2).
    let (tl, br) = grid.bounding_box().unwrap();
    assert_eq!(tl, GridPos { x: 0, y: 0 });
    assert_eq!(br, GridPos { x: 3, y: 2 });
}

/// (d) Cache becomes None once all entities are removed.
#[test]
fn test_bbox_cache_none_after_all_removed() {
    let mut grid = Grid::new();
    let id0 = grid
        .place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
        .unwrap();
    let id1 = grid
        .place("transport-belt", &pos(5.5, 5.5), Direction::North, None, None)
        .unwrap();

    assert!(grid.bounding_box().is_some());

    grid.remove(id0).unwrap();
    assert!(grid.bounding_box().is_some(), "bbox should still exist after partial removal");

    grid.remove(id1).unwrap();
    assert!(
        grid.bounding_box().is_none(),
        "bbox should be None once all entities are removed"
    );
}
