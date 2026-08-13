// A* shortest-path search over unoccupied grid cells.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::grid::Grid;
use crate::types::GridPos;

/// How far (in cells) beyond the axis-aligned box spanning `from`/`to` the search
/// is allowed to wander. Bounding the frontier guarantees termination — an
/// unreachable goal returns `None` after exhausting the box rather than expanding
/// the open, unbounded grid forever. 128 cells of slack is ample for the detours
/// a blueprint-scale layout needs.
const SEARCH_MARGIN: i32 = 128;

/// Tuning for [`find_path`].
#[derive(Debug, Clone, Default)]
pub struct AStarConfig {
    /// Optional cap on total path cost (step count). `None` = no cost limit.
    pub max_cost: Option<u32>,
    /// Permit 8-directional movement (diagonals). Default `false` → 4-directional.
    pub allow_diagonal: bool,
}

/// Frontier entry. Ordered so [`BinaryHeap`] (a max-heap) pops the lowest
/// f-score first, breaking ties toward the lower g-score (closer to the goal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Frontier {
    est: i64, // f = g + heuristic
    cost: i64, // g
    pos: GridPos,
}

impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse est so the smallest est is "greatest" for the max-heap.
        other
            .est
            .cmp(&self.est)
            .then_with(|| other.cost.cmp(&self.cost))
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Find the shortest path from `from` to `to`, treating occupied cells as walls.
///
/// The start and goal cells are always walkable even if occupied (endpoints may
/// sit inside an entity's footprint). Returns the path as a `Vec<GridPos>` ordered
/// `from` (inclusive) → `to` (inclusive), or `None` if no path exists within the
/// bounded search region / cost limit.
pub fn find_path(
    grid: &Grid,
    from: GridPos,
    to: GridPos,
    config: &AStarConfig,
) -> Option<Vec<GridPos>> {
    if from == to {
        return Some(vec![from]);
    }

    // Search box: the from/to span padded by SEARCH_MARGIN on every side.
    let min_x = from.x.min(to.x) - SEARCH_MARGIN;
    let min_y = from.y.min(to.y) - SEARCH_MARGIN;
    let max_x = from.x.max(to.x) + SEARCH_MARGIN;
    let max_y = from.y.max(to.y) + SEARCH_MARGIN;

    let walkable = |p: GridPos| -> bool {
        if p.x < min_x || p.x > max_x || p.y < min_y || p.y > max_y {
            return false;
        }
        // Endpoints are always traversable; other cells must be empty.
        p == from || p == to || grid.get_at(p.x, p.y).is_none()
    };

    let heuristic = |p: GridPos| -> i64 {
        let dx = (p.x - to.x).unsigned_abs() as i64;
        let dy = (p.y - to.y).unsigned_abs() as i64;
        if config.allow_diagonal {
            dx.max(dy) // Chebyshev
        } else {
            dx + dy // Manhattan
        }
    };

    let mut open = BinaryHeap::new();
    let mut g_score: HashMap<GridPos, i64> = HashMap::new();
    let mut came_from: HashMap<GridPos, GridPos> = HashMap::new();

    g_score.insert(from, 0);
    open.push(Frontier {
        est: heuristic(from),
        cost: 0,
        pos: from,
    });

    let max_cost = config.max_cost.map(|c| c as i64);

    while let Some(Frontier { cost, pos, .. }) = open.pop() {
        // Stale heap entry (a cheaper route to `pos` was found after this was queued).
        if cost > *g_score.get(&pos).unwrap_or(&i64::MAX) {
            continue;
        }

        if pos == to {
            return Some(reconstruct(&came_from, to));
        }

        let next_cost = cost + 1;
        if let Some(limit) = max_cost
            && next_cost > limit
        {
            continue;
        }

        for neighbor in neighbors(pos, config.allow_diagonal) {
            if !walkable(neighbor) {
                continue;
            }
            // Reject diagonal corner-cutting: both adjacent orthogonal cells must
            // be walkable, otherwise the path squeezes between two blocked tiles.
            let dx = neighbor.x - pos.x;
            let dy = neighbor.y - pos.y;
            if dx != 0 && dy != 0 {
                let orth_a = GridPos {
                    x: pos.x + dx,
                    y: pos.y,
                };
                let orth_b = GridPos {
                    x: pos.x,
                    y: pos.y + dy,
                };
                if !walkable(orth_a) || !walkable(orth_b) {
                    continue;
                }
            }
            if next_cost < *g_score.get(&neighbor).unwrap_or(&i64::MAX) {
                came_from.insert(neighbor, pos);
                g_score.insert(neighbor, next_cost);
                open.push(Frontier {
                    est: next_cost + heuristic(neighbor),
                    cost: next_cost,
                    pos: neighbor,
                });
            }
        }
    }

    None
}

