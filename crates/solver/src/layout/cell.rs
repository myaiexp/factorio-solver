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
    // A column owns its product lanes outright, so a two-product split has
    // no representation — deliberately a regression from the row topology,
    // whose `uranium-processing` layout dropped both outputs on one lane.
    if outs.len() > 1 {
        return Err(LayoutError::MultipleProducts {
            recipe: step.recipe.name.clone(),
            products: outs.iter().map(|r| r.item.clone()).collect(),
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
    // same trick handles an absent product for `column_cap` below.
    let cell_cap = values.iter().copied().fold(f64::INFINITY, f64::min);
    let product = outs.first().map(|r| (r.item.clone(), r.per_sec / machines as f64));
    let column_cap = product
        .as_ref()
        .map_or(f64::INFINITY, |(_, rate)| (topo.product_belts() as f64 * lane) / rate);
    let cell_term = cell_cap.floor();
    let column_term = 2.0 * column_cap.floor();
    let stream_too_slow = |item: String, rate: f64| {
        LayoutError::StreamExceedsOneLane { recipe: step.recipe.name.clone(), item, rate, lane }
    };
    // Ingredient side first; the second `expect` can't fire since an absent
    // product leaves `column_cap` (and its floor) `INFINITY`.
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
        let (item, rate) = product.expect("column_term == 0.0 implies a product set column_cap");
        return Err(stream_too_slow(item, rate));
    }
    let bound_by =
        binding_names(&lane_allocation, &values, cell_cap, cell_term, &product, column_term);
    let raw = cell_term.min(column_term);
    let machines_per_cell = if raw.is_finite() { (raw as u32).min(machines) } else { machines };
    let cells = (machines as f64 / machines_per_cell as f64).ceil() as u32;
    let columns = (machines_per_cell.div_ceil(2), machines_per_cell / 2);
    Ok(CellPlan { machines_per_cell, columns, cells, lane_allocation, bound_by })
}

/// Achieved `lanes * lane / rate` per item under `alloc`, and their min.
fn evaluate(alloc: &[u32], rates: &[(String, f64)], lane: f64) -> (Vec<f64>, f64) {
    let values: Vec<f64> =
        alloc.iter().zip(rates).map(|(&lanes, (_, rate))| (lanes as f64 * lane) / rate).collect();
    let cap = values.iter().copied().fold(f64::INFINITY, f64::min);
    (values, cap)
}

/// Searches every composition of `total_lanes` into `rates.len()` positive
/// parts for the one maximising the cell's ingredient cap. Returns winning
/// `(item, lanes)` pairs and each item's achieved value, both in `rates`'
/// order — reused by `size_step` for the zero-lane refusal and `bound_by`.
fn allocate_lanes(
    rates: &[(String, f64)],
    total_lanes: u32,
    lane: f64,
) -> (Vec<(String, u32)>, Vec<f64>) {
    if rates.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut best_alloc: Vec<u32> = Vec::new();
    let mut best_cap = f64::NEG_INFINITY;
    for alloc in compositions(total_lanes, rates.len()) {
        let (_, cap) = evaluate(&alloc, rates, lane);
        // Lex-decreasing order: strict `>` keeps the first — most lanes early — on a tie.
        if cap > best_cap {
            best_cap = cap;
            best_alloc = alloc;
        }
    }
    let (values, _) = evaluate(&best_alloc, rates, lane);
    let named: Vec<(String, u32)> =
        rates.iter().zip(best_alloc).map(|((name, _), lanes)| (name.clone(), lanes)).collect();
    (named, values)
}

/// Every composition of `total` into `parts` strictly-positive integers, in
/// lexicographically decreasing tuple order (never >4 lanes among >4 items here).
fn compositions(total: u32, parts: usize) -> Vec<Vec<u32>> {
    // Fewer lanes than ingredients has no composition at all, and the
    // descending `first` below would underflow reaching for one. `size_step`
    // returns `TooManyIngredientsForLanes` before we get here; this catches a
    // future caller that forgets to.
    debug_assert!(total as usize >= parts, "{parts} positive parts cannot sum to {total}");
    if parts == 0 {
        return Vec::new();
    }
    if parts == 1 {
        return vec![vec![total]];
    }
    let mut out = Vec::new();
    let mut first = total - (parts as u32 - 1); // leaves >=1 for every remaining part
    loop {
        for mut rest in compositions(total - first, parts - 1) {
            rest.insert(0, first);
            out.push(rest);
        }
        if first == 1 {
            break;
        }
        first -= 1;
    }
    out
}

/// Names whichever side's cap set `machines_per_cell` — ingredients at the
/// cell cap, the product at the column cap, or both on an exact tie.
fn binding_names(
    lane_allocation: &[(String, u32)],
    values: &[f64],
    cell_cap: f64,
    cell_term: f64,
    product: &Option<(String, f64)>,
    column_term: f64,
) -> String {
    // A small relative tolerance so an exact tie isn't lost to float noise.
    let achieves_min = |x: f64| (x - cell_cap).abs() <= 1e-9 * cell_cap.max(1.0);
    let mut binding: Vec<String> = Vec::new();
    if cell_term <= column_term {
        for ((name, _), &value) in lane_allocation.iter().zip(values) {
            if achieves_min(value) {
                binding.push(name.clone());
            }
        }
    }
    if cell_term >= column_term {
        if let Some((name, _)) = product {
            binding.push(name.clone());
        }
    }
    binding.join(", ")
}

#[cfg(test)]
mod tests;
