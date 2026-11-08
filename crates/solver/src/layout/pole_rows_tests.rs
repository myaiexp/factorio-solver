// Tests for the pole-row choice `generate` makes: the cheap `WhereNeeded` layout first,
// the `Always` fallback, and that poles never widen a block. Split out of `tests.rs`.
use super::*;
use crate::testsupport::{default_cfg, green_circuit_plan};

/// The cheap layout (pole rows only where a gutter is full) is tried first;
/// if its poles cannot be placed or do not cover the block, the block is
/// rebuilt with rows everywhere — the layout that has always generated.
#[test]
fn a_failed_cheap_pole_layout_falls_back_to_reserving_everywhere() {
    let mut tried = Vec::new();
    let out = with_pole_row_fallback(|rows| {
        tried.push(rows);
        match rows {
            PoleRows::WhereNeeded => Err(LayoutError::NoRoomForPole { step: "s".into(), pole: "p".into() }),
            PoleRows::Always => Ok(7),
        }
    });
    assert_eq!(out.unwrap(), 7);
    assert_eq!(tried, [PoleRows::WhereNeeded, PoleRows::Always]);

    // Any other refusal is the answer, not a reason to retry.
    let mut calls = 0;
    let out: Result<(), _> = with_pole_row_fallback(|_| {
        calls += 1;
        Err(LayoutError::EmptyPlan)
    });
    assert!(matches!(out, Err(LayoutError::EmptyPlan)));
    assert_eq!(calls, 1);
}

/// End to end, the flagship block is shorter than with rows everywhere
/// (its cable step reserves none), and that cheaper block validates on its
/// own — passing here through the fallback would hide a cheap layout that
/// never works.
#[test]
fn the_green_circuit_block_is_shorter_without_needless_pole_rows() {
    let plan = green_circuit_plan();
    let cfg = default_cfg();
    let height = |grid: &Grid| {
        let (min, max) = grid.bounding_box().unwrap();
        max.y - min.y + 1
    };
    let (cheap, _) = build(&plan, &cfg, PoleRows::WhereNeeded).unwrap();
    let (always, _) = build(&plan, &cfg, PoleRows::Always).unwrap();
    assert!(height(&cheap) < height(&always), "{} vs {}", height(&cheap), height(&always));
    validate::validate(&cheap, &plan, &cfg).expect("the cheaper block is valid without a fallback");
}

/// Poles never widen a block: every pole lies inside the footprint of what
/// was placed before any pole. Outside it, a pole stands where the player's
/// bus or the next block goes. The cheap pole-row layout first measured two
/// poles at x = -1, left of the block's own edge belt.
#[test]
fn poles_stay_inside_the_blocks_own_footprint() {
    let plan = green_circuit_plan();
    for rows in [PoleRows::WhereNeeded, PoleRows::Always] {
        let (grid, _) = build(&plan, &default_cfg(), rows).unwrap();
        let is_pole = |name: &str| name == "medium-electric-pole";
        let others: Vec<_> = grid.entities().filter(|e| !is_pole(e.prototype_name)).collect();
        let min_x = others.iter().map(|e| e.top_left.x).min().unwrap();
        let min_y = others.iter().map(|e| e.top_left.y).min().unwrap();
        let max_x = others.iter().map(|e| e.top_left.x + e.size.0 as i32 - 1).max().unwrap();
        let max_y = others.iter().map(|e| e.top_left.y + e.size.1 as i32 - 1).max().unwrap();
        for pole in grid.entities().filter(|e| is_pole(e.prototype_name)) {
            let p = pole.top_left;
            assert!(
                (min_x..=max_x).contains(&p.x) && (min_y..=max_y).contains(&p.y),
                "{rows:?}: pole at {p:?} outside the block ({min_x},{min_y})..=({max_x},{max_y})"
            );
        }
    }
}
