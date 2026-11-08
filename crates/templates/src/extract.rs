// Template extraction: copies a rectangular region of a Grid into a Template.
use factorio_grid::GridPos;

use crate::model::{Template, TemplateEntity};

// ── Template extraction ───────────────────────────────────────────────

/// Extract a `Template` from a rectangular region of a `Grid`.
///
/// All entities whose footprint overlaps `[min_x, max_x] × [min_y, max_y]`
/// are included. Their positions are remapped so that `(min_x, min_y)` maps
/// to `(0, 0)` in the template coordinate space.
///
/// Inverted corners (`max < min`) are normalized before querying and measuring
/// so width/height stay positive and relative positions use the true origin.
///
/// `io_points` is left empty — the user assigns connection points in the UI.
pub fn extract_template(
    grid: &factorio_grid::Grid,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    name: &str,
) -> Template {
    let (min_x, max_x) = (min_x.min(max_x), min_x.max(max_x));
    let (min_y, max_y) = (min_y.min(max_y), min_y.max(max_y));

    let entities = grid.query_rect(min_x, min_y, max_x, max_y);

    let template_entities = entities
        .into_iter()
        .map(|e| TemplateEntity {
            prototype_name: e.prototype_name.to_string(),
            relative_pos: GridPos {
                x: e.top_left.x - min_x,
                y: e.top_left.y - min_y,
            },
            direction: e.direction,
            size: e.size,
            recipe: e.recipe.clone(),
            entity_type: e.entity_type.clone(),
        })
        .collect();

    Template {
        name: name.to_string(),
        width: (max_x - min_x + 1) as u32,
        height: (max_y - min_y + 1) as u32,
        entities: template_entities,
        io_points: vec![],
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use factorio_grid::factorio_blueprint::{self, Direction};
    use factorio_grid::GridPos;

    // ── extract_template tests ─────────────────────────────────────────

    /// Center position for a 1×1 entity placed at the given top-left grid cell.
    /// Factorio uses center-based coordinates: for a 1×1 entity, center = top_left + 0.5.
    fn center_1x1(top_left_x: i32, top_left_y: i32) -> factorio_blueprint::Position {
        factorio_blueprint::Position {
            x: top_left_x as f64 + 0.5,
            y: top_left_y as f64 + 0.5,
        }
    }

    /// Positions are remapped so that (min_x, min_y) in world space becomes (0, 0)
    /// in the resulting template.
    #[test]
    fn extract_template_remaps_positions_to_region_origin() {
        let mut grid = factorio_grid::Grid::new();
        // Belt at world top-left (5, 3)
        grid.place("transport-belt", &center_1x1(5, 3), Direction::North, None, None)
            .unwrap();

        // Extract region starting at (3, 1) → origin becomes (0, 0)
        let t = extract_template(&grid, 3, 1, 8, 5, "remap-test");

        assert_eq!(t.entities.len(), 1);
        // Belt world top-left = (5, 3); relative = (5 - 3, 3 - 1) = (2, 2)
        assert_eq!(t.entities[0].relative_pos, GridPos { x: 2, y: 2 });
    }

    /// width = max_x - min_x + 1, height = max_y - min_y + 1 (inclusive region).
    #[test]
    fn extract_template_width_and_height_equal_region_extents() {
        // No entities needed — dimensions come entirely from the region coordinates.
        let grid = factorio_grid::Grid::new();
        // Region (2, 4) → (8, 11): width = 7, height = 8
        let t = extract_template(&grid, 2, 4, 8, 11, "dims-test");
        assert_eq!(t.width, 7); // 8 - 2 + 1
        assert_eq!(t.height, 8); // 11 - 4 + 1
    }

    /// prototype_name, direction, size, and recipe are all faithfully copied from
    /// the PlacedEntity into the TemplateEntity.
    #[test]
    fn extract_template_copies_entity_fields() {
        let mut grid = factorio_grid::Grid::new();

        // Underground belt (1×1) at world top-left (1, 0), facing East.
        // Factorio `type` is input/output — not a prototype category tag.
        grid.place(
            "underground-belt",
            &center_1x1(1, 0),
            Direction::East,
            None,
            Some("input".to_string()),
        )
        .unwrap();

        // Assembler-2 (3×3) at world top-left (4, 0):
        //   center = (4 + 1.5, 0 + 1.5) = (5.5, 1.5)
        grid.place(
            "assembling-machine-2",
            &factorio_blueprint::Position { x: 5.5, y: 1.5 },
            Direction::South,
            Some("iron-gear-wheel".to_string()),
            None,
        )
        .unwrap();

        // Extract region (0, 0) → (9, 2) — both entities fit inside
        let t = extract_template(&grid, 0, 0, 9, 2, "fields-test");

        assert_eq!(t.entities.len(), 2);

        let ug_belt = t
            .entities
            .iter()
            .find(|e| e.prototype_name == "underground-belt")
            .expect("underground-belt not found");
        assert_eq!(ug_belt.direction, Direction::East);
        assert_eq!(ug_belt.size, (1, 1));
        assert!(ug_belt.recipe.is_none());
        assert_eq!(ug_belt.entity_type.as_deref(), Some("input"));
        // World top-left (1, 0), region origin (0, 0) → relative (1, 0)
        assert_eq!(ug_belt.relative_pos, GridPos { x: 1, y: 0 });

        let asm = t
            .entities
            .iter()
            .find(|e| e.prototype_name == "assembling-machine-2")
            .expect("assembler not found");
        assert_eq!(asm.size, (3, 3));
        assert_eq!(asm.direction, Direction::South);
        assert_eq!(asm.recipe, Some("iron-gear-wheel".to_string()));
        assert!(asm.entity_type.is_none());
        // Assembler world top-left (4, 0), region origin (0, 0) → relative (4, 0)
        assert_eq!(asm.relative_pos, GridPos { x: 4, y: 0 });
    }

    /// Extracting a region with no entities produces an empty entity list,
    /// empty io_points, the given name, and correct dimensions.
    #[test]
    fn extract_template_empty_region_produces_no_entities() {
        let grid = factorio_grid::Grid::new();
        let t = extract_template(&grid, 0, 0, 3, 3, "empty-region");
        assert!(t.entities.is_empty());
        assert!(t.io_points.is_empty());
        assert_eq!(t.name, "empty-region");
        assert_eq!(t.width, 4); // 3 - 0 + 1
        assert_eq!(t.height, 4);
    }

    /// Inverted corners must normalize: same hits/dims/relative_pos as ordered args.
    #[test]
    fn extract_template_inverted_region_normalizes_dimensions() {
        let mut grid = factorio_grid::Grid::new();
        // Belt at world top-left (5, 3)
        grid.place("transport-belt", &center_1x1(5, 3), Direction::North, None, None)
            .unwrap();

        let ordered = extract_template(&grid, 3, 1, 8, 5, "ordered");
        let inverted = extract_template(&grid, 8, 5, 3, 1, "inverted");

        assert_eq!(inverted.width, ordered.width);
        assert_eq!(inverted.height, ordered.height);
        assert_eq!(inverted.width, 6); // |8-3|+1
        assert_eq!(inverted.height, 5); // |5-1|+1
        assert_eq!(inverted.entities.len(), 1);
        assert_eq!(ordered.entities.len(), 1);
        // Origin is the true min corner (3, 1) either way → relative (2, 2)
        assert_eq!(inverted.entities[0].relative_pos, GridPos { x: 2, y: 2 });
        assert_eq!(
            inverted.entities[0].relative_pos,
            ordered.entities[0].relative_pos
        );
    }
}
