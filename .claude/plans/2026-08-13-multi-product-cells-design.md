# Multi-product cells

> Design doc — idea #3377. Lets a step with more than one *item* result get a
> columnar layout instead of `LayoutError::MultipleProducts`.

## The problem

`cell::size_step` refuses any step with two or more positive item outputs:

```rust
if outs.len() > 1 {
    return Err(LayoutError::MultipleProducts { .. });
}
```

The refusal is honest. Under the deleted row topology `uranium-processing` did
lay out, and what it built was wrong: both outputs went onto the same far lane
of the same belt, so the block's stated rate was never deliverable and the
machines jammed as soon as the mixed belt backed up.

The reason a second product has nowhere to go is the far-lane rule
(`lane.rs`). On the **ingredient** side a cell's two machine columns pick from
the *same* belts, so the cell divides `2 × ingredient_belts` lanes among its
ingredients — `lane_allocation` already does exactly that. On the **product**
side an inserter can only ever drop on the belt's far lane, so a column reaches
exactly **one lane per belt**: `product_belts` lanes, owned outright, not
shared with the other column. There is no second lane on a belt for a column to
put a second product on.

## Recipes this is about

Of 649 recipes, 212 have two or more all-item results; 16 are not `hidden`.
Subtract the ones already refused for other, still-correct reasons —
`kovarex-enrichment-process` and every asteroid crushing/reprocessing recipe are
cyclic (`reject_if_cyclic`), `scrap-recycling` has 12 products — and the set
this unlocks is:

| recipe | products |
| --- | --- |
| `uranium-processing` | `uranium-235`, `uranium-238` |
| `jellynut-processing` | `jellynut-seed`, `jelly` |
| `yumako-processing` | `yumako-seed`, `yumako-mash` |
| `copper-bacteria` | `copper-bacteria`, `spoilage` |
| `iron-bacteria` | `iron-bacteria`, `spoilage` |

All are exactly 2 products, which is also the most a cell side can ever hold
(`spine_belts`/`edge_belts` are capped at 2). Plus the 196 hidden recycling
recipes, reachable only through an explicit `recipe_overrides` entry.

## Approach: one belt per product, separated by inserter filters

**Each product claims whole belts on the product side**, allocated the same way
ingredients claim lanes, and the product inserters carry a **filter** naming
the item their belt is for.

The filter is not optional decoration. An unfiltered inserter takes whatever
sits in a machine's output slot, so two unfiltered inserters on two belts would
put a random mix on both — the row topology's bug with extra steps. Factorio 2.0
makes this cheap: `filter-inserter` and `stack-filter-inserter` were removed and
*every* inserter gained filter slots. The registry confirms it — the 169-entity
prototype table has exactly `inserter`, `fast-inserter`, `bulk-inserter`,
`stack-inserter`, `long-handed-inserter`, `burner-inserter` and no filter
variant. So no new prototype, no config knob, no change to `LayoutConfig`.

A belt then carries exactly one item on both its lanes — column A fills one
lane, column B the other — which is what the next step (and the player) can
actually consume.

### The mirror: allocate to a belt, never to a slot

Getting that right is not automatic, and the obvious formulation is wrong. A
cell's two columns face their product belts **from opposite sides**, so the
same *slot index* reaches *different physical belts*:

| | product gutter | belt reached by slot `k` |
| --- | --- | --- |
| column A | `gutter_a_right`, `dir: +1` | `spine_x0 + k` |
| column B | `gutter_b_left`, `dir: -1` | `spine_x0 + s - 1 - k` |

With `s = 2` — the only case that matters, since two products need exactly two
product belts — those are opposite ends, and 2 has no fixed point under that
reversal. Assigning items by slot index therefore puts a `uranium-235`-filtered
inserter and a `uranium-238`-filtered inserter on the *same* belt from opposite
sides, and every belt comes out 50/50 mixed: the exact failure this feature
exists to prevent, reintroduced by the mechanism meant to prevent it.

The same reversal governs the **shared edge belt** between adjacent cells when
products sit on the edge side: cell N's column B and cell N+1's column A face
that strip from opposite directions, so a step needing two or more cells breaks
there instead of within a cell.

So the allocation addresses a **physical belt**, never a slot. Canonical
ordering is ascending x within the product-side belt group; both columns, and
both sides of a shared edge, agree because they are naming the same strip.
Each column then derives its own slot:

```rust
let slot = if gutter.dir > 0 { p } else { n_product_belts - 1 - p };
```

`CellPlan::product_belts(item)` returns those physical indices, and its doc
comment has to say so — a future reader "simplifying" it back to slot indices
reintroduces the bug silently, because it still type-checks and still puts the
two products on different *slots*.

