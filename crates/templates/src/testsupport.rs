// Test fixtures shared by the model and JSON persistence tests.
use factorio_grid::factorio_blueprint::Direction;
use factorio_grid::GridPos;

use crate::model::TemplateEntity;

/// A 1x1 north-facing transport belt at `(x, y)` in template space.
pub(crate) fn make_entity(x: i32, y: i32) -> TemplateEntity {
    TemplateEntity {
        prototype_name: "transport-belt".to_string(),
        relative_pos: GridPos { x, y },
        direction: Direction::North,
        size: (1, 1),
        recipe: None,
        // Ordinary belts have no Factorio `type` field.
        entity_type: None,
    }
}
