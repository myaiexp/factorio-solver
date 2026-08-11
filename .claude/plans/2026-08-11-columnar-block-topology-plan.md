# Columnar Block Topology Implementation Plan

**Goal:** Replace the block generator's row topology with configurable columnar cells, so a generated block delivers its target rate instead of half of it.

**Architecture:** `solver::layout` keeps its public shape — `generate(&ProductionPlan, &LayoutConfig) -> Grid` — but the interior inverts. `lanes.rs` and `rows.rs` are replaced by a lane-side model (`lane.rs`) and a cell builder (`cell.rs`), sized from belt throughput rather than machine count. `power.rs`, `error.rs` and the blueprint path are unchanged; `validate.rs` loses its belt-capacity warning and gains a delivered-rate check.

**Tech Stack:** Rust, edition 2024. No new dependencies.

**Design doc:** `.claude/plans/2026-08-11-columnar-block-topology-design.md` — read it first, especially "Ingredient lanes are per cell; product lanes are per column".

**Depends on:** Phase 6 (`solver::layout` exists). The chain calculator is untouched.

---

## File Structure

**Create:**
- `crates/solver/src/layout/lane.rs` — `Lane`, `LaneSide`, the far-lane rule, lane capacity
- `crates/solver/src/layout/cell.rs` — `CellTopology`, `CellPlan` sizing, lane allocation
- `crates/solver/src/layout/cell/tests.rs` — sizing tests (submodule file, as `rows.rs` does)
- `crates/solver/src/layout/place.rs` — turns a `CellPlan` into entities on the `Grid`
- `crates/solver/src/layout/place/tests.rs`

**Modify:**
- `crates/solver/src/layout/mod.rs` — `LayoutConfig` gains `topology`; `generate` tiles cells
- `crates/solver/src/layout/error.rs` — new variants, drop none
- `crates/solver/src/layout/validate.rs` — drop `belt_capacity_warnings`, add delivered-rate check
- `crates/solver/src/testsupport.rs` — fixtures for the worked examples
- `crates/solver/tests/layout_output.rs` — the over-capacity test goes, delivered-rate tests arrive
- `crates/ui/src/chain_panel/layout_controls.rs` — topology controls

**Delete:**
- `crates/solver/src/layout/lanes.rs` and `crates/solver/src/layout/rows.rs` (+ `rows/tests.rs`) — superseded. `pack_lanes`' belt-packing model is the bug; keeping it alongside the replacement would leave two answers to the same question.

---

### Task 1: The lane-side model

**Files:**
- Create: `crates/solver/src/layout/lane.rs`
- Modify: `crates/solver/src/layout/mod.rs` — add `long_inserter` to `LayoutConfig`/`ResolvedConfig`
- Test: inline `#[cfg(test)]`

**Contracts:**

`LayoutConfig` gains `pub long_inserter: String` (default `long-handed-inserter`)
and `ResolvedConfig` the matching `&'static EntityPrototype`, validated by
`resolve` exactly as the other three are. It lands here, in the first task,
because Task 3 places long inserters and cannot invent the field itself.
(`topology` arrives in Task 2, which is where `CellTopology` is defined.)

```rust
/// Which of a belt's two lanes, named by the side of the belt it runs on
/// (not "left/right", which flips meaning with travel direction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneSide { North, South, East, West }

/// The lane an inserter at `inserter_cell` drops onto when it inserts into
/// `belt_cell`: always the far side, never the near one.
pub fn drop_lane(inserter_cell: GridPos, belt_cell: GridPos) -> Option<LaneSide>;

/// Per-lane throughput: half the belt's rating.
pub fn lane_throughput(belt: &EntityPrototype) -> f64;
```

**Test Cases:**

