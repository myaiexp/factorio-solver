// Belt-lane maths: how many belts a step's ingredient rates actually need.
//
// A step's demand routinely exceeds one belt, and that is the normal path
// rather than a warning case: 45/s green circuits carries 135/s of copper
// cable internally, which is three blue belts against one belt of output.
use factorio_grid::prototype::EntityPrototype;

use crate::chain::ItemRate;
use crate::layout::lane::lane_throughput;

/// Lanes required to move `rate` items/sec, `ceil`ed to whole lanes.
///
/// A rate that divides exactly does not round up: 45/s on a blue belt is
/// exactly 2 lanes, not 3.
pub fn lanes_needed(rate: f64, belt: &EntityPrototype) -> u32 {
    let per_lane = lane_throughput(belt);
    debug_assert!(per_lane > 0.0, "lane maths on a non-belt: {}", belt.name);
    if rate <= 0.0 {
        return 0;
    }
    // A non-belt gives per_lane == 0.0, so this saturates to u32::MAX rather
    // than quietly reporting that one lane is enough.
    (rate / per_lane).ceil() as u32
}

/// One physical belt and what rides on each of its two lanes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BeltAssignment {
    pub left: Option<String>,
    pub right: Option<String>,
}

impl BeltAssignment {
    /// The distinct items on this belt, in lane order.
    pub fn items(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for lane in [self.left.as_deref(), self.right.as_deref()].into_iter().flatten() {
            if !out.contains(&lane) {
                out.push(lane);
            }
        }
        out
    }

    /// How many of this belt's two lanes carry `item`.
    pub fn lanes_for(&self, item: &str) -> u32 {
        [self.left.as_deref(), self.right.as_deref()]
            .iter()
            .filter(|lane| **lane == Some(item))
            .count() as u32
    }
}

