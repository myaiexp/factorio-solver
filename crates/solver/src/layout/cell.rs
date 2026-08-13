// Cell sizing: pure arithmetic for how many machines fit in one cell of the
// columnar topology, before any entity is placed. `size_step` touches no
// `Grid` (that's a later task). The asymmetry every formula rests on is
// `lane.rs`'s inserter rule: picking shares a lane (both columns draw the
// same ingredient belts), dropping owns one (only one column reaches the
// far lane of a product belt).
use factorio_grid::prototype::EntityPrototype;
use serde::{Deserialize, Serialize};

use crate::chain::{ItemRate, ProductionStep};
use crate::layout::{lane::lane_throughput, LayoutError};

mod allocate;
use allocate::{allocate_lanes, binding_names};

/// Which side of a cell carries a stream. [`CellTopology::ingredients_on`]
/// names which side is which; the other side always carries the opposite.
///
/// Serialize/Deserialize: the UI persists a `CellTopology` (which embeds this)
/// as part of the chain panel's saved block-generator config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Spine,
    Edge,
}

/// Belt counts on each side and which side carries ingredients; independent of any one step.
///
/// Serialize/Deserialize: persisted verbatim as the chain panel's saved
/// `layout_topology` — see `factorio-ui`'s `persist.rs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellTopology {
    /// Belts on the spine between a cell's two machine columns. 1 or 2.
    pub spine_belts: u8,
    /// Belts on the cell edge, shared with the neighbouring cell. 1 or 2.
    pub edge_belts: u8,
    /// Which side carries ingredients. The other carries products.
    pub ingredients_on: Side,
    /// Tile cells left to right until adding one would exceed this, then wrap
    /// below. `None` = one band. Consumed by placement later; `size_step` ignores it.
    pub target_width: Option<u32>,
}

impl Default for CellTopology {
    /// Two spine belts, one edge belt, ingredients on the spine — the design's worked examples.
    fn default() -> Self {
        Self { spine_belts: 2, edge_belts: 1, ingredients_on: Side::Spine, target_width: None }
    }
}

impl CellTopology {
    /// Belt count on the ingredient side.
    pub fn ingredient_belts(&self) -> u32 {
        (if self.ingredients_on == Side::Spine { self.spine_belts } else { self.edge_belts }) as u32
    }

    /// Belt count on the product side — whichever `ingredients_on` doesn't.
    pub fn product_belts(&self) -> u32 {
        (if self.ingredients_on == Side::Spine { self.edge_belts } else { self.spine_belts }) as u32
    }

    /// Ingredient lanes a whole cell divides up: both columns pick from the
    /// same belts, so the cell gets *both* lanes — unlike product lanes,
    /// which a column owns alone.
    pub fn ingredient_lanes(&self) -> u32 {
        2 * self.ingredient_belts()
    }

    /// Rejects a belt count outside 1..=2, before any arithmetic runs on it.
    pub fn validate(&self) -> Result<(), LayoutError> {
        for (field, value) in [("spine_belts", self.spine_belts), ("edge_belts", self.edge_belts)] {
            if !(1..=2).contains(&value) {
                return Err(LayoutError::InvalidTopology { field: field.to_string(), value });
            }
        }
        Ok(())
    }
}

/// How one step tiles into cells, before any entity is placed.
#[derive(Debug, Clone, PartialEq)]
pub struct CellPlan {
    pub machines_per_cell: u32,
    /// Machines in each column of a FULL cell, evenly split, extra to the
    /// first. A partly-filled trailing cell is the caller's business.
    pub columns: (u32, u32),
    pub cells: u32,
    /// Ingredient name -> lanes allocated to it, summing to
    /// `topo.ingredient_lanes()`, in the step's own ingredient order.
    pub lane_allocation: Vec<(String, u32)>,
    /// Product name -> whole belts allocated to it, summing to
    /// `topo.product_belts()`, in the step's own output order. Unlike
    /// `lane_allocation` these are belts, not lanes: a column reaches only
    /// one lane of each belt, so there is nothing finer to allocate.
    pub product_allocation: Vec<(String, u32)>,
    /// The stream(s) that set `machines_per_cell`. Comma-joined when
    /// several bind at once — the normal outcome of an optimal lane
    /// allocation, not an edge case.
    pub bound_by: String,
}

