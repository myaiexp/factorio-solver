# Phase 5: Production Chain Calculator Implementation Plan

**Goal:** Turn a `ChainGoal` ("45/s electronic circuits, I have plates on the bus") into a `ProductionPlan` listing which recipes to run, on which machines, and how many.

**Architecture:** A new `solver::chain` module. Pure computation over Phase 4's recipe registry and prototype registry — no geometry, no I/O. `ProductionPlan` is a plain data struct and the sole interface to Phase 6, so the calculator is fully testable without any layout code existing.

**Tech Stack:** Rust, `factorio-solver` crate. Adds a direct `factorio-grid` path dependency (see File Structure) — no external crates.

**Design doc:** `.claude/plans/2026-08-11-chain-calculator-and-block-generator-design.md` — read it first.

**Depends on:** Phase 4 (`.claude/plans/2026-08-11-game-data-foundation-plan.md`) must be landed — this needs `Recipe` (with `main_product`) and `EntityPrototype.crafting_categories`.

---

## File Structure

**Create:**
- `crates/solver/src/chain/mod.rs` — public API, `ChainGoal`/`ProductionPlan` types
- `crates/solver/src/chain/select.rs` — recipe and machine selection
- `crates/solver/src/chain/solve.rs` — the rate solver
- `crates/solver/src/chain/error.rs` — `ChainError`
- `crates/solver/tests/chain_ratios.rs` — hand-checkable ratio tests

**Modify:**
- `crates/solver/src/lib.rs` — export `chain`
- `crates/solver/Cargo.toml` — add a direct `factorio-grid` path dependency. `ProductionStep.machine` is a `&'static EntityPrototype`, and `solver` currently reaches `grid` only through `templates`' re-export. See the Phase 4 plan's Task 2 constraint.

Split into four files because selection (graph/policy logic) and solving (numeric) are
independently testable concerns, and `mod.rs` staying type-only keeps the public surface
readable.

---

### Task 1: Types and errors

**Files:**
- Create: `crates/solver/src/chain/mod.rs`, `crates/solver/src/chain/error.rs`
- Modify: `crates/solver/src/lib.rs`

**Contracts:**

```rust
pub enum Rate { ItemsPerSec(f64), ItemsPerMin(f64), Belts { count: u32, tier: String } }

pub enum MachineFallback { FastestAvailable, Named(String) }

pub struct MachinePolicy {
    pub preferred: HashMap<String, String>,   // crafting category -> machine name
    pub fallback: MachineFallback,
}

pub struct ChainGoal {
    pub product: String,
    pub rate: Rate,
    pub available: HashSet<String>,
    pub machines: MachinePolicy,
    pub recipe_overrides: HashMap<String, String>,
}

pub struct ItemRate { pub item: String, pub per_sec: f64 }

pub struct ProductionStep {
    pub recipe: &'static Recipe,
    pub machine: &'static EntityPrototype,
    pub exact_count: f64,
    pub machines_needed: u32,     // exact_count.ceil()
    pub crafts_per_sec: f64,
    pub inputs: Vec<ItemRate>,
    pub outputs: Vec<ItemRate>,
}

pub struct ProductionPlan {
    pub steps: Vec<ProductionStep>,   // topologically ordered, producers first
    pub inputs: Vec<ItemRate>,
    pub byproducts: Vec<ItemRate>,
    pub warnings: Vec<String>,
}

pub enum ChainError {
    UnknownItem(String),
    UnknownBeltTier(String),
    FluidIngredient { recipe: String, fluid: String },
    AmbiguousRecipe { item: String, candidates: Vec<String> },
    NoMachineForCategory { category: String, recipe: String },
    UnreachableBoundary { item: String },
}
```

`Rate::Belts` resolves via the belt prototype's `belt_throughput`, so conversion is
fallible: `fn per_sec(&self) -> Result<f64, ChainError>`, returning `UnknownBeltTier` for a
name that isn't a belt prototype. Not a panic and not a silent default — same
never-guess rule as recipe selection.

**Test Cases:**

```rust
#[test]
fn rate_conversions() {
    assert_eq!(Rate::ItemsPerMin(60.0).per_sec().unwrap(), 1.0);
    // "2 blue belts" == 90/s, from express-transport-belt's belt_throughput of 45
    assert_eq!(
        Rate::Belts { count: 2, tier: "express-transport-belt".into() }.per_sec().unwrap(),
        90.0);
}

#[test]
fn unknown_belt_tier_errors_rather_than_defaulting() {
    assert!(matches!(
        Rate::Belts { count: 1, tier: "not-a-belt".into() }.per_sec(),
        Err(ChainError::UnknownBeltTier(_))));
}

#[test]
fn errors_display_actionably() {
    // FluidIngredient must name BOTH the recipe and the fluid, and state the fix.
    let e = ChainError::FluidIngredient {
        recipe: "plastic-bar".into(), fluid: "petroleum-gas".into() };
    let s = e.to_string();
    assert!(s.contains("plastic-bar") && s.contains("petroleum-gas"));
}
```

