use crate::types::EntityId;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GridError {
    #[error("unknown prototype: {0}")]
    UnknownPrototype(String),

    #[error("collision at ({x}, {y}) with entity {occupant:?}")]
    Collision { x: i32, y: i32, occupant: EntityId },

    #[error("entity not found: {0:?}")]
    EntityNotFound(EntityId),

    #[error("out of bounds: ({x}, {y}) exceeds grid bounds ({max_x}, {max_y})")]
    OutOfBounds { x: i32, y: i32, max_x: i32, max_y: i32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_error_display() {
        let err = GridError::UnknownPrototype("modded-thing".to_string());
        assert!(err.to_string().contains("modded-thing"));

        let err = GridError::Collision { x: 5, y: 3, occupant: EntityId(7) };
        let msg = err.to_string();
        assert!(msg.contains("5") && msg.contains("3"));

        let err = GridError::EntityNotFound(EntityId(42));
        assert!(err.to_string().contains("42"));

        let err = GridError::OutOfBounds { x: 10, y: 20, max_x: 5, max_y: 5 };
        let msg = err.to_string();
        assert!(msg.contains("10") && msg.contains("20"));
    }
}
