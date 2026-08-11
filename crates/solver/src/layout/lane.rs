// Which lane of a belt an inserter touches, and what one lane carries.
//
// The whole columnar topology rests on one in-game fact: an inserter always
// drops on the **far** lane of the belt it inserts into — the lane furthest
// from itself. There is no near-lane fallback. So a belt filled by inserters
// needs a machine column on both sides to use both of its lanes, while a belt
// filled from the bus does not.
//
// The rule is stated here as geometry over two cells rather than as a fact
// about a `Direction`, because deriving it from an inserter's rotation would
// re-import the orientation question this module exists to settle.
use factorio_grid::prototype::EntityPrototype;
use factorio_grid::GridPos;

/// Which of a belt's two lanes, named by the side of the belt it runs on
/// (not "left/right", which flips meaning with the belt's travel direction).
///
/// A vertical belt has East and West lanes; a horizontal one has North and
/// South. Both pairs live in the same enum because a lane is identified by
/// the compass side of the belt tile it occupies, whichever way the belt runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaneSide {
    North,
    South,
    East,
    West,
}

/// The furthest an inserter in this generator ever reaches: a
/// `long-handed-inserter` spans two tiles.
const MAX_REACH: i32 = 2;

/// The lane an inserter at `inserter_cell` drops onto when it inserts into
/// `belt_cell`: always the far side, never the near one.
///
/// `None` when the two cells are not orthogonally aligned, are the same cell,
/// or are further apart than an inserter can reach — in none of those cases
/// does the inserter touch that belt at all.
///
/// Distance 2 is deliberately included. A `long-handed-inserter` reaching over
/// an inner belt to the outer one of a pair drops on that outer belt's far
/// lane by the same rule, and the pairs this design places make that the
/// common case rather than an exotic one.
pub fn drop_lane(inserter_cell: GridPos, belt_cell: GridPos) -> Option<LaneSide> {
    let (dx, dy) = (belt_cell.x - inserter_cell.x, belt_cell.y - inserter_cell.y);
    match (dx, dy) {
        (0, 0) => None,
        // Vertically aligned: the far lane is the one away from the inserter,
        // so an inserter to the north drops south.
        (0, dy) if dy.abs() <= MAX_REACH => {
            Some(if dy > 0 { LaneSide::South } else { LaneSide::North })
        }
        (dx, 0) if dx.abs() <= MAX_REACH => {
            Some(if dx > 0 { LaneSide::East } else { LaneSide::West })
        }
        _ => None,
    }
}

/// Per-lane throughput: half the belt's rating, because a Factorio belt
/// carries two independent lanes that can hold different items.
///
/// `0.0` for anything that is not a belt. Callers reach this through
/// `LayoutConfig::resolve`, which rejects non-belts before they get here.
pub fn lane_throughput(belt: &EntityPrototype) -> f64 {
    belt.belt_throughput.unwrap_or(0.0) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::blue_belt;
    use factorio_grid::prototype::lookup;

    fn pos(x: i32, y: i32) -> GridPos {
        GridPos { x, y }
    }

    #[test]
    fn an_inserter_drops_on_the_far_lane() {
        // Inserter north of the belt drops on the belt's south lane.
        assert_eq!(drop_lane(pos(0, 0), pos(0, 1)), Some(LaneSide::South));
        assert_eq!(drop_lane(pos(0, 2), pos(0, 1)), Some(LaneSide::North));
        assert_eq!(drop_lane(pos(0, 1), pos(1, 1)), Some(LaneSide::East));
        assert_eq!(drop_lane(pos(1, 1), pos(0, 1)), Some(LaneSide::West));
    }

    /// A long-handed inserter reaching over the inner belt of a pair still
    /// drops on the outer belt's far lane — same rule, one tile further.
    #[test]
    fn a_long_inserter_two_tiles_away_still_has_a_far_lane() {
        assert_eq!(drop_lane(pos(0, 1), pos(2, 1)), Some(LaneSide::East));
        assert_eq!(drop_lane(pos(2, 1), pos(0, 1)), Some(LaneSide::West));
        assert_eq!(drop_lane(pos(0, 0), pos(0, 2)), Some(LaneSide::South));
    }

    #[test]
    fn non_adjacent_cells_have_no_drop_lane() {
        assert_eq!(drop_lane(pos(0, 0), pos(1, 1)), None, "diagonal");
        assert_eq!(drop_lane(pos(0, 0), pos(2, 3)), None, "distant and diagonal");
        assert_eq!(drop_lane(pos(0, 0), pos(3, 0)), None, "out of any inserter's reach");
        assert_eq!(drop_lane(pos(0, 0), pos(0, 0)), None, "the same cell");
    }

    #[test]
    fn two_inserters_facing_each_other_claim_different_lanes() {
        // The property the whole design rests on: a belt with a machine column
        // on each side has both lanes filled, by different columns.
        let belt = pos(0, 1);
        assert_ne!(drop_lane(pos(0, 0), belt), drop_lane(pos(0, 2), belt));

        let belt = pos(1, 0);
        assert_ne!(drop_lane(pos(0, 0), belt), drop_lane(pos(2, 0), belt));
    }

    #[test]
    fn per_lane_is_half_the_belt() {
        assert_eq!(lane_throughput(blue_belt()), 22.5);
        assert_eq!(lane_throughput(lookup("transport-belt").unwrap()), 7.5);
    }

    #[test]
    fn a_non_belt_carries_nothing_per_lane() {
        assert_eq!(lane_throughput(lookup("assembling-machine-2").unwrap()), 0.0);
    }
}