```rust
#[test]
fn an_inserter_drops_on_the_far_lane() {
    // Inserter north of the belt drops on the belt's south lane.
    assert_eq!(drop_lane(pos(0, 0), pos(0, 1)), Some(LaneSide::South));
    assert_eq!(drop_lane(pos(0, 2), pos(0, 1)), Some(LaneSide::North));
    assert_eq!(drop_lane(pos(0, 1), pos(1, 1)), Some(LaneSide::East));
}

#[test]
fn non_adjacent_cells_have_no_drop_lane() { /* diagonal and distant both None */ }

#[test]
fn two_inserters_facing_each_other_claim_different_lanes() {
    // The property the whole design rests on: a belt with a machine column on
    // each side has both lanes filled, by different columns.
    let belt = pos(0, 1);
    assert_ne!(drop_lane(pos(0, 0), belt), drop_lane(pos(0, 2), belt));
}

#[test]
fn per_lane_is_half_the_belt() {
    assert_eq!(lane_throughput(blue_belt()), 22.5);
    assert_eq!(lane_throughput(lookup("transport-belt").unwrap()), 7.5);
}
```

**Constraints:**
- `drop_lane` takes cells, not directions — it is a geometric fact about two
  adjacent tiles, and deriving it from an inserter's `Direction` would re-import
  the orientation question this module exists to settle.
- Throughput comes from `belt_throughput`, never a hardcoded tier table.

**Verification:** `build-lock cargo test -p factorio-solver lane`

**Commit after passing.** `[Mode: Direct]`

---

### Task 2: Cell topology and sizing

**Files:**
- Create: `crates/solver/src/layout/cell.rs`, `crates/solver/src/layout/cell/tests.rs`
- Modify: `crates/solver/src/layout/error.rs`, `crates/solver/src/layout/mod.rs` (add `topology: CellTopology` to `LayoutConfig`)

**Contracts:**

```rust
pub enum Side { Spine, Edge }

pub struct CellTopology {
    pub spine_belts: u8,          // 1 or 2
    pub edge_belts: u8,           // 1 or 2
    pub ingredients_on: Side,
    pub target_width: Option<u32>,
}

impl Default for CellTopology { /* spine 2, edge 1, ingredients on spine, no width cap */ }

/// How one step tiles into cells, before any entity is placed.
pub struct CellPlan {
    pub machines_per_cell: u32,
    pub columns: (u32, u32),          // machines in each column, evenly split
    pub cells: u32,
    /// Ingredient name -> lanes allocated to it, summing to 2 * ingredient belts.
    pub lane_allocation: Vec<(String, u32)>,
    /// The stream that set `machines_per_cell`, for the UI and for tests.
    pub bound_by: String,
}

pub fn size_step(step: &ProductionStep, belt: &EntityPrototype, topo: &CellTopology)
    -> Result<CellPlan, LayoutError>;
```

New `LayoutError` variants:
```rust
TooManyIngredientsForLanes { recipe: String, ingredients: Vec<String>, lanes: u32 },
MultipleProducts { recipe: String, products: Vec<String> },
StreamExceedsOneLane { recipe: String, item: String, rate: f64, lane: f64 },
```

`TooManyIngredients` and `TooManyOutputs` are **removed** in the same edit —
these two replace them, and the replacements name the lane count, which is the
number that tells the user which knob to turn. Their only call site is
`rows.rs`, which Task 4 deletes; leaving them behind would leave two errors for
one condition. Every other variant stays.

**Test Cases:**