impl CellPlan {
    /// Belt-slot indices on the ingredient side that carry `item`, nearest
    /// belt first (0 = adjacent to a machine column). Empty if `item` isn't
    /// an ingredient here.
    ///
    /// Each ingredient claims whole belts to itself first (`lanes / 2`); a
    /// leftover single lane spills onto a shared belt, paired two at a time
    /// with other leftovers, same order. 3 of a cell's 4 lanes thus gets one
    /// belt outright plus a shared spillover lane — a machine needs an
    /// inserter to *each* belt this returns. Placement depends on this
    /// being the single definition of the mapping.
    pub fn ingredient_belts(&self, item: &str) -> Vec<u32> {
        let mut slots: Vec<[Option<&str>; 2]> = Vec::new();
        let mut spillover: Vec<&str> = Vec::new();
        for (name, lanes) in &self.lane_allocation {
            for _ in 0..lanes / 2 {
                slots.push([Some(name.as_str()), Some(name.as_str())]);
            }
            if lanes % 2 == 1 {
                spillover.push(name.as_str());
            }
        }
        for pair in spillover.chunks(2) {
            slots.push(match pair {
                [a, b] => [Some(*a), Some(*b)],
                [a] => [Some(*a), None],
                _ => unreachable!("chunks(2) yields one or two elements"),
            });
        }
        slots
            .iter()
            .enumerate()
            .filter(|(_, lp)| lp.contains(&Some(item)))
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// PHYSICAL belt indices on the product side carrying `item`: `0` is the
    /// belt nearest the lower-x end of the product-side group, ascending
    /// from there — the same numbering for every caller, regardless of which
    /// machine column or which cell is asking. Whole belts only, in
    /// allocation order — unlike the ingredient side, a product belt cannot
    /// be split between two items (a column reaches only its far lane, so
    /// there is no leftover lane to spill onto a neighbour the way
    /// `ingredient_belts` pairs spillover lanes).
    ///
    /// This is deliberately NOT "distance from this column's own gutter" —
    /// column B's gutter faces the group from the opposite side, so its
    /// distance-from-gutter runs the other way. `place::place_cell` converts
    /// this physical index into each column's own distance; do not fold that
    /// conversion back in here; it would make it column-specific and wrong
    /// for whichever column didn't drive the change.
    pub fn product_belts(&self, item: &str) -> Vec<u32> {
        let mut slot = 0u32;
        let mut out = Vec::new();
        for (name, belts) in &self.product_allocation {
            if name == item {
                out.extend(slot..slot + belts);
            }
            slot += belts;
        }
        out
    }
}

/// Rates come from `step.inputs` / `step.outputs` divided by
/// `step.machines_needed`, never recomputed from the recipe — the chain
/// calculator already owns that.
pub fn size_step(
    step: &ProductionStep,
    belt: &EntityPrototype,
    topo: &CellTopology,
) -> Result<CellPlan, LayoutError> {
    topo.validate()?;
    let ins: Vec<ItemRate> = step.inputs.iter().filter(|r| r.per_sec > 0.0).cloned().collect();
    let outs: Vec<ItemRate> = step.outputs.iter().filter(|r| r.per_sec > 0.0).cloned().collect();

    // Fluids first: the more fundamental refusal, since `chain::solve` only
    // rejects a fluid *ingredient* off the bus. Moved here verbatim from
    // `rows::place_step`, which this replaces.
    for (rates, amounts) in [(&ins, &step.recipe.ingredients), (&outs, &step.recipe.results)] {
        if let Some(fluid) = rates.iter().find(|r| {
            amounts.iter().any(|a| a.name == r.item && a.kind == crate::recipe::ItemKind::Fluid)
        }) {
            return Err(LayoutError::FluidOnBelt {
                recipe: step.recipe.name.clone(),
                item: fluid.item.clone(),
            });
        }
    }
    // A cell column owns its product lanes outright, so each product needs a
    // whole belt to itself (separated from the rest by an inserter filter —
    // `place::place_cell`). Checked before `allocate_lanes` runs on the
    // product side below: `compositions` asserts `total >= parts`, and this
    // is what keeps that true.
    let belts_total = topo.product_belts();
    if outs.len() as u32 > belts_total {
        return Err(LayoutError::TooManyProductsForBelts {
            recipe: step.recipe.name.clone(),
            products: outs.iter().map(|r| r.item.clone()).collect(),
            belts: belts_total,
        });
    }
    let lanes_total = topo.ingredient_lanes();
    if ins.len() as u32 > lanes_total {
        return Err(LayoutError::TooManyIngredientsForLanes {
            recipe: step.recipe.name.clone(),
            ingredients: ins.iter().map(|r| r.item.clone()).collect(),
            lanes: lanes_total,
        });
    }
    // Never more machines than the step has; also guards a zero
    // `machines_needed` below.
    let machines = step.machines_needed.max(1);
    let lane = lane_throughput(belt);
    let ins_rates: Vec<(String, f64)> =
        ins.iter().map(|r| (r.item.clone(), r.per_sec / machines as f64)).collect();
    let (lane_allocation, values) = allocate_lanes(&ins_rates, lanes_total, lane);
    // `fold` over empty is `INFINITY` — "no ingredients, no cap", and the
    // same trick handles no products for `column_cap` below.
    let cell_cap = values.iter().copied().fold(f64::INFINITY, f64::min);
    // The product side asks the same question `allocate_lanes` already
    // answers for ingredients, just in belts instead of lanes: a column
    // reaches one lane per belt, so "lanes" and "belts" are the same unit
    // here. Both columns place this allocation identically (mirrored), which
    // is why `column_term` below still doubles it.
    let outs_rates: Vec<(String, f64)> =
        outs.iter().map(|r| (r.item.clone(), r.per_sec / machines as f64)).collect();
    let (product_allocation, prod_values) = allocate_lanes(&outs_rates, belts_total, lane);
    let column_cap = prod_values.iter().copied().fold(f64::INFINITY, f64::min);
    let cell_term = cell_cap.floor();
    let column_term = 2.0 * column_cap.floor();
    let stream_too_slow = |item: String, rate: f64| {
        LayoutError::StreamExceedsOneLane { recipe: step.recipe.name.clone(), item, rate, lane }
    };
    // Ingredient side first.
    if cell_term == 0.0 {
        let idx = values
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .expect("cell_cap is INFINITY, never 0.0, when values is empty");
        return Err(stream_too_slow(ins_rates[idx].0.clone(), ins_rates[idx].1));
    }
    if column_term == 0.0 {
        let idx = prod_values
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .expect("column_cap is INFINITY, never 0.0, when prod_values is empty");
        return Err(stream_too_slow(outs_rates[idx].0.clone(), outs_rates[idx].1));
    }
    let bound_by = binding_names(
        &lane_allocation,
        &values,
        cell_cap,
        cell_term,
        &product_allocation,
        &prod_values,
        column_cap,
        column_term,
    );
    let raw = cell_term.min(column_term);
    let machines_per_cell = if raw.is_finite() { (raw as u32).min(machines) } else { machines };
    let cells = (machines as f64 / machines_per_cell as f64).ceil() as u32;
    let columns = (machines_per_cell.div_ceil(2), machines_per_cell / 2);
    Ok(CellPlan { machines_per_cell, columns, cells, lane_allocation, product_allocation, bound_by })
}

#[cfg(test)]
mod tests;
