// Delivered-capacity checks: does the placed grid actually carry, per
// product, the rate `chain::solve` promised for it — and never as a mix of
// two products sharing one belt. Split out of `validate.rs` to keep that
// file to the structural checks (connectivity, overlap, pole coverage);
// everything here is keyed on an output inserter's claimed `(run, lane)`,
// read straight off the grid rather than trusted from `cell::size_step`'s
// own arithmetic — the same posture that let the parent module catch #3364.
use std::collections::{HashMap, HashSet};

use factorio_grid::{EntityCategory, Grid, GridPos};

use crate::chain::ProductionPlan;
use crate::layout::{drop_lane, lane_throughput, LaneSide, LayoutError, ResolvedConfig};

use super::runs::run_anchor;
use super::{inserter_cells, is_machine};

/// Per `(recipe, filter)`, the `(run, lane)` pairs claimed by that recipe's
/// output inserters carrying that filter — `None` for unfiltered.
type Claims = HashMap<(String, Option<String>), HashSet<(GridPos, LaneSide)>>;

/// An output inserter — pickup cell holds a machine, insert cell holds a
/// belt — reveals its filter directly (`PlacedEntity.filters`, empty for
/// unfiltered), the same field `place_cell` set it from. `Some(item)` claims
/// belong to that item alone; `None` (unfiltered) claims cannot be
/// attributed to any one product, so they are only ever credited when a step
/// has exactly one — the single-product case, where "unfiltered" already
/// means "the whole belt is this recipe's one product" and always has.
///
/// A step with two or more products but an unfiltered claim on file would be
/// a `place_cell` bug (it always filters once `product_allocation.len() >= 2`),
/// and crediting an unattributed claim to every product in that case would
/// hide exactly the double-counting bug this per-product keying exists to
/// catch, so it is withheld rather than guessed at.
fn output_claims(grid: &Grid) -> Claims {
    let mut claimed: Claims = HashMap::new();

    for inserter in grid
        .entities()
        .filter(|e| EntityCategory::from_prototype_name(e.prototype_name) == EntityCategory::Inserter)
    {
        let Some((pickup, insert)) = inserter_cells(inserter) else { continue };
        let Some(machine) = grid.get_at(pickup.0, pickup.1) else { continue };
        if !is_machine(machine.prototype_name) {
            continue;
        }
        let Some(belt) = grid.get_at(insert.0, insert.1) else { continue };
        if EntityCategory::from_prototype_name(belt.prototype_name) != EntityCategory::Belt {
            continue;
        }
        let Some(recipe) = machine.recipe.clone() else { continue };
        let inserter_cell = GridPos { x: inserter.top_left.x, y: inserter.top_left.y };
        let insert_cell = GridPos { x: insert.0, y: insert.1 };
        let Some(lane) = drop_lane(inserter_cell, insert_cell) else { continue };

        let filter = inserter.filters.first().cloned();
        claimed.entry((recipe, filter)).or_default().insert((run_anchor(grid, belt), lane));
    }
    claimed
}