```rust
/// The design's worked table, asserted exactly. Both binding cases appear:
/// the circuit step is input-bound on cable, the cable step output-bound on
/// its own 2x yield.
#[test]
fn worked_example_express_belts() {
    let plan = green_circuit_plan();
    let topo = CellTopology::default();

    let circuit = size_step(step(&plan, "electronic-circuit"), blue_belt(), &topo).unwrap();
    assert_eq!(circuit.machines_per_cell, 15);
    assert_eq!(circuit.columns, (8, 7));
    assert_eq!(circuit.cells, 2);
    assert!(circuit.bound_by.contains("copper-cable"));

    let cable = size_step(step(&plan, "copper-cable"), blue_belt(), &topo).unwrap();
    assert_eq!(cable.machines_per_cell, 14);   // NOT 15: its product caps the column at 7
    assert_eq!(cable.columns, (7, 7));
    assert_eq!(cable.cells, 4);
    assert!(cable.bound_by.contains("copper-cable"));
}

#[test]
fn worked_example_yellow_belts() {
    let plan = green_circuit_plan();
    let topo = CellTopology::default();
    let yellow = lookup("transport-belt").unwrap();

    let circuit = size_step(step(&plan, "electronic-circuit"), yellow, &topo).unwrap();
    assert_eq!((circuit.machines_per_cell, circuit.columns, circuit.cells), (5, (3, 2), 6));

    let cable = size_step(step(&plan, "copper-cable"), yellow, &topo).unwrap();
    assert_eq!((cable.machines_per_cell, cable.columns, cable.cells), (4, (2, 2), 12));
}

/// Moving the product to the wider side turns 14 machines/cell into 30.
/// This is the reason the topology is configurable at all.
#[test]
fn flipping_the_topology_unbinds_an_output_bound_step() {
    let topo = CellTopology { edge_belts: 1, ingredients_on: Side::Edge, ..Default::default() };
    let cable = size_step(step(&green_circuit_plan(), "copper-cable"), blue_belt(), &topo).unwrap();
    assert_eq!(cable.machines_per_cell, 30);
    assert_eq!(cable.cells, 2);
}

#[test]
fn lane_allocation_maximises_machines_per_cell() {
    // A 3-ingredient recipe over 4 lanes: the searched allocation must beat or
    // match every other valid allocation. Enumerate them and check.
}

#[test]
fn lane_allocation_is_deterministic() { /* same input, same allocation, repeatedly */ }

#[test]
fn every_topology_combination_sizes_without_erroring() {
    // All 8 combinations of spine_belts x edge_belts x ingredients_on, on the
    // green-circuit plan. The arithmetic is stated per side precisely so the
    // seven non-default combinations are not untested guesses.
}

#[test]
fn more_ingredients_than_lanes_is_a_named_error() { /* advanced-circuit with 1 spine belt */ }

#[test]
fn a_two_product_recipe_is_refused_by_name() { /* uranium-processing */ }

#[test]
fn a_stream_that_cannot_feed_one_machine_is_an_error() {
    // A per-machine rate above one lane's throughput has no column length that
    // works, so it errors rather than producing a zero-machine cell.
}
```

**Constraints:**
- `machines_per_cell = min(floor(cell_cap), 2 * floor(column_cap))`, per the design.
- Lane allocation is exhaustive search maximising `cell_cap`, ties to the earlier
  ingredient in recipe order. Never proportional-with-rounding.
- `size_step` places nothing and touches no `Grid` — it is pure arithmetic over
  the step and the prototypes, so the worked numbers are testable without geometry.
- Rates come from `step.inputs` / `step.outputs` divided by `machines_needed`,
  not recomputed from the recipe — the chain calculator already owns that.

**Verification:** `build-lock cargo test -p factorio-solver cell`

**Commit after passing.** `[Mode: Delegated]`

---

### Task 3: Placing a cell

**Files:**
- Create: `crates/solver/src/layout/place.rs`, `crates/solver/src/layout/place/tests.rs`

**Contracts:**

```rust
pub struct CellExtent { pub width: u32, pub height: u32 }

/// Places one cell — two machine columns, their inserters, and the belts they
/// reach — with its top-left at `origin`. `edge_left` is false when the cell
/// to the left already placed the shared edge belt.
pub fn place_cell(
    grid: &mut Grid,
    step: &ProductionStep,
    plan: &CellPlan,
    cfg: &ResolvedConfig,
    topo: &CellTopology,
    origin: GridPos,
    edge_left: bool,
) -> Result<CellExtent, LayoutError>;
```

Cell geometry, left to right: edge belts (`edge_belts` wide, omitted when
`edge_left` is false), machine column A, spine belts (`spine_belts` wide),
machine column B, edge belts. Belts run vertically (`Direction::South`);
columns run vertically, machine `i` at `origin.y + i * machine_height`.

Inserters sit in the single tile between a machine column and a belt. The belt
adjacent to the column is served by `cfg.inserter`; a belt one further out is
served by `cfg.long_inserter`. One inserter per stream per machine, so a
machine needs `ingredients + products` tiles along its column edges — it has
`machine_height` on each side.

**Test Cases:**

