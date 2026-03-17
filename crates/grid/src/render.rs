use crate::grid::Grid;
use crate::EntityCategory;

/// Render the grid as ASCII art.
///
/// Entity type is mapped to a single character using [`EntityCategory`].
/// Empty cells are rendered as '.'.
///
/// Returns an empty string if the grid contains no entities.
pub fn render_ascii(grid: &Grid) -> String {
    let (min, max) = match grid.bounding_box() {
        Some(bounds) => bounds,
        None => return String::new(),
    };

    let mut output = String::new();

    for y in min.y..=max.y {
        for x in min.x..=max.x {
            let ch = match grid.get_at(x, y) {
                Some(entity) => {
                    EntityCategory::from_prototype_name(entity.prototype_name).label_char()
                }
                None => '.',
            };
            output.push(ch);
        }
        output.push('\n');
    }

    output
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use factorio_blueprint::{Direction, Position};

    fn pos(x: f64, y: f64) -> Position {
        Position { x, y }
    }

    #[test]
    fn test_render_empty() {
        let grid = Grid::new();
        let ascii = render_ascii(&grid);
        assert_eq!(ascii, "");
    }

    #[test]
    fn test_render_single_belt() {
        let mut grid = Grid::new();
        grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
            .unwrap();
        let ascii = render_ascii(&grid);
        assert_eq!(ascii, "B\n");
    }

    #[test]
    fn test_render_3x3_assembler() {
        let mut grid = Grid::new();
        grid.place(
            "assembling-machine-2",
            &pos(0.5, 0.5),
            Direction::North,
            Some("iron-gear-wheel".to_string()),
            None,
        )
        .unwrap();

        let ascii = render_ascii(&grid);
        assert_eq!(ascii, "AAA\nAAA\nAAA\n");
    }

    #[test]
    fn test_render_mixed_entities() {
        let mut grid = Grid::new();

        // Assembler at center (2.5, 2.5) → top-left (1, 1), covers (1..3, 1..3)
        grid.place(
            "assembling-machine-1",
            &pos(2.5, 2.5),
            Direction::North,
            None,
            None,
        )
        .unwrap();

        // Belt at (0.5, 0.5) → cell (0, 0)
        grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
            .unwrap();

        // Belt at (0.5, 1.5) → cell (0, 1)
        grid.place("transport-belt", &pos(0.5, 1.5), Direction::North, None, None)
            .unwrap();

        // Belt at (0.5, 2.5) → cell (0, 2)
        grid.place("transport-belt", &pos(0.5, 2.5), Direction::North, None, None)
            .unwrap();

        let ascii = render_ascii(&grid);
        // Grid spans (0,0) to (3,3):
        // Row 0: B . . .
        // Row 1: B A A A
        // Row 2: B A A A
        // Row 3: . A A A
        let expected = "B...\nBAAA\nBAAA\n.AAA\n";
        assert_eq!(ascii, expected);
    }

    #[test]
    fn test_render_imported_blueprint() {
        const ASSEMBLER_SETUP: &str = concat!(
            "0eNqNkcFqwzAMhl9l6GxD6yYpyW3P0OMow0lFKrCVYDvrSsi7T1kgDJptvRgs8X2/",
            "kEao3YB9IE5QjUAJPVQ/agqcrdFJ7TVG9LXD8HLCNPTSQU6UCCNUb+Pyub/z4GsMUO",
            "0VsPUonF044lZ721yJURuB+y4K3PGc+gnVTsFd3klBwIb6GaTQsW7RBn27ooyg4ELSX",
            "CAzqYdMs2YSRwxJag852qxB/9gOT9g2ZcWGLFtlKViOfReSlrWmjQEPW85sw5k/7fx",
            "FeVZATcfL/SK1bN0M/HU3URBfUJz7Gf/AEL9leWHKrCzzbHcsiqOZpi/Cqcff",
        );

        let data = factorio_blueprint::decode(ASSEMBLER_SETUP).unwrap();
        let blueprint = data.blueprint.as_ref().expect("expected a blueprint");

        let result = crate::import::from_blueprint(blueprint);

        // Should have placed entities (no panics, no errors for known entities)
        assert!(result.grid.entity_count() > 0);

        let ascii = render_ascii(&result.grid);

        // Verify output is non-empty and contains expected characters
        assert!(!ascii.is_empty());

        // The blueprint has assembling machines, inserters, and belts
        assert!(ascii.contains('A'), "expected assembler chars in render");
        assert!(ascii.contains('I'), "expected inserter chars in render");
        assert!(ascii.contains('B'), "expected belt chars in render");
    }
}
