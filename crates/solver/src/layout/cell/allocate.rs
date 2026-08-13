// The lane/belt allocation search `size_step` calls on both sides of a cell,
// split out of `cell.rs` to keep that file to the sizing formula itself.
// Ingredients divide `ingredient_lanes()` lanes this way; products divide
// `product_belts()` belts the identical way — a column reaches one lane per
// belt, so "lanes" and "belts" are the same unit to this search, and there is
// exactly one implementation of it.

/// Achieved `lanes * lane / rate` per item under `alloc`, and their min.
fn evaluate(alloc: &[u32], rates: &[(String, f64)], lane: f64) -> (Vec<f64>, f64) {
    let values: Vec<f64> =
        alloc.iter().zip(rates).map(|(&lanes, (_, rate))| (lanes as f64 * lane) / rate).collect();
    let cap = values.iter().copied().fold(f64::INFINITY, f64::min);
    (values, cap)
}

/// Searches every composition of `total_lanes` into `rates.len()` positive
/// parts for the one maximising the achieved minimum `lanes * lane / rate`.
/// Returns winning `(item, lanes)` pairs and each item's achieved value, both
/// in `rates`' order. Reused by `size_step` for the zero-allocation refusal
/// on either side and for `bound_by`.
pub(super) fn allocate_lanes(
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
    // returns `TooManyIngredientsForLanes`/`TooManyProductsForBelts` before we
    // get here; this catches a future caller that forgets to.
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

/// Names whichever side's cap set `machines_per_cell` — every ingredient
/// achieving the cell cap, every product achieving the column cap, or both
/// on an exact tie between the two sides. Two independent tolerances (one
/// per cap) because the two scales are unrelated — a single one keyed on
/// `cell_cap`, as before multiple products existed, would silently mis-tag
/// every product tie against the wrong cap.
#[allow(clippy::too_many_arguments)]
pub(super) fn binding_names(
    lane_allocation: &[(String, u32)],
    values: &[f64],
    cell_cap: f64,
    cell_term: f64,
    product_allocation: &[(String, u32)],
    prod_values: &[f64],
    column_cap: f64,
    column_term: f64,
) -> String {
    // A small relative tolerance so an exact tie isn't lost to float noise.
    let near = |x: f64, cap: f64| (x - cap).abs() <= 1e-9 * cap.max(1.0);
    let mut binding: Vec<String> = Vec::new();
    if cell_term <= column_term {
        for ((name, _), &value) in lane_allocation.iter().zip(values) {
            if near(value, cell_cap) {
                binding.push(name.clone());
            }
        }
    }
    if cell_term >= column_term {
        for ((name, _), &value) in product_allocation.iter().zip(prod_values) {
            if near(value, column_cap) {
                binding.push(name.clone());
            }
        }
    }
    binding.join(", ")
}
