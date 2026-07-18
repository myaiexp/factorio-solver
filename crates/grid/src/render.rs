// ASCII rendering of a grid's occupied cells for debugging and tests.
use crate::category::EntityCategory;
use crate::grid::Grid;

/// Maximum bounding-box area (in cells) that [`render_ascii`] will materialize.
///
/// The grid is sparse with unbounded coordinates, so two entities placed far
/// apart — e.g. an imported blueprint with a single outlier at 100k — would
/// otherwise force a nested loop over the entire bounding box: up to ~10^10
/// iterations and a multi-gigabyte String. Blueprint import accepts arbitrary
/// untrusted positions, so this cap keeps one outlier entity from hanging or
/// OOMing any caller. A 1000×1000 factory (1M cells) still renders in full.
pub const MAX_RENDER_CELLS: i64 = 1_000_000;

/// Render the grid as ASCII art.
///
/// Each occupied cell becomes the [`EntityCategory::label_char`] of its entity
/// (B=belt, I=inserter, A=assembler, …, ?=unknown); empty cells render as '.'.
/// Classification is delegated to [`EntityCategory`] so the substring-matching
/// ladder lives in exactly one place.
///
/// Returns an empty string if the grid contains no entities, or a short
/// bracketed placeholder (never valid art) when the bounding box would exceed
/// [`MAX_RENDER_CELLS`] — see that constant for why.
pub fn render_ascii(grid: &Grid) -> String {
    let (min, max) = match grid.bounding_box() {
        Some(bounds) => bounds,
        None => return String::new(),
    };

    // Extents are computed in i64: coordinates are unbounded i32, so an
    // adversarial pair near i32::MIN/MAX would overflow an i32 subtraction.
    let width = max.x as i64 - min.x as i64 + 1;
    let height = max.y as i64 - min.y as i64 + 1;
    if width.saturating_mul(height) > MAX_RENDER_CELLS {
        return format!(
            "<render skipped: {width}×{height} region exceeds {MAX_RENDER_CELLS}-cell cap>"
        );
    }

    // Past the guard the area fits usize; +height reserves the row newlines.
    let mut output = String::with_capacity((width * height + height) as usize);

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
        use factorio_blueprint::fixtures::ASSEMBLER_SETUP;

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

    // ── Oversized-region guard ───────────────────────────────────────

    #[test]
    fn test_render_far_apart_entities_are_capped() {
        // Two belts 100k cells apart: the bounding box is ~10^10 cells, which
        // would hang/OOM an unguarded renderer. It must return the placeholder
        // (and thus terminate immediately) instead of scanning every cell.
        let mut grid = Grid::new();
        grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
            .unwrap();
        grid.place(
            "transport-belt",
            &pos(100_000.5, 100_000.5),
            Direction::North,
            None,
            None,
        )
        .unwrap();

        let ascii = render_ascii(&grid);
        assert!(
            ascii.starts_with("<render skipped:"),
            "expected placeholder, got {} chars",
            ascii.len()
        );
    }

    #[test]
    fn test_render_at_cap_still_renders() {
        // A grid whose bounding box is within the cap renders normally rather
        // than tripping the guard.
        let mut grid = Grid::new();
        grid.place("transport-belt", &pos(0.5, 0.5), Direction::North, None, None)
            .unwrap();
        grid.place("transport-belt", &pos(9.5, 9.5), Direction::North, None, None)
            .unwrap();

        let ascii = render_ascii(&grid);
        assert!(!ascii.starts_with("<render skipped:"));
        assert!(ascii.contains('B'));
    }
}