### Alternatives considered and rejected

- **Share a belt lane-wise between two products.** Impossible, not merely
  undesirable: a column reaches only the far lane of each belt, so one column
  cannot fill two lanes of one belt. It would need a machine column per
  product, which is a different topology.
- **Alternate machines between two output belts.** Every machine produces both
  products, so alternating splits neither stream — it halves both and mixes both.
- **Mixed belt with a combined throughput model** (`rate₁ + rate₂ ≤ lane`).
  Physically consistent, and it needs no filters, but the block's output is a
  mixed belt that nothing downstream can consume, and the delivered-rate check
  would go from "this lane carries this item at this rate" to a joint claim.
  It is the row topology's answer with arithmetic bolted on.

## Changes

### `blueprint` — the filter wire format  *(shipped in `b572f63`)*

Taken from Factorio's own runtime docs at **the matching version**
(`lua-api.factorio.com/2.0.77`), not guessed. `BlueprintEntity`'s inserter
section declares `filters :: array[BlueprintItemFilter]?`, `use_filters ::
boolean?` **defaulting to `false`**, and `filter_mode :: "whitelist" |
"blacklist"?` defaulting to `"whitelist"`. `BlueprintItemFilter` is `index ::
uint32` (**required**), plus optional `name`, `quality` and `comparator`.

Three consequences the shape dictates:

- **`use_filters: true` must be emitted.** It defaults to `false`, so a
  `filters` array alone would be stored and ignored — an inserter that looks
  filtered in the blueprint and grabs both products in game. This is the single
  most breakable detail in the design.
- **`filter_mode` is omitted**, because its default is already what we want.
- **`quality` and `comparator` are omitted.** They are optional, and leaving
  them out matches *any* quality. Pinning `"normal"` would make the inserter
  refuse a legendary `uranium-238` the moment quality modules are in the
  machine — a bug that only shows up on someone else's base.

So a generated product inserter serializes as:

```json
{ "name": "inserter", "position": {...}, "direction": 4,
  "use_filters": true, "filters": [{ "index": 1, "name": "uranium-238" }] }
```

`Entity` gains the two optional fields, and a new `ItemFilter` type mirroring
`BlueprintItemFilter` exactly — `name` optional, not `String`, so an imported
blueprint's quality-only filter round-trips instead of failing to parse:

