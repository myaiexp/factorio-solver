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
    pub position: GridPos,
    pub center: Position,
    pub direction: Direction,
    pub size: (u32, u32),
    pub recipe: Option<String>,
    pub entity_type: Option<String>,
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
    fn test_entity_id_newtype() {
        let id0 = EntityId(0);
        let id1 = EntityId(1);
        let id0b = EntityId(0);
        assert_ne!(id0, id1);
        assert_eq!(id0, id0b);
    }
}
