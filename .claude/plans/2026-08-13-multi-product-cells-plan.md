# Multi-product cells — Implementation Plan

**Goal:** Let a step with two item results get a columnar layout — each product
on its own belt, separated by inserter filters — instead of
`LayoutError::MultipleProducts`.

**Architecture:** The product side of a cell allocates whole *belts* among
products exactly the way the ingredient side already allocates *lanes* among
ingredients (`allocate_lanes`, reused verbatim). Because an inserter only ever
reaches a belt's far lane, a column owns one lane per belt and a belt therefore
carries exactly one item — which only holds if the inserter filters that item,
so filters are plumbed from `place_cell` through the `Grid` model into the
emitted blueprint. `validate` then reads those filters back off the grid to
check delivered rate per product.

**Tech Stack:** Rust workspace — `factorio-blueprint`, `factorio-grid`,
`factorio-solver`. serde/serde_json for the blueprint wire format.

Spec: `.claude/plans/2026-08-13-multi-product-cells-design.md`

---

## File Structure

| File | Change |
| --- | --- |
| `crates/blueprint/src/types.rs` | `ItemFilter` type; `Entity.use_filters`, `Entity.filters` |
| `crates/grid/src/types.rs` | `PlacedEntity.filters: Vec<String>` |
| `crates/grid/src/grid.rs` | `Grid::set_filters`; `place` initialises `filters` empty |
| `crates/grid/src/export.rs` | emit `use_filters` + `filters` |
| `crates/grid/src/import.rs` | read `filters` back into the grid |
| `crates/solver/src/layout/error.rs` | `MultipleProducts` → `TooManyProductsForBelts` |
| `crates/solver/src/layout/cell.rs` | product allocation; `CellPlan.product_allocation`, `CellPlan::product_belts` |
| `crates/solver/src/layout/place.rs` | `product_slots` as `(slot, filter)`; set filters on placed inserters |
| `crates/solver/src/layout/validate.rs` | delivered rate per product, keyed on filter |

---

### Task 1: Filter plumbing — blueprint wire format and grid model  [Mode: Direct]

**Files:**
- Modify: `crates/blueprint/src/types.rs`, `crates/grid/src/types.rs`,
  `crates/grid/src/grid.rs`, `crates/grid/src/export.rs`,
  `crates/grid/src/import.rs`
- Test: inline `mod tests` in `export.rs` / `grid.rs`, plus
  `crates/grid/tests/blueprint_import.rs`

**Contracts:**

```rust
// factorio_blueprint — mirrors BlueprintItemFilter from lua-api 2.0.77
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemFilter {
    pub index: u32,                                             // required, 1-based
    #[serde(default, skip_serializing_if = "Option::is_none")] pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub quality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub comparator: Option<String>,
}

// on Entity
#[serde(default, skip_serializing_if = "Option::is_none")] pub use_filters: Option<bool>,
#[serde(default, skip_serializing_if = "Option::is_none")] pub filters: Option<Vec<ItemFilter>>,

// factorio_grid
pub struct PlacedEntity { /* … */ pub filters: Vec<String> }   // empty = unfiltered
impl Grid {
    pub fn set_filters(&mut self, id: EntityId, filters: Vec<String>) -> Result<(), GridError>;
}
```

`Grid::place`'s signature is **unchanged** — it initialises `filters` empty and
`set_filters` mutates afterwards. Filters are not geometry, so no spatial-index
or bbox invalidation is involved.

**Test Cases:**

```rust
// export.rs — pins the exact wire shape; this test is the contract with Factorio
#[test]
fn a_filtered_inserter_exports_use_filters_and_a_one_based_index() {
    let mut grid = Grid::new();
    let id = grid.place("inserter", &Position { x: 0.5, y: 0.5 }, Direction::North, None, None).unwrap();
    grid.set_filters(id, vec!["uranium-238".into()]).unwrap();

    let bp = to_blueprint(&grid, None, BLUEPRINT_VERSION_UNDER_TEST);
    let e = &bp.entities[0];
    assert_eq!(e.use_filters, Some(true), "use_filters defaults to false in game — it must be emitted");
    let f = e.filters.as_ref().unwrap();
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].index, 1);
    assert_eq!(f[0].name.as_deref(), Some("uranium-238"));
    assert_eq!(f[0].quality, None, "omitted so any quality passes the filter");
    assert_eq!(f[0].comparator, None);

    // and serialized, the keys are exactly these
    let v = serde_json::to_value(e).unwrap();
    assert_eq!(v["filters"][0], serde_json::json!({"index": 1, "name": "uranium-238"}));
}

#[test]
fn an_unfiltered_entity_emits_neither_key() {
    // every blueprint generated before this change must serialize byte-identically
    let v = serde_json::to_value(/* an unfiltered inserter Entity */).unwrap();
    assert!(v.get("use_filters").is_none() && v.get("filters").is_none());
}

#[test]
fn filters_survive_grid_to_blueprint_to_grid() { /* two filters, both names preserved, order kept */ }

#[test]
fn a_filter_with_no_name_round_trips() {
    // BlueprintItemFilter.name is optional; a quality-only filter from a real
    // blueprint must parse rather than error
}

#[test]
fn set_filters_on_a_missing_id_is_entity_not_found() { /* GridError::EntityNotFound */ }

#[test]
fn set_filters_leaves_the_entity_findable_at_its_cells() {
    // spatial index / bbox unaffected: get_at and query_rect still return it
}
```

**Constraints:**
- Adding fields to `Entity` breaks every struct-literal construction site.
  Find them all (`rg 'Entity \{' --type rust`) and fix each — `export.rs` and
  the blueprint/grid tests — rather than deriving `Default` to paper over it.