/// Pack a step's item rates onto belts, at most two distinct items per belt.
/// Returns one entry per physical belt — which is also one entry per machine
/// sub-row, since a row's inserters can only reach the one belt beside them.
///
/// That reachability is what shapes the packing, and it is why this is not a
/// plain bin-pack of lanes into belt-halves. **Every belt in a chunk carries
/// the same item set**, because each sub-row's machines need every ingredient,
/// and a sub-row whose belt happened to hold only copper cable could not craft
/// anything. So a pair of items needs `max(lanes)` belts, not
/// `ceil(total_lanes / 2)`.
///
/// The design's worked example both ways: 45/s iron plate (2 lanes) beside
/// 135/s copper cable (6 lanes) shares out to **6** belts, while copper cable
/// alone takes both lanes of **3**.
///
/// Items are chunked in pairs, which is the general shape a future
/// second-input-belt topology needs. Today `rows` refuses a recipe with more
/// than two ingredients before it reaches here, since one belt is all a row's
/// inserters can reach.
pub fn pack_lanes(items: &[ItemRate], belt: &EntityPrototype) -> Vec<BeltAssignment> {
    let carried: Vec<&ItemRate> = items.iter().filter(|r| r.per_sec > 0.0).collect();
    let mut belts = Vec::new();

    for pair in carried.chunks(2) {
        match pair {
            // A lone item takes both lanes, halving the belts it needs. The
            // last belt runs single-laned when the lane count is odd.
            [only] => {
                let lanes = lanes_needed(only.per_sec, belt);
                for lane_pair in 0..lanes.div_ceil(2) {
                    let both = lane_pair * 2 + 1 < lanes;
                    belts.push(BeltAssignment {
                        left: Some(only.item.clone()),
                        right: both.then(|| only.item.clone()),
                    });
                }
            }
            // Two items share every belt, one per lane, so each sub-row can
            // craft. The wider demand sets the belt count.
            [a, b] => {
                let lanes =
                    lanes_needed(a.per_sec, belt).max(lanes_needed(b.per_sec, belt));
                for _ in 0..lanes {
                    belts.push(BeltAssignment {
                        left: Some(a.item.clone()),
                        right: Some(b.item.clone()),
                    });
                }
            }
            _ => unreachable!("chunks(2) yields one or two elements"),
        }
    }

    belts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::{blue_belt, rate};
    use factorio_grid::prototype::lookup;

    #[test]
    fn per_lane_is_half_the_belt() {
        // express-transport-belt has belt_throughput 45.0
        assert_eq!(lane_throughput(blue_belt()), 22.5);
        assert_eq!(lane_throughput(lookup("transport-belt").unwrap()), 7.5);
    }

    #[test]
    fn cable_demand_needs_six_lanes() {
        // The design's centerpiece: 135/s of copper cable on blue belts.
        assert_eq!(lanes_needed(135.0, blue_belt()), 6);
    }

    #[test]
    fn exact_multiple_does_not_round_up() {
        assert_eq!(lanes_needed(45.0, blue_belt()), 2); // 45 / 22.5 == 2 exactly
        assert_eq!(lanes_needed(22.5, blue_belt()), 1);
        assert_eq!(lanes_needed(22.6, blue_belt()), 2);
        assert_eq!(lanes_needed(0.0, blue_belt()), 0);
    }

    #[test]
    fn two_ingredients_share_one_belt() {
        // Green circuits: 45/s iron plate + 135/s cable -> plate fits 2 lanes,
        // cable 6.
        let packed =
            pack_lanes(&[rate("iron-plate", 45.0), rate("copper-cable", 135.0)], blue_belt());
        assert!(
            packed.iter().any(|b| b.left.is_some() && b.right.is_some()),
            "at least one belt carries two different items"
        );
    }

    /// The two numbers the design states, and the difference between them.
    #[test]
    fn sharing_costs_belts_that_dedicating_both_lanes_saves() {
        let shared =
            pack_lanes(&[rate("iron-plate", 45.0), rate("copper-cable", 135.0)], blue_belt());
        assert_eq!(shared.len(), 6, "cable's 6 lanes set the count, plate rides along");

        let dedicated = pack_lanes(&[rate("copper-cable", 135.0)], blue_belt());
        assert_eq!(dedicated.len(), 3, "both lanes to cable halves the belts");
    }

    /// Every belt must carry the full ingredient set — a sub-row fed a belt of
    /// cable and no iron plate cannot craft a circuit.
    #[test]
    fn every_shared_belt_carries_every_ingredient() {
        let packed =
            pack_lanes(&[rate("iron-plate", 45.0), rate("copper-cable", 135.0)], blue_belt());
        for belt in &packed {
            let mut items = belt.items();
            items.sort_unstable();
            assert_eq!(items, vec!["copper-cable", "iron-plate"], "{belt:?}");
        }
    }

    #[test]
    fn a_lone_item_with_an_odd_lane_count_leaves_the_last_lane_empty() {
        // 3 lanes -> 2 belts: one full, one half.
        let packed = pack_lanes(&[rate("iron-plate", 67.5)], blue_belt());
        assert_eq!(packed.len(), 2);
        assert_eq!(packed[0].lanes_for("iron-plate"), 2);
        assert_eq!(packed[1].lanes_for("iron-plate"), 1);
        assert_eq!(packed[1].right, None);
    }

    #[test]
    fn zero_rate_items_do_not_get_a_belt() {
        assert!(pack_lanes(&[], blue_belt()).is_empty());
        assert!(pack_lanes(&[rate("iron-plate", 0.0)], blue_belt()).is_empty());

        // A zero-rate item alongside a real one must not shift the real one
        // into a shared-belt pairing with a ghost.
        let packed = pack_lanes(&[rate("ghost", 0.0), rate("copper-cable", 135.0)], blue_belt());
        assert_eq!(packed.len(), 3, "cable alone still gets both lanes");
        assert!(packed.iter().all(|b| !b.items().contains(&"ghost")));
    }

    #[test]
    fn a_slower_belt_needs_more_of_them() {
        let yellow = lookup("transport-belt").unwrap(); // 15/s -> 7.5/s per lane
        let packed = pack_lanes(&[rate("copper-cable", 135.0)], yellow);
        assert_eq!(packed.len(), 9, "18 lanes at 7.5/s, both lanes per belt");
    }

    #[test]
    fn belt_assignment_reports_its_items_once_each() {
        let both = BeltAssignment {
            left: Some("iron-plate".into()),
            right: Some("iron-plate".into()),
        };
        assert_eq!(both.items(), vec!["iron-plate"]);
        assert_eq!(both.lanes_for("iron-plate"), 2);
        assert_eq!(both.lanes_for("copper-cable"), 0);
    }
}
