use factorio_blueprint::{Direction, Position};
use serde::{Deserialize, Serialize};

// ── Grid position (integer cell coordinates) ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GridPos {
    pub x: i32,
    pub y: i32,
}

// ── Entity identity ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(pub(crate) usize);

// ── Cell state ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    Occupied { entity_id: EntityId },
}

// ── Placed entity ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PlacedEntity {
    pub id: EntityId,
    pub prototype_name: &'static str,
    /// Top-left cell of the entity's footprint, in integer grid coordinates.
    ///
    /// Deliberately **not** named `position`: in Factorio's JSON (and in the
    /// `blueprint` crate) `position` always means the entity *center*, which
    /// lives in `center` below. Keeping the names distinct stops grid entities
    /// and blueprint entities from being silently conflated.
    pub top_left: GridPos,
    /// Factorio center position — what a blueprint calls `position`.
    pub center: Position,
    pub direction: Direction,
    pub size: (u32, u32),
    pub recipe: Option<String>,
    pub entity_type: Option<String>,
}

// ── Footprint geometry ───────────────────────────────────────────────

/// Inclusive axis-aligned bounding box `(min_x, min_y, max_x, max_y)` of a
/// footprint anchored at `top_left` with the given `size`. The `+ size - 1`
/// converts the top-left origin into the inclusive bottom-right corner — the
/// off-by-one-prone step, defined once here so every call site (bbox cache,
/// query overlap, chunk range) stays consistent.
pub(crate) fn footprint_aabb(top_left: (i32, i32), size: (u32, u32)) -> (i32, i32, i32, i32) {
    let (x, y) = top_left;
    (x, y, x + size.0 as i32 - 1, y + size.1 as i32 - 1)
}

/// Every cell covered by a footprint anchored at `top_left` with the given
/// `size`, in row-major order (all of row `y`, then row `y + 1`, …).
///
/// Defined once so collision detection, cell occupation and cell freeing walk
/// the *same* set of cells in the *same* order — a placement that reports a
/// collision at cell C is then guaranteed to be the placement that would have
/// occupied C. Row-major order is part of the contract: `Grid::place` reports
/// the first colliding cell it finds, and callers (and tests) rely on that
/// being the topmost-leftmost one.
pub(crate) fn footprint_cells(
    top_left: (i32, i32),
    size: (u32, u32),
) -> impl Iterator<Item = (i32, i32)> {
    let (x, y) = top_left;
    let (w, h) = (size.0 as i32, size.1 as i32);
    (0..h).flat_map(move |dy| (0..w).map(move |dx| (x + dx, y + dy)))
}

impl PlacedEntity {
    /// Inclusive AABB `(min_x, min_y, max_x, max_y)` of this entity's footprint
    /// in cell coordinates.
    pub fn aabb(&self) -> (i32, i32, i32, i32) {
        footprint_aabb((self.top_left.x, self.top_left.y), self.size)
    }

    /// Every cell this entity occupies, in row-major order.
    pub fn cells(&self) -> impl Iterator<Item = (i32, i32)> {
        footprint_cells((self.top_left.x, self.top_left.y), self.size)
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_grid_pos_equality_and_hash() {
        let a = GridPos { x: 1, y: 2 };
        let b = GridPos { x: 1, y: 2 };
        let c = GridPos { x: 3, y: 4 };
        assert_eq!(a, b);
        assert_ne!(a, c);

        // Works as HashMap key
        let mut map = HashMap::new();
        map.insert(a, "hello");
        assert_eq!(map.get(&b), Some(&"hello"));
        assert_eq!(map.get(&c), None);
    }

    #[test]
    fn test_footprint_cells_row_major_and_complete() {
        // 3×2 footprint anchored at (-1, 4): 6 cells, row-major.
        let cells: Vec<(i32, i32)> = footprint_cells((-1, 4), (3, 2)).collect();
        assert_eq!(
            cells,
            vec![(-1, 4), (0, 4), (1, 4), (-1, 5), (0, 5), (1, 5)]
        );

        // 1×1 is the single anchor cell.
        assert_eq!(
            footprint_cells((7, -3), (1, 1)).collect::<Vec<_>>(),
            vec![(7, -3)]
        );

        // Cell set and AABB must agree: every cell lies inside the AABB, and the
        // AABB corners are both produced.
        let (min_x, min_y, max_x, max_y) = footprint_aabb((2, 2), (4, 5));
        let cells: Vec<(i32, i32)> = footprint_cells((2, 2), (4, 5)).collect();
        assert_eq!(cells.len(), 20);
        assert!(cells
            .iter()
            .all(|&(x, y)| x >= min_x && x <= max_x && y >= min_y && y <= max_y));
        assert_eq!(cells.first(), Some(&(min_x, min_y)));
        assert_eq!(cells.last(), Some(&(max_x, max_y)));
    }

    #[test]
    fn test_entity_id_newtype() {
        let id0 = EntityId(0);
        let id1 = EntityId(1);
        let id0b = EntityId(0);
        assert_ne!(id0, id1);
        assert_eq!(id0, id0b);
    }
}
