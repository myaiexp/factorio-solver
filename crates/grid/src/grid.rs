use std::collections::HashMap;

use factorio_blueprint::{Direction, Position};

use crate::error::GridError;
use crate::prototype::{effective_size, lookup};
use crate::spatial::SpatialIndex;
use crate::types::{CellState, EntityId, GridPos, PlacedEntity};

// ── Grid ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Grid {
    cells: HashMap<(i32, i32), CellState>,
    entities: Vec<Option<PlacedEntity>>,
    /// Optional constraint rectangle: placement outside this area is rejected.
    constraint: Option<(i32, i32, i32, i32)>, // (min_x, min_y, max_x, max_y)
    /// Incremental bounding box of all currently-placed entity footprints.
    /// `None` when the grid is empty. Updated by `place` and `remove`.
    bbox: Option<(i32, i32, i32, i32)>, // (min_x, min_y, max_x, max_y)
    live_count: usize,
    spatial: SpatialIndex,
}

impl Grid {
    pub fn new() -> Self {
        Self {
            cells: HashMap::new(),
            entities: Vec::new(),
            constraint: None,
            bbox: None,
            live_count: 0,
            spatial: SpatialIndex::new(),
        }
    }

    /// Construct a grid that rejects placements outside the given rectangle.
    ///
    /// The constraint rectangle is not the same as the bounding box of placed
    /// entities — it is a hard limit enforced during `place` / `can_place`.
    pub fn with_bounds(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Self {
        Self {
            cells: HashMap::new(),
            entities: Vec::new(),
            constraint: Some((min_x, min_y, max_x, max_y)),
            bbox: None,
            live_count: 0,
            spatial: SpatialIndex::new(),
        }
    }

    // ── Core placement ──────────────────────────────────────────────

    /// Shared validation: resolve prototype, compute footprint, check bounds.
    fn validate_placement(
        &self,
        prototype_name: &str,
        center: &Position,
        direction: Direction,
    ) -> Result<(&'static crate::prototype::EntityPrototype, i32, i32, u32, u32), GridError> {
        let proto = lookup(prototype_name)
            .ok_or_else(|| GridError::UnknownPrototype(prototype_name.to_string()))?;

        let (w, h) = effective_size(proto, direction);
        let (top_left_x, top_left_y) = center_to_topleft(center, w, h);

        // Constraint check — rejects entities outside the hard placement boundary.
        if let Some((min_x, min_y, max_x, max_y)) = self.constraint {
            for dy in 0..h as i32 {
                for dx in 0..w as i32 {
                    let cx = top_left_x + dx;
                    let cy = top_left_y + dy;
                    if cx < min_x || cx > max_x || cy < min_y || cy > max_y {
                        return Err(GridError::OutOfBounds {
                            x: cx,
                            y: cy,
                            max_x,
                            max_y,
                        });
                    }
                }
            }
        }

        Ok((proto, top_left_x, top_left_y, w, h))
    }

    /// Check whether an entity can be placed at the given center position.
    /// Returns `Ok(true)` if placement is valid, `Ok(false)` if a collision
    /// would occur, or `Err` for unknown prototypes or out-of-bounds.
    pub fn can_place(
        &self,
        prototype_name: &str,
        center: &Position,
        direction: Direction,
    ) -> Result<bool, GridError> {
        let (_proto, top_left_x, top_left_y, w, h) =
            self.validate_placement(prototype_name, center, direction)?;

        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                if self.cells.contains_key(&(top_left_x + dx, top_left_y + dy)) {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Place an entity on the grid. Returns the assigned `EntityId`.
    pub fn place(
        &mut self,
        prototype_name: &str,
        center: &Position,
        direction: Direction,
        recipe: Option<String>,
        entity_type: Option<String>,
    ) -> Result<EntityId, GridError> {
        let (proto, top_left_x, top_left_y, w, h) =
            self.validate_placement(prototype_name, center, direction)?;

        // Collision check
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                let cx = top_left_x + dx;
                let cy = top_left_y + dy;
                if let Some(CellState::Occupied { entity_id }) = self.cells.get(&(cx, cy)) {
                    return Err(GridError::Collision {
                        x: cx,
                        y: cy,
                        occupant: *entity_id,
                    });
                }
            }
        }

        // Allocate entity
        let id = EntityId(self.entities.len());
        let entity = PlacedEntity {
            id,
            prototype_name: proto.name,
            position: GridPos {
                x: top_left_x,
                y: top_left_y,
            },
            center: Position {
                x: center.x,
                y: center.y,
            },
            direction,
            size: (w, h),
            recipe,
            entity_type,
        };
        self.entities.push(Some(entity));
        self.live_count += 1;

        // Occupy cells
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                let cx = top_left_x + dx;
                let cy = top_left_y + dy;
                self.cells.insert((cx, cy), CellState::Occupied { entity_id: id });
            }
        }