```rust
pub struct ItemFilter {
    pub index: u32,                  // required; 1-based
    pub name: Option<String>,
    pub quality: Option<String>,
    pub comparator: Option<String>,
}
```

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub use_filters: Option<bool>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub filters: Option<Vec<ItemFilter>>,
```

Both `Option`, both skipped when absent, so every blueprint we already
round-trip serializes byte-identically. Today an imported filtered inserter
survives only because it falls into `Entity::extra`; typing the fields moves it
out of the untyped bag without changing the JSON.

> Still worth a paste-test: the field *shape* is confirmed against the
> version-matched docs, but nothing here has watched Factorio parse it. One test
> pins the emitted JSON verbatim, so a correction is a single edit.

### `grid` — carrying a filter  *(shipped in `b572f63`)*

- `PlacedEntity` gains `pub filters: Vec<String>` — empty means unfiltered.
  `Vec`, not `Option<String>`, because a real inserter has up to five slots and
  an imported blueprint may use them.
- `Grid::place` is **unchanged** (it already takes five arguments; a sixth for a
  field that 86 call sites never set is churn for nothing). Instead
  `Grid::set_filters(id, Vec<String>) -> Result<(), GridError>` mutates an
  already-placed entity. Safe by construction: filters are not geometry, so the
  spatial index and bbox cache cannot go stale. `GridError::EntityNotFound`
  already exists for a bad id.
- `export::to_blueprint` emits `use_filters`/`filters` when `filters` is
  non-empty, `None` for both otherwise.
- `import::from_blueprint` reads them back, so grid → blueprint → grid → blueprint
  is stable for the field this design adds. (`items`, `control_behavior` and
  `wires` are still dropped on that path — pre-existing, out of scope, backlogged.)

### `solver/layout/cell.rs` — sizing

Delete the `outs.len() > 1` refusal. In its place:

- **Refusal**, mirroring `TooManyIngredientsForLanes`:

  ```rust
  TooManyProductsForBelts { recipe: String, products: Vec<String>, belts: u32 }
  ```

  when `outs.len() as u32 > topo.product_belts()`. The message names the fix a
  player can act on — move products to the wider side (`ingredients_on`), widen
  it (`spine_belts`/`edge_belts`), or declare a product available so it comes off
  the bus as its own block. `MultipleProducts` is removed; nothing outside the
  solver crate matches on it (the UI only Displays `LayoutError`).

- **Allocation** reuses `allocate_lanes` verbatim, with `total_lanes =
  topo.product_belts()`. It already maximises `min_p (share_p × lane / rate_p)`
  over every strictly-positive composition, which is exactly the product-side
  question with "lane" reading "belt" — a column gets one lane per belt, so the
  two are the same unit here. Empty `outs` still yields `INFINITY`, preserving
  the current "no product, no cap" behaviour.

  ```rust
  let (product_allocation, prod_values) =
      allocate_lanes(&outs_rates, topo.product_belts(), lane);
  let column_cap = prod_values.iter().copied().fold(f64::INFINITY, f64::min);
  ```

- `CellPlan` gains `product_allocation: Vec<(String, u32)>` and

  ```rust
  /// Belt-slot indices on the product side carrying `item`, nearest first.
  pub fn product_belts(&self, item: &str) -> Vec<u32>
  ```

  Whole belts in allocation order — no spillover pairing, because unlike the
  ingredient side a belt here cannot be split between two items.

- `StreamExceedsOneLane` is raised for whichever product achieves the minimum
  when `column_term == 0.0`, replacing the `product.expect(..)`.
- `bound_by` names *every* product achieving the column cap, the way it already
  names every binding ingredient.

### `solver/layout/place.rs` — placement

`product_slots` goes from a count to a list:

```rust
let product_slots: Vec<(u32, Option<String>)> = ...  // (belt slot, filter)
```

built from `plan.product_allocation` via `CellPlan::product_belts`. The filter is
`Some(item)` only when the step has two or more products; a single-product step
places an **unfiltered** inserter exactly as today, so every blueprint the
generator emits now is unchanged to the byte. Row within the gutter stays `y +
slot`, so a cell's height and width are unaffected.

The two pre-placement refusals follow the list rather than the count:
`product_slots.len() as u32 > mh`, and `needs_long` becomes
`product_slots.iter().any(|(k, _)| *k >= 1)` (unchanged in meaning: slot ≥ 1 is
the outer belt of a pair).

### `solver/layout/validate.rs` — delivered rate, per product

`check_delivered_rate` currently takes the *first* positive output and counts
every product lane claimed by the recipe against it. With two products that
double-counts: each product would be credited with the other's belt.

The placed filter is what fixes it, and it is genuinely independent evidence —
the check reads the filter off the grid, not off `CellPlan`. Claims key on
`(recipe, filter)`:

```rust
HashMap<(String, Option<String>), HashSet<(GridPos, LaneSide)>>
```

and each step checks *every* positive output, not just the first. Lanes for an
item are those claimed under `Some(item)`, plus — only when the step has exactly
one positive output — those claimed under `None`. That second clause is what
keeps single-product behaviour bit-identical while multi-product steps are
checked per item.

**Per-item rates are not enough on their own.** Under the mirror bug above,
each product independently accumulates one lane per belt and independently
satisfies its own rate, so a rate check alone reports success on a block whose
belts are physically 50/50 mixed. So `validate` also asserts the invariant the
whole design rests on: **for one recipe, no belt run may be claimed under two
distinct product filters.** A violation is a hard error (`MixedProductBelt`),
never a warning — it is a silently-wrong blueprint, the category this module
exists to make impossible.

This is also why the obvious placement test is worthless. "The two products
land on different belt slots" passes with the mirror bug present, because slot
0 ≠ slot 1 at the `CellPlan` level even when both columns cross over. The test
has to assert on placed geometry: gather every product inserter's `(belt cell,
filter)` and require each physical belt cell to carry exactly one distinct
filter across *both* columns.

## Testing

- `cell.rs`: `uranium-processing` with products on a 2-belt side allocates
  `(1, 1)`; `bound_by` names `uranium-238` (the ~142× larger stream); the same
  step against a 1-belt product side is `TooManyProductsForBelts`; a
  single-product step's `product_allocation` gives that item every belt, and its
  `machines_per_cell` is unchanged from today.
- `place.rs`: a two-product cell places one inserter per product per machine,
  each carrying its own filter, and the two products land on different belt
  slots; a single-product cell places no filters at all.
- `validate.rs`: a hand-built two-product grid whose second product is one belt
  short fails `UnderDelivers` naming *that* item — the case the old
  first-output-only check passed.
- `export`/`import`: a filtered inserter survives grid → blueprint → grid, and
  one test pins the emitted JSON shape verbatim.
- End to end: `generate` on a `uranium-processing` plan with
  `ingredients_on: Side::Edge` (products on the 2-belt spine) returns a grid and
  passes `validate`.
