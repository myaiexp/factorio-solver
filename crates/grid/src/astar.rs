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

#[cfg(test)]
mod tests;
