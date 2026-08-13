// Test-module root for the individual checks. Hand-built grids exercise the
// hard errors directly (through the private per-check functions, via
// `super::*`) rather than only through `generate`, since `generate`'s own
// cell/pole passes are supposed to make every one of these impossible — the
// point of this module is to prove that independently. The mandatory
// end-to-end coverage (a real generated block, the round-trip) lives in
// `tests/layout_output.rs` instead.
//
// Split to mirror the source's own `validate.rs` / `validate/delivery.rs`
// seam: `structure` is what must be *present* (inserters, poles, no overlap),
// `delivery` is what must actually *flow* through it.
use super::*;
use factorio_blueprint::{Direction, Position};

mod delivery;
mod structure;

/// A 3x3 machine with `recipe` set, top-left at `(x, y)`.
fn place_machine_at(grid: &mut Grid, recipe: &str, x: i32, y: i32) {
    grid.place(
        "assembling-machine-2",
        &Position { x: x as f64 + 1.5, y: y as f64 + 1.5 },
        Direction::North,
        Some(recipe.to_string()),
        None,
    )
    .unwrap();
}

/// A bare 3x3 machine with `recipe` set, top-left at (0, 0). Callers add
/// whatever inserters the case under test needs.
fn place_machine(grid: &mut Grid, recipe: &str) {
    place_machine_at(grid, recipe, 0, 0);
}

/// A single 1x1 inserter at `(x, y)` facing `dir`. `fast-inserter` matches
/// what `default_cfg()` resolves to, so its pickup/insert positions are the
/// same (0, -1) / (0, 1) the placement code relies on.
fn place_inserter(grid: &mut Grid, x: i32, y: i32, dir: Direction) {
    grid.place(
        "fast-inserter",
        &Position { x: x as f64 + 0.5, y: y as f64 + 0.5 },
        dir,
        None,
        None,
    )
    .unwrap();
}

/// A single 1x1 belt tile at `(x, y)`, facing `dir`. `express-transport-belt`
/// matches what `default_cfg()` resolves to (22.5/s per lane).
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

/// A single 1x1 inserter at `(x, y)` facing `dir`, filtered for `item` — the
/// same prototype `place_inserter` uses, plus the filter a multi-product
/// step's own output inserter would carry.
fn place_filtered_inserter(grid: &mut Grid, x: i32, y: i32, dir: Direction, item: &str) {
    let id = grid
        .place("fast-inserter", &Position { x: x as f64 + 0.5, y: y as f64 + 0.5 }, dir, None, None)
        .unwrap();
    grid.set_filters(id, vec![item.to_string()]).unwrap();
}
