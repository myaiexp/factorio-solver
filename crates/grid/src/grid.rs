// The Grid itself: construction, placement validation, placing and removing entities.
use std::collections::HashMap;

use factorio_blueprint::{Direction, Position};

use crate::error::GridError;
use crate::prototype::{effective_size, lookup};
use crate::spatial::SpatialIndex;
use crate::types::{footprint_aabb, footprint_cells, CellState, EntityId, GridPos, PlacedEntity};

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
        //
        // Both the footprint and the constraint are axis-aligned rectangles, so
        // "does the footprint fit?" is one inclusive-AABB containment test rather
        // than a per-cell scan. The reported (x, y) is the *violating edge* of the
        // footprint on each axis (the corner that pokes out), which is more useful
        // than the first interior cell a scan would have hit.
        if let Some((min_x, min_y, max_x, max_y)) = self.constraint {
            let (f_min_x, f_min_y, f_max_x, f_max_y) =
                footprint_aabb((top_left_x, top_left_y), (w, h));

            if f_min_x < min_x || f_min_y < min_y || f_max_x > max_x || f_max_y > max_y {
                // Per axis: report whichever edge is out of bounds, falling back
                // to the (in-bounds) low edge — always a real footprint cell.
                let violating = |lo: i32, hi: i32, min: i32, max: i32| {
                    if lo < min {
                        lo
                    } else if hi > max {
                        hi
                    } else {
                        lo
                    }
                };
                return Err(GridError::OutOfBounds {
                    x: violating(f_min_x, f_max_x, min_x, max_x),
                    y: violating(f_min_y, f_max_y, min_y, max_y),
                    max_x,
                    max_y,
                });
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

        let collides = footprint_cells((top_left_x, top_left_y), (w, h))
            .any(|cell| self.cells.contains_key(&cell));

        Ok(!collides)
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

        // Collision check — first occupied cell in row-major order wins.
        for (cx, cy) in footprint_cells((top_left_x, top_left_y), (w, h)) {
            if let Some(CellState::Occupied { entity_id }) = self.cells.get(&(cx, cy)) {
                return Err(GridError::Collision {
                    x: cx,
                    y: cy,
                    occupant: *entity_id,
                });
            }
        }

        // Allocate entity
        let id = EntityId(self.entities.len());
        let entity = PlacedEntity {
            id,
            prototype_name: proto.name.as_str(),
            top_left: GridPos {
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
            filters: Vec::new(),
        };
        self.entities.push(Some(entity));
        self.live_count += 1;

        // Occupy cells
        for cell in footprint_cells((top_left_x, top_left_y), (w, h)) {
            self.cells.insert(cell, CellState::Occupied { entity_id: id });
        }

        // Register in spatial index for fast range queries
        self.spatial.insert(id, (top_left_x, top_left_y), (w, h));

        // Expand incremental bounding box cache to include this entity's footprint.
        // `get_or_insert` initialises the cache on the first placement, then both
        // paths (init and expand) share the same min/max clamp below.
        let (entity_min_x, entity_min_y, entity_max_x, entity_max_y) =
            footprint_aabb((top_left_x, top_left_y), (w, h));
        let bb = self
            .bbox
            .get_or_insert((entity_min_x, entity_min_y, entity_max_x, entity_max_y));
        bb.0 = bb.0.min(entity_min_x);
        bb.1 = bb.1.min(entity_min_y);
        bb.2 = bb.2.max(entity_max_x);
        bb.3 = bb.3.max(entity_max_y);

        Ok(id)
    }

    /// Set an already-placed entity's filter slots, replacing whatever it had.
    ///
    /// A separate call rather than a sixth `place` argument, because a filter
    /// is not geometry: it cannot move a footprint, so neither the cell map,
    /// the spatial index nor the bbox cache can go stale from setting one —
    /// and 86 call sites that never filter anything would otherwise all grow
    /// a `None`.
    pub fn set_filters(&mut self, id: EntityId, filters: Vec<String>) -> Result<(), GridError> {
        let entity = self
            .entities
            .get_mut(id.0)
            .and_then(|slot| slot.as_mut())
            .ok_or(GridError::EntityNotFound(id))?;
        entity.filters = filters;
        Ok(())
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
        self.spatial
            .remove(id, (entity.top_left.x, entity.top_left.y), entity.size);

        // Free cells
        for cell in entity.cells() {
            self.cells.remove(&cell);
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
            let (entity_min_x, entity_min_y, entity_max_x, entity_max_y) = entity.aabb();

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
                        let (e_min_x, e_min_y, e_max_x, e_max_y) = e.aabb();
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

mod queries;
#[cfg(test)]
mod tests;