/// No single physical belt **run** may be claimed by output inserters
/// filtered for two different items — that would mean the belt actually
/// carries a mix, which nothing downstream (a picking inserter, or the
/// player) can sort back apart. `place_cell` gives every product of a
/// multi-product step a whole belt of its own specifically to make this
/// impossible; a hit here means its physical-belt addressing has drifted
/// (see `LayoutError::MixedProductBelt`'s own doc comment for the bug this
/// caught in review). Built as its own pass over every inserter rather than
/// reusing `output_claims`'s map — a bug in that map's construction should
/// not also blind the one check meant to catch a mixed belt.
///
/// **Keyed on the run alone — deliberately not on `(run, lane)`, and not on
/// the belt tile.** Both narrower keys make this check decorative, and both
/// were measured doing exactly that: with the mirror bug reintroduced, the
/// two columns drop their different products onto the same belt column at
/// *different rows* and on *different lanes*, so a `(run, lane)` key splits
/// the two claims into separate buckets and a per-tile key gives each tile
/// one filter. Either way every product looks unmixed and the whole suite
/// passes. A belt's two lanes are one physical belt for this purpose: a
/// downstream inserter picks from both, so "one item per lane" is not the
/// invariant — "one item per run" is. This is the same trap as #3364, where
/// counting delivered capacity per tile instead of per run let every block
/// pass trivially.
pub(super) fn check_no_mixed_product_belts(grid: &Grid) -> Result<(), LayoutError> {
    let mut items_by_belt: HashMap<(String, GridPos), HashSet<String>> = HashMap::new();

    for inserter in grid
        .entities()
        .filter(|e| EntityCategory::from_prototype_name(e.prototype_name) == EntityCategory::Inserter)
    {
        // Unfiltered: either a single-product step (nothing to mix) or an
        // ingredient inserter, sorted out below by the pickup/insert shape
        // check same as `output_claims`.
        let Some(item) = inserter.filters.first() else { continue };
        let Some((pickup, insert)) = inserter_cells(inserter) else { continue };
        let Some(machine) = grid.get_at(pickup.0, pickup.1) else { continue };
        if !is_machine(machine.prototype_name) {
            continue;
        }
        let Some(belt) = grid.get_at(insert.0, insert.1) else { continue };
        if EntityCategory::from_prototype_name(belt.prototype_name) != EntityCategory::Belt {
            continue;
        }
        let Some(recipe) = machine.recipe.clone() else { continue };
        // The drop lane is still required, but only as proof this inserter
        // really reaches that belt — it must NOT enter the key (see above).
        let inserter_cell = GridPos { x: inserter.top_left.x, y: inserter.top_left.y };
        let insert_cell = GridPos { x: insert.0, y: insert.1 };
        if drop_lane(inserter_cell, insert_cell).is_none() {
            continue;
        }

        let key = (recipe, run_anchor(grid, belt));
        items_by_belt.entry(key).or_default().insert(item.clone());
    }

    // `HashMap` iteration order is arbitrary; sorted so a run with more than
    // one violation still reports the same one every time. Keyed on
    // primitive fields rather than the tuple itself — `GridPos` isn't `Ord`,
    // and giving it an ordering for this alone would invite a caller
    // elsewhere in the crate to lean on a comparison that has no real-world
    // meaning.
    let mut violations: Vec<_> = items_by_belt.into_iter().filter(|(_, items)| items.len() > 1).collect();
    violations.sort_by_key(|((recipe, anchor), _)| (recipe.clone(), anchor.x, anchor.y));

    if let Some(((recipe, anchor), items)) = violations.into_iter().next() {
        let mut items: Vec<String> = items.into_iter().collect();
        items.sort();
        return Err(LayoutError::MixedProductBelt { recipe, items, x: anchor.x, y: anchor.y });
    }
    Ok(())
}

/// Its far lane (`lane::drop_lane`) on a belt's run (`runs::run_anchor`) is
/// one claim on that run's capacity; two inserters claiming the same
/// `(run, lane)` share it rather than doubling it, which is why `output_claims`
/// collects a `HashSet` per key instead of a running total.
///
/// Sums each step's claimed lanes, per positive output, against
/// `lane_throughput(cfg.belt)` and fails the first output whose rate exceeds
/// what its claimed lanes can carry. A single-product step also collects its
/// unfiltered claims (see `output_claims`'s own doc comment for why only
/// then); a multi-product step is checked strictly per filter, so one
/// product's belts can never be credited to another's shortfall — the bug
/// checking only `step.outputs.iter().find(|r| r.per_sec > 0.0)` (the first
/// positive output) would have let: with two products, everything the second
/// one delivered was invisible to this check, and everything short of it
/// would have been silently covered by the first's belts instead. A step
/// with no positive output rate (nothing left after netting, or an
/// ingredient-only step) is skipped rather than required to deliver zero of
/// nothing.
pub(super) fn check_delivered_rate(
    grid: &Grid,
    plan: &ProductionPlan,
    cfg: &ResolvedConfig,
) -> Result<(), LayoutError> {
    let claimed = output_claims(grid);

    for step in &plan.steps {
        let positive: Vec<_> = step.outputs.iter().filter(|r| r.per_sec > 0.0).collect();
        for wanted in &positive {
            let mut lanes = claimed
                .get(&(step.recipe.name.clone(), Some(wanted.item.clone())))
                .map_or(0, HashSet::len);
            if positive.len() == 1 {
                lanes += claimed.get(&(step.recipe.name.clone(), None)).map_or(0, HashSet::len);
            }
            let delivered = lanes as f64 * lane_throughput(cfg.belt);
            if delivered < wanted.per_sec * (1.0 - 1e-9) {
                return Err(LayoutError::UnderDelivers {
                    recipe: step.recipe.name.clone(),
                    item: wanted.item.clone(),
                    wanted: wanted.per_sec,
                    delivered,
                });
            }
        }
    }
    Ok(())
}