/// Walk `came_from` back from the goal to produce a start→goal ordered path.
fn reconstruct(came_from: &HashMap<GridPos, GridPos>, goal: GridPos) -> Vec<GridPos> {
    let mut path = vec![goal];
    let mut current = goal;
    while let Some(&prev) = came_from.get(&current) {
        path.push(prev);
        current = prev;
    }
    path.reverse();
    path
}

/// The 4 orthogonal (or 8 including diagonal) neighbors of `pos`.
fn neighbors(pos: GridPos, allow_diagonal: bool) -> Vec<GridPos> {
    let mut out = vec![
        GridPos { x: pos.x + 1, y: pos.y },
        GridPos { x: pos.x - 1, y: pos.y },
        GridPos { x: pos.x, y: pos.y + 1 },
        GridPos { x: pos.x, y: pos.y - 1 },
    ];
    if allow_diagonal {
        out.extend([
            GridPos { x: pos.x + 1, y: pos.y + 1 },
            GridPos { x: pos.x + 1, y: pos.y - 1 },
            GridPos { x: pos.x - 1, y: pos.y + 1 },
            GridPos { x: pos.x - 1, y: pos.y - 1 },
        ]);
    }
    out
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
    fn test_trivial_same_cell() {
        let grid = Grid::new();
        let p = GridPos { x: 3, y: 3 };
        assert_eq!(find_path(&grid, p, p, &AStarConfig::default()), Some(vec![p]));
    }

    #[test]
    fn test_straight_line_on_empty_grid() {
        let grid = Grid::new();
        let path = find_path(
            &grid,
            GridPos { x: 0, y: 0 },
            GridPos { x: 3, y: 0 },
            &AStarConfig::default(),
        )
        .expect("path exists on empty grid");
        // Manhattan distance 3 → 4 cells inclusive.
        assert_eq!(path.len(), 4);
        assert_eq!(path.first(), Some(&GridPos { x: 0, y: 0 }));
        assert_eq!(path.last(), Some(&GridPos { x: 3, y: 0 }));
    }

    #[test]
    fn test_routes_around_obstacle() {
        let mut grid = Grid::new();
        // Wall of belts at x=1, y=-1..=1 blocks the straight path from (0,0)→(2,0).
        for y in -1..=1 {
            grid.place("transport-belt", &pos(1.5, y as f64 + 0.5), Direction::North, None, None)
                .unwrap();
        }
        let path = find_path(
            &grid,
            GridPos { x: 0, y: 0 },
            GridPos { x: 2, y: 0 },
            &AStarConfig::default(),
        )
        .expect("a detour exists");
        // Endpoints correct and no interior cell overlaps the wall.
        assert_eq!(path.first(), Some(&GridPos { x: 0, y: 0 }));
        assert_eq!(path.last(), Some(&GridPos { x: 2, y: 0 }));
        for step in &path[1..path.len() - 1] {
            assert!(
                !(step.x == 1 && (-1..=1).contains(&step.y)),
                "path cut through the wall at {step:?}"
            );
        }
    }

    #[test]
    fn test_no_path_when_goal_enclosed() {
        let mut grid = Grid::new();
        // Fully wall off cell (5,5) on all four sides.
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let cx = 5 + dx;
            let cy = 5 + dy;
            grid.place(
                "transport-belt",
                &pos(cx as f64 + 0.5, cy as f64 + 0.5),
                Direction::North,
                None,
                None,
            )
            .unwrap();
        }
        let result = find_path(
            &grid,
            GridPos { x: 0, y: 0 },
            GridPos { x: 5, y: 5 },
            &AStarConfig::default(),
        );
        assert!(result.is_none(), "enclosed goal must be unreachable");
    }

    #[test]
    fn test_diagonal_beats_orthogonal() {
        // Exercises the 8-directional neighbor generation and the Chebyshev
        // heuristic — both dead code paths under AStarConfig::default().
        let grid = Grid::new();
        let from = GridPos { x: 0, y: 0 };
        let to = GridPos { x: 3, y: 3 };

        let ortho = find_path(&grid, from, to, &AStarConfig::default())
            .expect("orthogonal path exists on empty grid");
        // Manhattan distance 6 → 7 cells inclusive.
        assert_eq!(ortho.len(), 7);

        let diag = find_path(
            &grid,
            from,
            to,
            &AStarConfig {
                allow_diagonal: true,
                ..Default::default()
            },
        )
        .expect("diagonal path exists on empty grid");
        // Chebyshev distance max(3,3)=3 → 4 cells inclusive: strictly shorter,
        // and equal to the theoretical minimum (proves optimality).
        assert_eq!(diag.len(), 4);
        assert!(diag.len() < ortho.len(), "diagonal route must be shorter");
        assert_eq!(diag.first(), Some(&from));
        assert_eq!(diag.last(), Some(&to));

        // A 3-step, 4-cell path from (0,0)→(3,3) can only be three (+1,+1)
        // moves — monotone toward the goal, every step a legal single hop.
        for pair in diag.windows(2) {
            let dx = pair[1].x - pair[0].x;
            let dy = pair[1].y - pair[0].y;
            assert_eq!((dx, dy), (1, 1), "step {pair:?} is not a monotone diagonal move");
        }
    }

    #[test]
    fn test_diagonal_rejects_corner_cutting() {
        // Walls on both corner-adjacent cells between (0,0) and (1,1). Without
        // the anti-corner-cut check, a single diagonal hop would slip through
        // the blocked corner. With the check, that hop is illegal and any path
        // must detour (path length > 2 cells inclusive).
        let mut grid = Grid::new();
        for (x, y) in [(1, 0), (0, 1)] {
            grid.place(
                "transport-belt",
                &pos(x as f64 + 0.5, y as f64 + 0.5),
                Direction::North,
                None,
                None,
            )
            .unwrap();
        }

        let from = GridPos { x: 0, y: 0 };
        let to = GridPos { x: 1, y: 1 };
        let diag_cfg = AStarConfig {
            allow_diagonal: true,
            ..Default::default()
        };

        let path = find_path(&grid, from, to, &diag_cfg).expect("detour around corner walls");
        assert_eq!(path.first(), Some(&from));
        assert_eq!(path.last(), Some(&to));
        assert!(
            path.len() > 2,
            "must not take a single diagonal hop through the blocked corner; got {path:?}"
        );

        // Every diagonal step on the path must have had both orthogonal sides
        // clear (the walls are only at (1,0) and (0,1) — never step across them).
        for pair in path.windows(2) {
            let dx = pair[1].x - pair[0].x;
            let dy = pair[1].y - pair[0].y;
            if dx != 0 && dy != 0 {
                let orth_a = GridPos {
                    x: pair[0].x + dx,
                    y: pair[0].y,
                };
                let orth_b = GridPos {
                    x: pair[0].x,
                    y: pair[0].y + dy,
                };
                assert!(
                    !(orth_a.x == 1 && orth_a.y == 0) && !(orth_b.x == 1 && orth_b.y == 0),
                    "diagonal step {pair:?} grazes wall at (1,0)"
                );
                assert!(
                    !(orth_a.x == 0 && orth_a.y == 1) && !(orth_b.x == 0 && orth_b.y == 1),
                    "diagonal step {pair:?} grazes wall at (0,1)"
                );
            }
            // Path interior must not land on the wall cells either.
            if pair[1] != to {
                assert!(
                    !(pair[1].x == 1 && pair[1].y == 0) && !(pair[1].x == 0 && pair[1].y == 1),
                    "path stepped onto wall at {:?}",
                    pair[1]
                );
            }
        }
    }

    #[test]
    fn test_diagonal_open_corner_still_allowed() {
        // Empty grid: (0,0)→(1,1) remains a single diagonal hop when both
        // orthogonal sides are free (sanity check that the guard is not over-strict).
        let grid = Grid::new();
        let path = find_path(
            &grid,
            GridPos { x: 0, y: 0 },
            GridPos { x: 1, y: 1 },
            &AStarConfig {
                allow_diagonal: true,
                ..Default::default()
            },
        )
        .expect("open diagonal hop");
        assert_eq!(
            path,
            vec![GridPos { x: 0, y: 0 }, GridPos { x: 1, y: 1 }]
        );
    }

    #[test]
    fn test_max_cost_bounds_reachability() {
        // Exercises the cost-limit early-exit branch. True path cost from
        // (0,0)→(3,0) is 3 steps (4 cells inclusive).
        let grid = Grid::new();
        let from = GridPos { x: 0, y: 0 };
        let to = GridPos { x: 3, y: 0 };

        let with_limit = |limit: u32| {
            find_path(
                &grid,
                from,
                to,
                &AStarConfig {
                    max_cost: Some(limit),
                    ..Default::default()
                },
            )
        };

        // Just below the true cost → goal never expanded within budget.
        assert!(
            with_limit(2).is_none(),
            "cost limit 2 (< 3) must yield no path"
        );

        // Exactly at the true cost → reachable.
        let at = with_limit(3).expect("cost limit 3 (== 3) must yield a path");
        assert_eq!(at.len(), 4);

        // Above the true cost → still reachable, same optimal path.
        let above = with_limit(10).expect("cost limit 10 (> 3) must yield a path");
        assert_eq!(above.len(), 4);
    }
}
