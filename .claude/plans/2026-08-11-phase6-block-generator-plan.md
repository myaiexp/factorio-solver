# Phase 6: Block Generator Implementation Plan

**Goal:** Turn a `ProductionPlan` into a `Grid` of real entities and out to a blueprint string you can paste into Factorio.

**Architecture:** A new `solver::layout` module. Consumes `ProductionPlan` (Phase 5's pure data output), places machines in belt-fed rows with deterministic geometry — **no A\*** — then hands the `Grid` to the existing `factorio_grid::to_blueprint`. Layout knows nothing about recipes beyond what a step carries.

**Tech Stack:** Rust. **Adds direct `factorio-grid` and `factorio-blueprint` path dependencies to `crates/solver/Cargo.toml`** — today `solver` depends only on `factorio-templates` + `thiserror` and reaches those crates transitively through `templates`' re-export, so bare `factorio_grid::…` paths do not resolve. Making the dependency explicit preserves the graph direction (`solver` is already above `grid`); see the Phase 4 plan's Task 2 constraint.

**Design doc:** `.claude/plans/2026-08-11-chain-calculator-and-block-generator-design.md` — read it first, especially the belt-lane and cyclic-rejection sections.

**Depends on:** Phase 4 (needs `supply_area_distance`, `pickup_position`/`insert_position`, `underground_max_distance`) and Phase 5 (`ProductionPlan`).

---

## File Structure

**Create:**
- `crates/solver/src/testsupport.rs` — shared test fixtures (`green_circuit_plan()`, `default_cfg()`, `blue_belt()`, `plan_containing_kovarex()`, `rate()`). Gated `#[cfg(any(test, feature = "testsupport"))]` and exported, because these are used from **both** inline `#[cfg(test)]` modules and the separate `tests/layout_output.rs` integration crate — which are different compilation units, so an inline-only helper would have to be duplicated.
- `crates/solver/src/layout/mod.rs` — public API, `LayoutConfig`, orchestration
- `crates/solver/src/layout/lanes.rs` — belt-lane maths and ingredient→lane packing
- `crates/solver/src/layout/rows.rs` — machine + inserter placement
- `crates/solver/src/layout/power.rs` — pole placement from `supply_area_distance`
- `crates/solver/src/layout/validate.rs` — pre-emit checks
- `crates/solver/src/layout/error.rs` — `LayoutError`
- `crates/solver/tests/layout_output.rs` — end-to-end + round-trip tests

**Modify:**
- `crates/solver/src/lib.rs` — export `layout`
- `crates/solver/Cargo.toml` — **add `factorio-grid` and `factorio-blueprint` path dependencies** (Task 1; without these every `factorio_grid::…` / `factorio_blueprint::…` path in this plan fails to resolve)
- `crates/ui/src/chain_panel.rs` — add Generate + copy-string (Phase 5 created this file)

---

### Task 1: Types, config, and cyclic rejection

**Files:**
- Create: `crates/solver/src/layout/mod.rs`, `error.rs`
- Modify: `crates/solver/src/lib.rs`

**Contracts:**

```rust
pub struct LayoutConfig {
    pub belt_tier: String,        // e.g. "express-transport-belt"
    pub pole: String,             // e.g. "medium-electric-pole"
    pub inserter: String,         // e.g. "fast-inserter"
}

pub fn generate(plan: &ProductionPlan, cfg: &LayoutConfig) -> Result<Grid, LayoutError>;

pub enum LayoutError {
    CyclicStep { recipe: String, item: String },
    BeltTierUnknown(String),
    NoRoomForPole { step: String },
    Placement(factorio_grid::GridError),
}
```

**Cyclic rejection is a Task 1 requirement, not a later refinement.** A step whose recipe
both consumes and produces the same item (`kovarex-enrichment-process`) has no spatial
realization in the belt-fed-row topology — it needs a self-loop return belt. `generate`
returns `CyclicStep` naming the recipe and item. Phase 5 still computes its ratios correctly;
only the blueprint is refused.

**Test Cases:**

```rust
#[test]
fn cyclic_step_is_rejected_by_name() {
    let plan = plan_containing_kovarex();
    match generate(&plan, &default_cfg()) {
        Err(LayoutError::CyclicStep { recipe, item }) => {
            assert_eq!(recipe, "kovarex-enrichment-process");
            assert_eq!(item, "uranium-235");
        }
        other => panic!("expected CyclicStep, got {other:?}"),
    }
}

#[test]
fn acyclic_plan_is_accepted() { /* green-circuit plan passes the cycle check */ }
```

**Verification:** `build-lock cargo test -p factorio-solver`

**Commit after passing.** `[Mode: Direct]`

---

### Task 2: Belt-lane maths

**Files:**
- Create: `crates/solver/src/layout/lanes.rs`
- Test: inline `#[cfg(test)]`

**Contracts:**

```rust
/// A Factorio belt carries TWO independent lanes; per-lane throughput is half
/// the prototype's belt_throughput.
pub fn lane_throughput(belt: &EntityPrototype) -> f64;

/// ceil(rate / lane_throughput)
pub fn lanes_needed(rate: f64, belt: &EntityPrototype) -> u32;

/// Packs items into belts, at most 2 items per belt (one per lane).
/// Returns one entry per physical belt.
pub fn pack_lanes(items: &[ItemRate], belt: &EntityPrototype) -> Vec<BeltAssignment>;

pub struct BeltAssignment { pub left: Option<String>, pub right: Option<String> }
```

**Test Cases:**

```rust
#[test]
fn per_lane_is_half_the_belt() {
    // express-transport-belt has belt_throughput 45.0
    assert_eq!(lane_throughput(blue_belt()), 22.5);
}

#[test]
fn cable_demand_needs_six_lanes() {
    // The design's centerpiece: 135/s of copper cable on blue belts.
    assert_eq!(lanes_needed(135.0, blue_belt()), 6);
}

#[test]
fn two_ingredients_share_one_belt() {
    // Green circuits: 45/s iron plate + 135/s cable -> plate fits 2 lanes, cable 6.
    let packed = pack_lanes(&[rate("iron-plate", 45.0), rate("copper-cable", 135.0)],
                            blue_belt());
    assert!(packed.iter().any(|b| b.left.is_some() && b.right.is_some()),
            "at least one belt carries two different items");
}

#[test]
fn exact_multiple_does_not_round_up() {
    assert_eq!(lanes_needed(45.0, blue_belt()), 2);   // 45 / 22.5 == 2 exactly
}
```

**Constraints:**
- Never exceed 2 items per belt.
- Use the prototype's `belt_throughput` (populated by Phase 4), never a hardcoded table.

**Verification:** `build-lock cargo test -p factorio-solver`

**Commit after passing.** `[Mode: Delegated]`

---

### Task 3: Machine rows and inserters

**Files:**
- Create: `crates/solver/src/layout/rows.rs`
- Test: inline + `crates/solver/tests/layout_output.rs`

**Contracts:**

```rust
/// Places one step: machines side by side, an inserter per machine on the input
/// and output edges, and the belts those inserters reach.
/// Splits into parallel sub-rows when an item needs more lanes than one belt provides.
pub fn place_step(grid: &mut Grid, step: &ProductionStep, cfg: &LayoutConfig,
                  origin: GridPos) -> Result<StepExtent, LayoutError>;

pub struct StepExtent { pub width: u32, pub height: u32 }
```

Vertical band per sub-row: input belt(s) → inserter row → machine row → inserter row →
output belt(s). Steps stack along one axis in the plan's topological order.

**Test Cases:**

```rust
#[test]
fn machines_do_not_overlap() {
    let grid = generate(&green_circuit_plan(), &default_cfg()).unwrap();
    // Grid::place already rejects collisions; assert the count actually landed.
    let machines = grid.entities().filter(|e| e.prototype_name.starts_with("assembling")).count();
    assert_eq!(machines, 75);   // 30 circuit + 45 cable
}

#[test]
fn every_machine_has_an_adjacent_inserter() { /* each machine has >=1 inserter touching it */ }

#[test]
fn inserters_face_between_machine_and_belt() { /* direction correctness, not just presence */ }

#[test]
fn step_extent_matches_placed_entities() { /* returned extent bounds all placed cells */ }
```

**Constraints:**
- Use `EntityPrototype.pickup_position`/`insert_position` for inserter orientation — real
  prototype data, not assumed offsets.
- Respect `effective_size` for rotated non-square entities.
- All placement goes through `Grid::place` so collision detection is never bypassed.

**Verification:** `build-lock cargo test -p factorio-solver`

**Commit after passing.** `[Mode: Delegated]`

---

### Task 4: Power poles

**Files:**
- Create: `crates/solver/src/layout/power.rs`
- Test: inline

**Contracts:**

```rust
/// Places poles so every machine is within the pole's supply_area_distance.
pub fn place_poles(grid: &mut Grid, cfg: &LayoutConfig) -> Result<(), LayoutError>;

/// Every machine cell is covered by at least one pole's supply area.
pub fn coverage_gaps(grid: &Grid, cfg: &LayoutConfig) -> Vec<GridPos>;
```

**Test Cases:**

```rust
#[test]
fn every_machine_is_powered() {
    let grid = generate(&green_circuit_plan(), &default_cfg()).unwrap();
    assert!(coverage_gaps(&grid, &default_cfg()).is_empty());
}

#[test]
fn pole_reach_comes_from_prototype_data() {
    // medium-electric-pole supply_area_distance is 3.5, small is 2.5 — spacing must
    // differ between the two, proving the value is read, not hardcoded.
}
```

**Constraints:**
- Reach comes from `EntityPrototype.supply_area_distance` (Phase 4), never a constant.
- Poles must not displace machines, belts, or inserters — place into gaps.

**Verification:** `build-lock cargo test -p factorio-solver`

**Commit after passing.** `[Mode: Delegated]`

---

### Task 5: Validation and blueprint output

**Files:**
- Create: `crates/solver/src/layout/validate.rs`
- Test: `crates/solver/tests/layout_output.rs`

**Contracts:**

```rust
pub struct Validation { pub warnings: Vec<String> }

/// Runs before emitting. Hard failures are LayoutError; soft issues are warnings.
pub fn validate(grid: &Grid, plan: &ProductionPlan, cfg: &LayoutConfig)
    -> Result<Validation, LayoutError>;
```

Checks: every machine has an input path and an output path; zero overlapping entities; no
single belt segment over its rating after lane splitting (warning, naming the tier that
fixes it); full pole coverage.

**Test Cases:**

```rust
/// The end-to-end guarantee that the output will actually paste into the game.
#[test]
fn generated_blueprint_round_trips() {
    let grid = generate(&green_circuit_plan(), &default_cfg()).unwrap();
    // NOTE: to_blueprint returns a `Blueprint`, but encode takes `&BlueprintData`.
    // It must be wrapped — a bare Blueprint will not compile.
    let data = BlueprintData {
        blueprint: Some(factorio_grid::to_blueprint(&grid, Some("test".into()), version)),
        blueprint_book: None,
    };
    let s = factorio_blueprint::encode(&data).unwrap();
    let back = factorio_blueprint::decode(&s).unwrap();
    let regrid = factorio_grid::from_blueprint(back.blueprint.as_ref().unwrap());
    assert!(regrid.skipped.is_empty(), "no entity may be dropped on round-trip");
    assert_eq!(regrid.grid.entity_count(), grid.entity_count());
}

#[test]
fn over_capacity_segment_warns_with_the_fixing_tier() { /* names a higher belt tier */ }
```

**Constraints:**
- Round-trip fidelity is the hard gate — a blueprint that loses entities is worthless.
- Warnings never block emission; errors always do.

**Verification:** `test-suite` (full workspace run)

**Commit after passing.** `[Mode: Delegated]`

---

### Task 6: Generate + copy in the UI

**Files:**
- Modify: `crates/ui/src/chain_panel.rs`

**Contracts:** A Generate button on the existing Phase 5 panel that runs `layout::generate`,
loads the resulting grid into the viewport, and shows the blueprint string with a copy
button. Warnings render inline; errors render as their message text.

**Constraints:**
- Reuse the existing viewport rather than adding a second renderer.
- The copy button must place the string on the system clipboard — that is the whole point,
  since pasting it into Factorio is the delivery mechanism.
- Belt tier, pole, and inserter are user-selectable via `LayoutConfig`.

**Verification:** `build-lock cargo test -p factorio-ui`, then the real test: generate a
green-circuit block, copy the string, **paste it into Factorio and confirm it runs**. No unit
test substitutes for this.

**Commit after passing.** `[Mode: Delegated]`

---

## Execution
**Skill:** Subagent Dev
- Mode A (1): orchestrator implements directly
- Mode B (2–6): dispatched to subagents

**Ordering:** 1 → 2 → 3 → 4 → 5 → 6. Tasks 2 and 3 could overlap (different files, and
lane maths is pure), but 3 consumes 2's output so sequential is simpler.

**Note:** the belt-lane packing in Task 2 and the row geometry in Task 3 are the parts most
likely to need iteration against real in-game results. Task 6's paste-into-Factorio check is
what will surface that.
