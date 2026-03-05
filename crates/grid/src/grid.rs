use std::collections::HashMap;

use factorio_blueprint::{Direction, Position};

use crate::error::GridError;
use crate::prototype::{effective_size, lookup};
use crate::types::{CellState, EntityId, GridPos, PlacedEntity};

// ── Grid ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Grid {
    cells: HashMap<(i32, i32), CellState>,
    entities: Vec<Option<PlacedEntity>>,
    bounds: Option<(i32, i32, i32, i32)>, // (min_x, min_y, max_x, max_y)
    live_count: usize,
}

impl Grid {
    pub fn new() -> Self {
        Self {
            cells: HashMap::new(),
            entities: Vec::new(),
            bounds: None,
            live_count: 0,
        }
    }

    pub fn with_bounds(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Self {
        Self {
            cells: HashMap::new(),
            entities: Vec::new(),
            bounds: Some((min_x, min_y, max_x, max_y)),
            live_count: 0,
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

        // Bounds check
        if let Some((min_x, min_y, max_x, max_y)) = self.bounds {
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

        // Free cells
        let (w, h) = entity.size;
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

    /// Axis-aligned bounding box of all occupied cells.
    /// Returns `(top_left, bottom_right)` in cell coordinates, or `None` if empty.
    pub fn bounding_box(&self) -> Option<(GridPos, GridPos)> {
        if self.cells.is_empty() {
            return None;
        }

        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for &(x, y) in self.cells.keys() {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }

        Some((
            GridPos { x: min_x, y: min_y },
            GridPos { x: max_x, y: max_y },
        ))
    }

    /// Find all entities whose occupied cells fall within `radius` cells
    /// of the given center cell (Chebyshev distance).
    pub fn get_neighbors(&self, center: GridPos, radius: i32) -> Vec<&PlacedEntity> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let cx = center.x + dx;
                let cy = center.y + dy;
                if let Some(CellState::Occupied { entity_id }) = self.cells.get(&(cx, cy)) {
                    if seen.insert(*entity_id) {
                        if let Some(entity) = self.entities[entity_id.0].as_ref() {
                            result.push(entity);
                        }
                    }
                }
            }
        }

        result
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
}
