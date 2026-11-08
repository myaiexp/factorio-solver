// Read-only queries on a Grid: cell and id lookups, range queries, counts, bounds, paths.
use super::Grid;
use crate::types::{CellState, EntityId, GridPos, PlacedEntity};

impl Grid {
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
    ///
    /// Inverted rectangles (`max < min` on either axis) are normalized the same way
    /// as `SpatialIndex::query_rect`, so callers need not pre-sort the corners.
    pub fn query_rect(&self, min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Vec<&PlacedEntity> {
        // Match SpatialIndex: tolerate inverted corners rather than filtering everything out.
        let (min_x, max_x) = (min_x.min(max_x), min_x.max(max_x));
        let (min_y, max_y) = (min_y.min(max_y), min_y.max(max_y));

        self.spatial
            .query_rect(min_x, min_y, max_x, max_y)
            .into_iter()
            .filter_map(|id| {
                // Resolve ID → live entity reference; skip tombstones.
                let entity = self.entities.get(id.0)?.as_ref()?;
                // Exact footprint intersection (spatial index is chunk-coarse).
                let (tl_x, tl_y, br_x, br_y) = entity.aabb();
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