```rust
/// The assertion Phase 6 lacked, and the reason this exists.
#[test]
fn every_output_inserter_claims_the_far_lane_and_no_lane_is_claimed_twice() {
    // For each placed inserter that inserts into a belt, compute drop_lane and
    // assert no (belt cell, lane) pair is claimed by two inserters.
}

#[test]
fn both_lanes_of_a_shared_belt_are_claimed_by_opposite_columns() {
    // Place two adjacent cells; the shared edge belt between them must have
    // both lanes claimed, one per neighbouring column.
}

#[test]
fn a_long_inserter_serves_the_far_belt_of_a_pair() {
    // With spine_belts: 2, the inserter reaching the outer spine belt is
    // cfg.long_inserter and the one reaching the inner is cfg.inserter.
}

#[test]
fn machines_do_not_overlap_and_the_extent_bounds_them() { /* incl. a negative origin */ }

#[test]
fn a_machine_too_narrow_for_its_streams_is_a_named_error() { /* NoRoomForInserters */ }
```

**Constraints:**
- All placement goes through `Grid::place`, so collision detection is never bypassed.
- Inserter orientation is derived from the prototype's `pickup_position` /
  `insert_position` as `rows.rs` does today — that part was verified in-game and
  must survive the rewrite. Reuse the derivation; do not re-derive by hand.
- `Grid::place` takes a Factorio **centre** position, not a top-left.
- Belts run vertically here where `rows.rs` ran them horizontally; nothing else
  in the codebase assumes belt direction.

**Verification:** `build-lock cargo test -p factorio-solver place`

**Commit after passing.** `[Mode: Delegated]`

---

### Task 4: Tiling, and retiring the row topology

**Files:**
- Modify: `crates/solver/src/layout/mod.rs`, `crates/solver/src/testsupport.rs`
- Modify: `crates/solver/src/layout/validate.rs`, `crates/solver/src/layout/validate/tests.rs` — remove `belt_capacity_warnings` and its now-orphaned helpers `lanes_for_item` and `fixing_tier`, plus the four tests covering them
- Delete: `crates/solver/src/layout/lanes.rs`, `rows.rs`, `rows/tests.rs`

The `validate.rs` removal belongs **here**, not in Task 5: `belt_capacity_warnings`
imports `pack_lanes` and `lane_throughput` from the module this task deletes, so
leaving it for later means the crate does not compile at this task's own commit
gate. Task 4 removes; Task 5 adds the replacement check.

**Contracts:**

```rust
pub struct LayoutConfig {
    pub belt_tier: String,
    pub pole: String,
    pub inserter: String,
    pub long_inserter: String,     // new: reaches the outer belt of a pair
    pub topology: CellTopology,    // new
}
```

`generate` sizes each step, then tiles its cells left to right, wrapping to a
new band when `target_width` would be exceeded. Steps stack downward in plan
order separated by `STEP_GAP`, as today. Poles run last, unchanged.

**Test Cases:**

```rust
#[test]
fn the_green_circuit_block_generates_and_is_powered() {
    let grid = generate(&green_circuit_plan(), &default_cfg()).unwrap();
    assert_eq!(grid.entities().filter(|e| e.prototype_name == "assembling-machine-2").count(), 75);
    assert!(coverage_gaps(&grid).is_empty());
}

#[test]
fn target_width_wraps_cells_into_bands() {
    // The same plan with and without a width cap: capped is wider-bounded and
    // taller, uncapped is one band. Machine count identical.
}

#[test]
fn cyclic_and_fluid_refusals_still_fire() { /* unchanged behaviour through the rewrite */ }
```

**Constraints:**
- `long_inserter` defaults to `long-handed-inserter` and is validated by
  `LayoutConfig::resolve` like the others.
- Deleting `lanes.rs`/`rows.rs` must leave no dangling `pub use` in `mod.rs`.

**Verification:** `build-lock cargo test -p factorio-solver`

**Commit after passing.** `[Mode: Delegated]`

---

### Task 5: Delivered-rate validation

**Files:**
- Modify: `crates/solver/src/layout/validate.rs`, `crates/solver/src/layout/validate/tests.rs`, `crates/solver/tests/layout_output.rs`