        // Register in spatial index for fast range queries
        self.spatial.insert(id, (top_left_x, top_left_y), (w, h));

        // Expand incremental bounding box cache to include this entity's footprint.
        // `get_or_insert` initialises the cache on the first placement, then both
        // paths (init and expand) share the same min/max clamp below.
        let entity_min_x = top_left_x;
        let entity_min_y = top_left_y;
        let entity_max_x = top_left_x + w as i32 - 1;
        let entity_max_y = top_left_y + h as i32 - 1;
        let bb = self
            .bbox
            .get_or_insert((entity_min_x, entity_min_y, entity_max_x, entity_max_y));
        bb.0 = bb.0.min(entity_min_x);
        bb.1 = bb.1.min(entity_min_y);
        bb.2 = bb.2.max(entity_max_x);
        bb.3 = bb.3.max(entity_max_y);

        Ok(id)
    }

    /// Remove an entity from the grid. Frees all cells it occupied.
    /// The entity slot becomes a tombstone (None) — IDs are never reused.
    pub fn remove(&mut self, id: EntityId) -> Result<PlacedEntity, GridError> {
        let entity = self
            .entities
            .get(id.0)
            .and_then(|slot| slot.as_ref())
            .ok_or(GridError::EntityNotFound(id))?
            .clone();

        // Remove from spatial index before freeing cells
        let (w, h) = entity.size;
        self.spatial
            .remove(id, (entity.position.x, entity.position.y), (w, h));

        // Free cells
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                let cx = entity.position.x + dx;
                let cy = entity.position.y + dy;
                self.cells.remove(&(cx, cy));
            }
        }

        // Tombstone the slot
        self.entities[id.0] = None;
        self.live_count -= 1;

        // Update bounding box cache.
        //
        // Three cases:
        //   1. Grid is now empty → clear the cache.
        //   2. Removed entity touched a bbox edge → recompute from remaining entities
        //      (O(entities), but only triggered when necessary).
        //   3. Entity was entirely interior to the bbox → cache is still valid, do nothing.
        if self.live_count == 0 {
            self.bbox = None;
        } else if let Some(bb) = self.bbox {
            let entity_min_x = entity.position.x;
            let entity_min_y = entity.position.y;
            let entity_max_x = entity.position.x + entity.size.0 as i32 - 1;
            let entity_max_y = entity.position.y + entity.size.1 as i32 - 1;

            // If any edge of the removed entity's footprint coincides with a bbox
            // boundary the bbox may have shrunk — recompute from the entity vec.
            // Entities that are wholly interior cannot affect the bbox, so we skip them.
            if entity_min_x == bb.0
                || entity_min_y == bb.1
                || entity_max_x == bb.2
                || entity_max_y == bb.3
            {
                self.bbox = self.entities.iter().filter_map(|slot| slot.as_ref()).fold(
                    None,
                    |acc, e| {
                        let e_min_x = e.position.x;
                        let e_min_y = e.position.y;
                        let e_max_x = e.position.x + e.size.0 as i32 - 1;
                        let e_max_y = e.position.y + e.size.1 as i32 - 1;
                        Some(match acc {
                            None => (e_min_x, e_min_y, e_max_x, e_max_y),
                            Some((min_x, min_y, max_x, max_y)) => (
                                min_x.min(e_min_x),
                                min_y.min(e_min_y),
                                max_x.max(e_max_x),
                                max_y.max(e_max_y),
                            ),
                        })
                    },
                );
            }
            // else: entity was interior — bbox is still valid
        }

        Ok(entity)
    }

    // ── Queries ─────────────────────────────────────────────────────

    /// Get the entity occupying a cell, if any.
    pub fn get_at(&self, x: i32, y: i32) -> Option<&PlacedEntity> {
        match self.cells.get(&(x, y)) {
            Some(CellState::Occupied { entity_id }) => {
                self.entities[entity_id.0].as_ref()
            }
            None => None,
        }
    }

    /// Get an entity by its ID.
    pub fn get_entity(&self, id: EntityId) -> Option<&PlacedEntity> {
        self.entities.get(id.0).and_then(|slot| slot.as_ref())
    }

    /// Return all live entities whose footprint overlaps the rectangle
    /// `[min_x, max_x] × [min_y, max_y]` (cell coordinates, inclusive).
    ///
    /// Uses the chunk-based `SpatialIndex` for fast candidate selection (O(chunks
    /// touched + candidates)), then applies an exact AABB check to exclude entities
    /// that are in a touched chunk but don't actually overlap the query rectangle.
    /// Tombstoned entity slots are silently skipped.
    pub fn query_rect(&self, min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Vec<&PlacedEntity> {
        self.spatial
            .query_rect(min_x, min_y, max_x, max_y)
            .into_iter()
            .filter_map(|id| {
                // Resolve ID → live entity reference; skip tombstones.
                let entity = self.entities.get(id.0)?.as_ref()?;
                // Exact footprint intersection (spatial index is chunk-coarse).
                let tl_x = entity.position.x;
                let tl_y = entity.position.y;
                let br_x = tl_x + entity.size.0 as i32 - 1;
                let br_y = tl_y + entity.size.1 as i32 - 1;
                if tl_x <= max_x && br_x >= min_x && tl_y <= max_y && br_y >= min_y {
                    Some(entity)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Iterate over all live (non-removed) entities.
    pub fn entities(&self) -> impl Iterator<Item = &PlacedEntity> {
        self.entities.iter().filter_map(|slot| slot.as_ref())
    }

    /// Number of live entities on the grid.
    pub fn entity_count(&self) -> usize {
        self.live_count
    }

    /// Number of occupied cells.
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Axis-aligned bounding box of all placed entity footprints.
    /// Returns `(top_left, bottom_right)` in cell coordinates, or `None` if empty.
    ///
    /// O(1) — reads from the incremental `bbox` cache maintained by `place` and
    /// `remove`. The cache is expanded on every `place` and recomputed (O(entities))
    /// only when a `remove` touches a bbox edge.
    pub fn bounding_box(&self) -> Option<(GridPos, GridPos)> {
        self.bbox.map(|(min_x, min_y, max_x, max_y)| {
            (GridPos { x: min_x, y: min_y }, GridPos { x: max_x, y: max_y })
        })
    }

    /// Find all entities whose footprint overlaps the square of `radius` cells
    /// around `center` (Chebyshev distance). Delegates to `query_rect` so no
    /// manual cell iteration or HashSet deduplication is needed.
    pub fn get_neighbors(&self, center: GridPos, radius: i32) -> Vec<&PlacedEntity> {
        self.query_rect(
            center.x - radius,
            center.y - radius,
            center.x + radius,
            center.y + radius,
        )
    }

    /// Find the shortest path from `from` to `to` using A* with default settings:
    /// 4-directional movement and no cost limit.
    ///
    /// Returns `Some(path)` where `path` is a `Vec<GridPos>` ordered from `from`
    /// (inclusive) to `to` (inclusive), or `None` if no path exists.
    ///
    /// Occupied cells are treated as non-walkable; the start and goal cells are
    /// always walkable regardless of occupancy (endpoints may lie inside entities).
    pub fn find_path(&self, from: GridPos, to: GridPos) -> Option<Vec<GridPos>> {
        crate::astar::find_path(self, from, to, &crate::astar::AStarConfig::default())
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self::new()
    }
}

// ── Position mapping ────────────────────────────────────────────────────

/// Convert a Factorio center position to top-left grid cell.
///
/// Formula: top_left = ((center_x - width/2.0).round(), (center_y - height/2.0).round())
fn center_to_topleft(center: &Position, width: u32, height: u32) -> (i32, i32) {
    let top_left_x = (center.x - width as f64 / 2.0).round() as i32;
    let top_left_y = (center.y - height as f64 / 2.0).round() as i32;
    (top_left_x, top_left_y)
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to make positions concise
    fn pos(x: f64, y: f64) -> Position {
        Position { x, y }
    }

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
        assert_eq!(entity.position, GridPos { x: 0, y: 0 });
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
        assert_eq!(entity.position, GridPos { x: -1, y: -1 });
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
        assert_eq!(entity.position, GridPos { x: 0, y: 0 });
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
        assert_eq!(result.unwrap(), false);

        // Adjacent cell — should be fine
        let result = grid.can_place("transport-belt", &pos(1.5, 0.5), Direction::North);
        assert_eq!(result.unwrap(), true);
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
        assert_eq!(can.unwrap(), true);
    }

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
        assert_eq!(e_n.position, GridPos { x: -1, y: 0 });
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
        assert_eq!(e_e.position, GridPos { x: 5, y: -1 });
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
        assert_eq!(e_n.position, GridPos { x: 0, y: -1 });
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
        assert_eq!(e_e.position, GridPos { x: 4, y: 5 });
        assert!(grid.get_at(4, 5).is_some());
        assert!(grid.get_at(5, 5).is_some());
        assert!(grid.get_at(4, 6).is_none()); // only 1 tall
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

    // ── Bounding-box cache tests (subtask 2-5) ──────────────────────

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

    // ── Bounds tests ────────────────────────────────────────────────

    #[test]
    fn test_with_bounds_rejects_out_of_bounds() {
        let mut grid = Grid::with_bounds(0, 0, 9, 9);

        // Place inside bounds — should succeed
        let result = grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None);
        assert!(result.is_ok());

        // can_place inside bounds
        let result = grid.can_place("transport-belt", &pos(5.5, 5.5), Direction::North);
        assert_eq!(result.unwrap(), true);

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

    // ── Memory footprint sanity test (subtask 6-2) ──────────────────

    /// Verify that a grid with 5,000 entities stays well under 500 MB.
    ///
    /// Per-entity memory breakdown (approximate):
    ///   - `Option<PlacedEntity>` in `entities` vec:
    ///       PlacedEntity = EntityId(8) + &'static str ptr(8) + GridPos(8) +
    ///                      Position(16) + Direction(1+pad≈4) + (u32,u32)(8) +
    ///                      Option<String>(24) + Option<String>(24) ≈ 104 bytes
    ///       With Option discriminant overhead: ~112 bytes
    ///   - `CellState` cells in `HashMap<(i32,i32), CellState>`:
    ///       Each 1×1 entity occupies 1 cell.
    ///       HashMap entry ≈ (i32,i32)(8) + CellState(8) + overhead ≈ 40 bytes
    ///   - SpatialIndex `HashMap<(i32,i32), Vec<EntityId>>` chunk entries:
    ///       5,000 belts in a ~70×72 area → ceil(70/16)×ceil(72/16) ≈ 20 chunks
    ///       Each chunk vec entry: 8 bytes per EntityId.
    ///       Total spatial index ≈ 5000 × 8 = 40 KB (negligible)
    ///
    /// Estimated total for 5,000 1×1 entities:
    ///   entities vec: 5,000 × 112   =  560 KB
    ///   cells map:    5,000 × 40    =  200 KB
    ///   spatial idx:  5,000 × 8     =   40 KB
    ///   ─────────────────────────────────────
    ///   Total:                      ≈  800 KB  (< 1 MB for 5,000 entities)
    ///
    /// For 100,000 entities (large megabase) the estimate scales to ~16 MB —
    /// more than two orders of magnitude below the 500 MB acceptance threshold.
    #[test]
    fn test_memory_footprint_5000_entities() {
        use std::mem::size_of;

        let mut grid = Grid::new();

        // Place 5,000 transport-belt entities in a ~70×72 region.
        // (71 * 71 = 5041 > 5000; we stop at 5000 to hit the target exactly.)
        let mut count = 0_usize;
        'outer: for y in 0..72_i32 {
            for x in 0..71_i32 {
                grid.place(
                    "transport-belt",
                    &pos(x as f64 + 0.5, y as f64 + 0.5),
                    Direction::North,
                    None,
                    None,
                )
                .unwrap();
                count += 1;
                if count == 5_000 {
                    break 'outer;
                }
            }
        }

        assert_eq!(grid.entity_count(), 5_000);

        // Estimate memory consumed by the core data structures.
        //
        // We can't call size_of_val on the Grid itself (it only measures the
        // stack portion, not heap allocations), so we compute an upper-bound
        // estimate from known element sizes × counts.

        // entities vec: each slot is Option<PlacedEntity>
        let entity_slot_bytes = size_of::<Option<PlacedEntity>>();
        let entities_heap = entity_slot_bytes * grid.entities.capacity();

        // cells hashmap: each entry holds a (i32,i32) key and CellState value.
        // HashMap has ~1.8× load overhead, so we multiply by 2 to be safe.
        let cell_entry_bytes = size_of::<(i32, i32)>() + size_of::<CellState>();
        let cells_heap = cell_entry_bytes * grid.cells.capacity();

        // Per-entity String heap for recipe/entity_type (None for belts → 0).
        // Include it for correctness even though it's zero here.
        let strings_heap: usize = grid
            .entities()
            .map(|e| {
                e.recipe.as_ref().map_or(0, |s| s.capacity())
                    + e.entity_type.as_ref().map_or(0, |s| s.capacity())
            })
            .sum();

        let total_bytes = entities_heap + cells_heap + strings_heap;
        let total_mb = total_bytes as f64 / (1024.0 * 1024.0);

        // 500 MB acceptance criterion.  In practice this runs at < 1 MB.
        const LIMIT_MB: f64 = 500.0;
        assert!(
            total_mb < LIMIT_MB,
            "estimated memory {total_mb:.2} MB exceeds the {LIMIT_MB} MB limit for 5,000 entities"
        );
    }

    // ── Performance test ─────────────────────────────────────────────

    /// Verify that `query_rect` on a large grid scales with result size, not
    /// total entity count.  We place 10,000 transport-belt entities (1×1) in a
    /// 100×100 grid, then query a 10×10 region and assert:
    ///   1. The returned entity count matches the expected 100 entities in
    ///      that region.
    ///   2. The query completes in under 5 ms — demonstrating O(result) rather
    ///      than O(all-entities) behaviour.
    #[test]
    fn test_query_rect_performance_10k_entities() {
        let mut grid = Grid::new();

        // Place 10,000 transport-belt entities at (x+0.5, y+0.5) for x,y in 0..100
        for y in 0..100_i32 {
            for x in 0..100_i32 {
                grid.place(
                    "transport-belt",
                    &pos(x as f64 + 0.5, y as f64 + 0.5),
                    Direction::North,
                    None,
                    None,
                )
                .unwrap();
            }
        }

        assert_eq!(grid.entity_count(), 10_000);

        // Query a 10×10 region (cells 0..=9 in both axes → 100 entities)
        let start = std::time::Instant::now();
        let results = grid.query_rect(0, 0, 9, 9);
        let elapsed = start.elapsed();

        assert_eq!(
            results.len(),
            100,
            "expected exactly 100 entities in the 10×10 query region"
        );

        assert!(
            elapsed.as_millis() < 5,
            "query_rect should complete in < 5 ms, took {} ms",
            elapsed.as_millis()
        );
    }
}
