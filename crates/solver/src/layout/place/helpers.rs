// Placement primitives `place_cell` composes: the top-left -> centre
// conversion every entity needs, the inserter-direction search carried over
// unchanged from `rows.rs` (verified in-game; `rows.rs` is deleted in the
// next task, so this duplicates rather than shares it), a vertical belt
// column, and the gutter-inserter placement the far-lane rule rests on.
use factorio_grid::prototype::{effective_size, EntityPrototype};
use factorio_grid::Grid;

use factorio_blueprint::{Direction, Position};

use crate::layout::{LayoutError, ResolvedConfig};

/// The four cardinals a layout ever rotates an inserter to. North first
/// purely so the common (unrotated) `inserter`/`fast-inserter` case is
/// found on the first try.
const CARDINALS: [Direction; 4] =
    [Direction::North, Direction::East, Direction::South, Direction::West];

/// Rotate a centre-relative offset clockwise by `turns` quarter turns.
/// `(x, y) -> (-y, x)` is one turn — check: North's `(0, -1)` becomes
/// East's `(1, 0)`, matching the dump's own direction convention.
pub(super) fn rotate(offset: (f64, f64), turns: u8) -> (f64, f64) {
    (0..turns).fold(offset, |(x, y), _| (-y, x))
}

/// A rotated centre-relative offset, converted to an integer cell delta.
pub(super) fn to_delta(offset: (f64, f64)) -> (i32, i32) {
    (offset.0.round() as i32, offset.1.round() as i32)
}

/// Which cardinal direction makes `proto`'s pickup/insert offsets land on
/// the two cell deltas asked for (offsets from the inserter's own cell).
///
/// Derived from the prototype's own North-orientation `pickup_position` /
/// `insert_position` rather than assumed, so swapping in a different
/// inserter prototype cannot silently produce a backwards inserter.
pub(super) fn inserter_direction(
    proto: &EntityPrototype,
    pickup_delta: (i32, i32),
    insert_delta: (i32, i32),
) -> Result<Direction, LayoutError> {
    let unknown = || LayoutError::InserterUnknown(proto.name.clone());
    let pickup = proto.pickup_position.ok_or_else(unknown)?;
    let insert = proto.insert_position.ok_or_else(unknown)?;

    CARDINALS
        .into_iter()
        .find(|&dir| {
            let turns = dir.as_u8() / 4;
            to_delta(rotate(pickup, turns)) == pickup_delta
                && to_delta(rotate(insert, turns)) == insert_delta
        })
        .ok_or_else(unknown)
}

/// Place `proto` with its top-left at `top_left`. `Grid::place` wants a
/// centre; this is the one place that does the top-left -> centre
/// conversion, using the entity's size as rotated by `direction`.
pub(super) fn place_at(
    grid: &mut Grid,
    proto: &EntityPrototype,
    top_left: (i32, i32),
    direction: Direction,
    recipe: Option<String>,
) -> Result<(), LayoutError> {
    let (w, h) = effective_size(proto, direction);
    let center = Position {
        x: top_left.0 as f64 + w as f64 / 2.0,
        y: top_left.1 as f64 + h as f64 / 2.0,
    };
    grid.place(&proto.name, &center, direction, recipe, None)?;
    Ok(())
}

/// One belt entity per row from `y0` to `y0 + height - 1` at column `x`, all
/// facing South. Cells here run vertically — the spine and the two edges of
/// a cell — the opposite of `rows.rs`'s horizontal `place_belt_row`.
pub(super) fn place_belt_column(
    grid: &mut Grid,
    x: i32,
    y0: i32,
    height: u32,
    belt: &EntityPrototype,
) -> Result<(), LayoutError> {
    for dy in 0..height as i32 {
        place_at(grid, belt, (x, y0 + dy), Direction::South, None)?;
    }
    Ok(())
}

/// Which side of a machine an inserter serves: `Ingredient` picks up from
/// the belt and inserts into the machine; `Product` is the mirror image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    Ingredient,
    Product,
}

/// Places one inserter in gutter tile `at`, reaching belt slot `slot`
/// (0 = adjacent) in the direction `dir` (`+1`/`-1` along x) points from the
/// gutter toward the belts.
///
/// Distance `slot + 1` selects `cfg.inserter` (1) or `cfg.long_inserter` (2)
/// — and mirrors onto the machine side too, which is why a long inserter
/// needs `mw >= 2` to have somewhere real to insert into or pick up from
/// (checked by the caller before this runs).
pub(super) fn place_gutter_inserter(
    grid: &mut Grid,
    cfg: &ResolvedConfig,
    at: (i32, i32),
    dir: i32,
    slot: u32,
    role: Role,
) -> Result<(), LayoutError> {
    let d = slot as i32 + 1;
    let proto = if d == 1 { cfg.inserter } else { cfg.long_inserter };
    let belt_delta = dir * d;
    let machine_delta = -belt_delta;
    let (pickup_delta, insert_delta) = match role {
        Role::Ingredient => ((belt_delta, 0), (machine_delta, 0)),
        Role::Product => ((machine_delta, 0), (belt_delta, 0)),
    };
    let direction = inserter_direction(proto, pickup_delta, insert_delta)?;
    place_at(grid, proto, at, direction, None)
}