**Constraints:**
- `ChainError` implements `std::error::Error` via `thiserror` (already a `solver` dependency).
- Error messages must name the offending item AND the remedy — a silently wrong blueprint costs real in-game time.

**Verification:** `build-lock cargo test -p factorio-solver`

**Commit after passing.** `[Mode: Direct]`

---

### Task 2: Recipe and machine selection

**Files:**
- Create: `crates/solver/src/chain/select.rs`
- Test: inline `#[cfg(test)]`

**Contracts:**

```rust
/// Candidates = recipes whose `results` contain `item`, minus hidden, minus
/// category == "recycling", minus multi-result recipes that don't declare
/// main_product == item.
pub fn candidates_for(item: &str) -> Vec<&'static Recipe>;

/// 1. recipe_overrides  2. exactly one candidate  3. else AmbiguousRecipe
pub fn select_recipe(item: &str, overrides: &HashMap<String, String>)
    -> Result<&'static Recipe, ChainError>;

pub fn select_machine(recipe: &Recipe, policy: &MachinePolicy)
    -> Result<&'static EntityPrototype, ChainError>;
```

**`main_product` is declared on only 13 of 659 recipes — do not build selection on it.**
The load-bearing rule is that 420 recipes have exactly one result, which trivially
identifies their product.

**Test Cases:**

```rust
#[test]
fn single_result_recipe_resolves_without_override() {
    let r = select_recipe("electronic-circuit", &HashMap::new()).unwrap();
    assert_eq!(r.name, "electronic-circuit");
}

#[test]
fn copper_cable_is_ambiguous_and_errors_with_candidates() {
    // Two survivors after filtering recycling: copper-cable, casting-copper-cable.
    match select_recipe("copper-cable", &HashMap::new()) {
        Err(ChainError::AmbiguousRecipe { candidates, .. }) => {
            assert!(candidates.len() >= 2);
            assert!(candidates.iter().any(|c| c == "copper-cable"));
            assert!(candidates.iter().any(|c| c == "casting-copper-cable"));
        }
        other => panic!("expected AmbiguousRecipe, got {other:?}"),
    }
}

#[test]
fn override_resolves_ambiguity() {
    let mut o = HashMap::new();
    o.insert("copper-cable".to_string(), "copper-cable".to_string());
    assert_eq!(select_recipe("copper-cable", &o).unwrap().name, "copper-cable");
}

#[test]
fn recycling_recipes_are_never_candidates() {
    // 310 of 649 recipes are category == "recycling"; none may be auto-selected.
    assert!(candidates_for("iron-plate").iter().all(|r| r.category != "recycling"));
}

#[test]
fn machine_selection_matches_crafting_category() {
    let recipe = factorio_solver::recipe::get("electronic-circuit").unwrap();
    let m = select_machine(recipe, &MachinePolicy::fastest()).unwrap();
    assert!(m.crafting_categories.iter().any(|c| c == "electronics"));
}

#[test]
fn no_machine_for_category_errors() { /* NoMachineForCategory names category + recipe */ }
```

**Constraints:**
- Never guess between candidates. Ambiguity is an error, always.
- `candidates_for` must be pure and cheap — it runs once per item during resolution.

**Verification:** `build-lock cargo test -p factorio-solver`

**Commit after passing.** `[Mode: Delegated]`

---

### Task 3: The rate solver

**Files:**
- Create: `crates/solver/src/chain/solve.rs`
- Test: inline `#[cfg(test)]` + `crates/solver/tests/chain_ratios.rs`

**Contracts:**

```rust
pub fn solve(goal: &ChainGoal) -> Result<ProductionPlan, ChainError>;
```

**Requirement, not implementation:** the solver must produce correct rates for
**multi-output** and **cyclic** recipes. A tree recursion cannot express either and must not
be used. The numerical method is the implementer's choice — the tests below are the spec.

Resolution stops at any item in `goal.available` (it becomes a `ProductionPlan.inputs`
entry) and at items with no candidate recipe (raw resources). A recipe whose ingredients
include a fluid that is not in `available` → `FluidIngredient`.

**Test Cases:**

