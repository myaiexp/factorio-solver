// Belt-run identification for `check_delivered_rate`: many inserters along
// one physical run share its lane capacity, so counting capacity per tile
// (rather than per run) would let every block pass trivially — the failure
// mode that let #3364 ship.
use factorio_blueprint::Direction;
use factorio_grid::{Grid, GridPos, PlacedEntity};

/// The canonical anchor cell of the belt run `belt` belongs to: found by
/// walking backwards along the belt's own flow axis — North/South belts walk
/// the y axis, East/West walk the x axis, read from the *placed* direction
/// rather than assumed vertical — while the neighbouring cell holds a belt of
/// the same prototype and direction.
///
/// Every cell of one physical run resolves to the same anchor. Two different
/// runs never do, even when they sit at the same x a `STEP_GAP` apart, since
/// a mismatched prototype, a mismatched direction, or a gap of empty ground
/// all stop the walk.
pub(super) fn run_anchor(grid: &Grid, belt: &PlacedEntity) -> GridPos {
    let (dx, dy) = match belt.direction {
        Direction::North | Direction::South => (0, -1),
        _ => (-1, 0),
    };
    let mut cur = belt.top_left;
    loop {
        let next = GridPos { x: cur.x + dx, y: cur.y + dy };
        match grid.get_at(next.x, next.y) {
            Some(e) if e.prototype_name == belt.prototype_name && e.direction == belt.direction => {
                cur = next;
            }
            _ => return cur,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use factorio_blueprint::Position;

    fn place_belt(grid: &mut Grid, x: i32, y: i32, dir: Direction) {
        grid.place(
            "express-transport-belt",
            &Position { x: x as f64 + 0.5, y: y as f64 + 0.5 },
            dir,
            None,
            None,
        )
        .unwrap();
    }

    #[test]
    fn a_vertical_run_shares_one_anchor() {
        let mut grid = Grid::new();
        for y in 0..3 {
            place_belt(&mut grid, 0, y, Direction::South);
        }
        let anchor = GridPos { x: 0, y: 0 };
        for y in 0..3 {
            let belt = grid.get_at(0, y).unwrap();
            assert_eq!(run_anchor(&grid, belt), anchor);
        }
    }

    #[test]
    fn a_horizontal_run_walks_the_x_axis_not_y() {
        let mut grid = Grid::new();
        for x in 0..3 {
            place_belt(&mut grid, x, 0, Direction::East);
        }
        let anchor = GridPos { x: 0, y: 0 };
        for x in 0..3 {
            let belt = grid.get_at(x, 0).unwrap();
            assert_eq!(run_anchor(&grid, belt), anchor);
        }
    }

    /// Two steps can place belts at the same x, a `STEP_GAP` apart — the gap
    /// of empty ground must stop the walk, or the two steps' runs would be
    /// merged into one and their capacities double-counted or shared.
    #[test]
    fn a_gap_of_empty_ground_starts_a_new_run() {
        let mut grid = Grid::new();
        place_belt(&mut grid, 0, 0, Direction::South);
        place_belt(&mut grid, 0, 2, Direction::South); // y = 1 left empty
        let a = grid.get_at(0, 0).unwrap();
        let b = grid.get_at(0, 2).unwrap();
        assert_ne!(run_anchor(&grid, a), run_anchor(&grid, b));
    }

    /// A same-x run of a *different* prototype or direction is a different
    /// run too — matching only position would wrongly merge them.
    #[test]
    fn a_direction_change_starts_a_new_run() {
        let mut grid = Grid::new();
        place_belt(&mut grid, 0, 0, Direction::South);
        place_belt(&mut grid, 0, 1, Direction::North);
        let a = grid.get_at(0, 0).unwrap();
        let b = grid.get_at(0, 1).unwrap();
        assert_ne!(run_anchor(&grid, a), run_anchor(&grid, b));
    }
}