**Contracts:** `validate` keeps its signature. `belt_capacity_warnings` is
already gone (Task 4) — sizing from the belt makes an over-rating segment
structurally impossible, and its calculation was written in terms of the
departed `sub_rows`. This task adds what replaces it: a hard check that the
block delivers what the plan asked for. `layout_output.rs`'s
`over_capacity_segment_warns_with_the_fixing_tier` goes with it — the condition
it tests can no longer arise.

```rust
/// Sums the lane capacity actually reachable by placed output inserters for
/// each step's product, and fails if it is under the plan's rate.
fn check_delivered_rate(grid: &Grid, plan: &ProductionPlan, cfg: &ResolvedConfig)
    -> Result<(), LayoutError>;
```

New variant: `UnderDelivers { recipe: String, item: String, wanted: f64, delivered: f64 }`.

**Test Cases:**

```rust
/// The test that would have caught #3364: measured from the placed grid, not
/// from the sizing arithmetic that decided the placement.
#[test]
fn the_block_delivers_its_goal_rate() {
    let plan = green_circuit_plan();
    let grid = generate(&plan, &default_cfg()).unwrap();
    validate(&grid, &plan, &default_cfg()).unwrap();
}

#[test]
fn a_deliberately_undersized_grid_is_caught() {
    // Remove machines/inserters from a generated grid, then assert validate
    // reports UnderDelivers. Proves the check measures rather than restates.
}

#[test]
fn generated_blueprint_round_trips() { /* unchanged from Phase 6 */ }
```

**Constraints:**
- Capacity must be counted from the grid — each output inserter contributes its
  belt's lane throughput for the lane it claims, counted once per claimed lane.
  Restating `size_step`'s arithmetic would make the check circular.
- Warnings never block; hard failures always do.

**Verification:** `test-suite` is vitest-only, so: `cargo test --workspace`

**Commit after passing.** `[Mode: Delegated]`

---

### Task 6: Topology controls in the UI

**Files:**
- Modify: `crates/ui/src/chain_panel/layout_controls.rs`, `crates/ui/src/chain_panel/generate.rs`, `crates/ui/src/chain_panel/mod.rs` (new `ChainPanel` fields + `new()` defaults, mirroring `layout_belt`/`layout_pole`/`layout_inserter`), `crates/ui/src/chain_panel/render_tests.rs`

**Contracts:** Controls for `spine_belts` (1–2), `edge_belts` (1–2),
`ingredients_on` (Spine/Edge) and `target_width` (optional), alongside the
existing belt/pole/inserter pickers, plus a `long_inserter` picker sourced the
same way. The generated-block panel reports which stream bound the sizing
(`CellPlan::bound_by`) — that is the number that tells the user which knob to
turn.

**Test Cases:**

```rust
#[test]
fn topology_controls_paint() { /* the four controls appear on a fresh panel */ }

#[test]
fn changing_the_topology_changes_the_block() {
    // Same plan, ingredients_on flipped: different entity count. Proves the
    // control reaches LayoutConfig rather than being decorative.
}

#[test]
fn the_binding_stream_is_shown() { /* "copper-cable" appears in the painted text */ }
```

**Constraints:**
- Reuse the existing viewport and the `take_generated_grid` handoff.
- Long-inserter list: `EntityCategory::Inserter` with a pickup reach of 2+ tiles,
  derived from `pickup_position`, not a name match.

**Verification:** `build-lock cargo test -p factorio-ui`, then generate a block,
paste it into Factorio and **confirm it runs at rate** — the check that found
this bug in the first place.

**Commit after passing.** `[Mode: Delegated]`

---

## Execution
**Skill:** Subagent Dev
- Mode A (1): orchestrator implements directly
- Mode B (2–6): dispatched to subagents

**Ordering:** 1 → 2 → 3 → 4 → 5 → 6. Task 2 is pure arithmetic and Task 3 pure
geometry, so they could overlap, but 3 consumes 2's `CellPlan`.

**Note:** Task 2 is where the design's numbers get pinned and Task 3 is where
the far-lane rule becomes real. Those two carry the correctness of the whole
change; the rest is wiring.