```rust
/// The canonical hand-checkable ratio. electronic-circuit and copper-cable both have
/// energy_required absent -> 0.5s. assembling-machine-2 crafting_speed 0.75.
/// circuit: 0.75/0.5 = 1.5 crafts/s -> 45/1.5 = 30 machines
/// cable demand: 45*3 = 135/s; cable yields 2 per craft -> 3/s -> 135/3 = 45 machines
#[test]
fn green_circuits_from_plates_is_30_and_45() {
    let plan = solve(&ChainGoal {
        product: "electronic-circuit".into(),
        rate: Rate::ItemsPerSec(45.0),
        available: ["iron-plate", "copper-plate"].into_iter().map(String::from).collect(),
        machines: MachinePolicy::all("assembling-machine-2"),
        recipe_overrides: HashMap::new(),
    }).unwrap();

    let circuits = plan.steps.iter().find(|s| s.recipe.name == "electronic-circuit").unwrap();
    let cable    = plan.steps.iter().find(|s| s.recipe.name == "copper-cable").unwrap();
    assert_eq!(circuits.machines_needed, 30);
    assert_eq!(cable.machines_needed, 45);
}

/// Same goal, narrower boundary -> one step instead of two, same code path.
#[test]
fn declaring_cable_available_removes_its_step() {
    let plan = solve(&goal_with_available(&["iron-plate", "copper-cable"])).unwrap();
    assert_eq!(plan.steps.len(), 1);
    assert!(plan.inputs.iter().any(|i| i.item == "copper-cable"));
}

/// Producers must precede consumers so the layout phase can walk steps in order.
#[test]
fn steps_are_topologically_ordered() {
    let plan = solve(&green_circuit_goal()).unwrap();
    let cable_ix = plan.steps.iter().position(|s| s.recipe.name == "copper-cable").unwrap();
    let circ_ix  = plan.steps.iter().position(|s| s.recipe.name == "electronic-circuit").unwrap();
    assert!(cable_ix < circ_ix);
}

/// Multi-output, fluid-free. CRITICAL: both results declare `amount: 1`; the entire
/// split lives in `probability` (0.007 U-235, 0.993 U-238). Effective yield per craft
/// is `amount * probability`. Using `amount` alone silently gives a 1:1 ratio.
/// 103 recipes carry `probability` — this is not a special case.
#[test]
fn uranium_processing_uses_probability_weighted_yield() {
    let plan = solve(&goal_for_rate("uranium-235", 1.0, &["uranium-ore"])).unwrap();
    let step = plan.steps.iter().find(|s| s.recipe.name == "uranium-processing").unwrap();
    // 1/s of U-235 at 0.007 per craft needs ~142.86 crafts/s, NOT 1.
    assert!((step.crafts_per_sec - 1.0 / 0.007).abs() < 1e-6);
    // The U-238 byproduct comes out at 0.993/0.007 times the U-235 rate.
    let u238 = plan.byproducts.iter().find(|b| b.item == "uranium-238").unwrap();
    assert!((u238.per_sec - 0.993 / 0.007).abs() < 1e-6);
}

/// Cyclic, fluid-free: consumes 40 U-235 + 5 U-238, yields 41 U-235 + 2 U-238.
/// Must terminate and net +1 U-235 / -3 U-238 per batch.
#[test]
fn kovarex_cycle_terminates_and_balances() { /* no infinite loop, correct net rates */ }

#[test]
fn fluid_ingredient_is_rejected_with_named_error() {
    let e = solve(&goal_for("plastic-bar", &["coal"])).unwrap_err();
    assert!(matches!(e, ChainError::FluidIngredient { ref fluid, .. } if fluid == "petroleum-gas"));
}

#[test]
fn declaring_the_fluid_product_available_succeeds() {
    // advanced-circuit with plastic-bar on the bus needs no fluid at all.
    assert!(solve(&goal_for("advanced-circuit",
        &["plastic-bar", "electronic-circuit", "copper-cable"])).is_ok());
}

#[test]
fn byproducts_are_reported() { /* multi-output surplus lands in plan.byproducts */ }
```

**Constraints:**
- Must terminate on cyclic recipes — no unbounded recursion. A cycle-detection or
  convergence bound is required, and exceeding it is an error, never a silent partial result.
- `exact_count` keeps the fractional value; `machines_needed` is its `ceil`. Both are needed —
  the fraction is the true ratio, the ceil is what you build.
- **All yield maths uses `amount × probability`, never `amount` alone.** 103 recipes carry a
  `probability`, and for `uranium-processing` it is the *only* thing distinguishing the two
  outputs.
- No geometry, no `Grid`, no I/O in this module.

**Verification:** `build-lock cargo test -p factorio-solver`

**Commit after passing.** `[Mode: Delegated]`

---

### Task 4: Surface the plan in the UI

**Files:**
- Create: `crates/ui/src/chain_panel.rs`
- Modify: `crates/ui/src/app.rs` (mount the panel), `crates/ui/Cargo.toml` if needed

**Contracts:** A side panel with product entry, rate entry + unit selector, an editable
"available on the bus" list, and a machine-tier selector. On solve, render the steps as a
table (recipe display name, machine, count) plus inputs and byproducts. Errors render as the
message text — they are written to be user-facing.

**Constraints:**
- **The recipe picker must filter `category == "recycling"` and `hidden` by default** — 310 of
  649 recipes are recycling and make the picker unusable otherwise. Provide a toggle.
- Use `display_name` from Phase 4, not the internal name.
- `app.rs` is already 519 lines; the panel lives in its own module, not inline.
- No blueprint output yet — that is Phase 6.

**Verification:** `build-lock cargo test -p factorio-ui`, then run against a headless
compositor on the desktop machine and screenshot with `grim` to confirm the panel renders and
a green-circuit goal shows 30 + 45.

**Commit after passing.** `[Mode: Delegated]`

---

## Execution
**Skill:** Subagent Dev
- Mode A (1): orchestrator implements directly
- Mode B (2, 3, 4): dispatched to subagents

**Ordering:** 1 → 2 → 3 → 4, strictly sequential (each consumes the previous).