- `import::from_blueprint` must capture the `EntityId` that `grid.place`
  returns and call `set_filters` when the entity carries filters; a filter
  whose `name` is `None` contributes nothing to the grid's `Vec<String>`.
- Round-trip fidelity is the existing hard rule: an unfiltered entity must
  serialize exactly as it does today.

**Verification:**
`build-lock cargo test -p factorio-blueprint -p factorio-grid`

**Commit after passing.**

---

### Task 2: Product allocation, filtered placement, per-product validation  [Mode: Delegated]

**Files:**
- Modify: `crates/solver/src/layout/error.rs`, `crates/solver/src/layout/cell.rs`,
  `crates/solver/src/layout/place.rs`, `crates/solver/src/layout/validate.rs`
- Test: `crates/solver/src/layout/cell/tests.rs`,
  `crates/solver/src/layout/place/tests.rs`,
  `crates/solver/src/layout/validate/tests.rs`,
  `crates/solver/src/layout/tests.rs`

**Contracts:**

```rust
// error.rs — replaces MultipleProducts (nothing outside the crate matches on it)
TooManyProductsForBelts { recipe: String, products: Vec<String>, belts: u32 },

// cell.rs
pub struct CellPlan {
    // … existing fields …
    /// Product name -> belts allocated, summing to `topo.product_belts()`,
    /// in the step's own output order.
    pub product_allocation: Vec<(String, u32)>,
}
impl CellPlan {
    /// Belt-slot indices on the product side carrying `item`, nearest first.
    /// Whole belts only — unlike the ingredient side, a product belt cannot be
    /// split between two items, because a column reaches only its far lane.
    pub fn product_belts(&self, item: &str) -> Vec<u32>;
}
```

**Behaviour required:**

1. `size_step` drops the `outs.len() > 1` refusal and instead returns
   `TooManyProductsForBelts` when `outs.len() as u32 > topo.product_belts()`.
   This check must run **before** `allocate_lanes` is called on the product
   side — `compositions` has a `debug_assert!(total >= parts)`.
2. Product allocation reuses `allocate_lanes` with `total_lanes =
   topo.product_belts()`; `column_cap` is the min of its returned values. Empty
   outputs must still yield `INFINITY` (today's "no product, no cap").
3. `StreamExceedsOneLane` on `column_term == 0.0` names the product achieving
   the minimum, replacing the current `product.expect(..)`.
4. `bound_by` names every product achieving the column cap, matching how it
   already names every binding ingredient.
5. `place_cell` builds `Vec<(u32 /*slot*/, Option<String> /*filter*/)>` from
   `product_allocation`. Filter is `Some(item)` **only** when the step has two
   or more products, so single-product blueprints are unchanged to the byte.
   Both existing refusals follow the list (`len() > mh`; `needs_long` is
   `any(|(k, _)| *k >= 1)`).
6. `check_delivered_rate` keys claims on `(recipe, filter)` and checks **every**
   positive output. An item's lanes are those claimed under `Some(item)`, plus —
   only when the step has exactly one positive output — those claimed under
   `None`.

**Test Cases:**

```rust
// cell/tests.rs
#[test] fn two_products_split_the_product_belts() {
    // uranium-processing, ingredients_on: Edge (products on the 2-belt spine)
    // product_allocation == [("uranium-235", 1), ("uranium-238", 1)]
    // product_belts("uranium-235") == [0]; product_belts("uranium-238") == [1]
}
#[test] fn two_products_on_a_one_belt_side_is_too_many_products() {
    // default topology: products on the single edge belt -> TooManyProductsForBelts,
    // message names both products and the belt count
}
#[test] fn a_single_product_still_takes_every_product_belt() {
    // product_allocation == [(item, product_belts)], product_belts(item) == [0, 1]
    // and machines_per_cell is unchanged from the pre-change value
}
#[test] fn bound_by_names_the_larger_product() {
    // uranium-238 is ~142x uranium-235, so it alone binds
}

// place/tests.rs
#[test] fn each_product_gets_its_own_filtered_inserter() {
    // one inserter per product per machine; the two land on different belt
    // slots; each carries exactly its own item as its filter
}
#[test] fn a_single_product_cell_places_no_filters() {
    // every placed inserter has empty `filters`
}

// validate/tests.rs
#[test] fn a_second_product_one_belt_short_under_delivers() {
    // hand-built grid: both products' inserters filtered, but one product's
    // belt run is short. UnderDelivers names THAT item — the case the old
    // first-output-only check passed.
}
#[test] fn one_products_lanes_are_not_credited_to_the_other() {}

// layout/tests.rs
#[test] fn a_uranium_processing_plan_generates_and_validates() {
    // end to end through `generate_with_report`, products on the spine
}
```

**Constraints:**
- `allocate_lanes` is reused, not reimplemented. If it needs a doc-comment
  amendment to cover the product side, amend it — do not fork it.
- `CellPlan` is constructed in `size_step` and cloned in `tile::place_step`;
  adding a field must not need changes there beyond the struct literal.
- Every existing layout test must keep passing unchanged — this feature is
  purely additive for single-product steps. If a test's expectation shifts,
  that is a bug in the change, not a test to update.
- Comments carry *why*. The `MultipleProducts` doc comment explains why a
  second product had nowhere to go; its replacement must explain what changed
  (whole-belt allocation + filters) and why 3+ products still refuse.

**Verification:**
`build-lock cargo test -p factorio-solver` and `build-lock cargo clippy --workspace --all-targets`

**Commit after passing.**

---

## Execution
**Skill:** Subagent Dev
- Task 1 (Mode A): orchestrator implements directly
- Task 2 (Mode B): dispatched to a subagent, diff reviewed on return

Tasks are **sequential** — Task 2 does not compile until Task 1's
`Grid::set_filters` and `PlacedEntity.filters` exist.
