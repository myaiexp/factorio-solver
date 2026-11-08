// Tests for A* pathfinding: shortest paths, obstacles, diagonal rules and the cost bound.
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
