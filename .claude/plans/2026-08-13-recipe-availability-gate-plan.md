# Recipe Availability Gate Implementation Plan

**Goal:** Gate recipe and machine selection on a set of unlocked recipes, editable in the UI and persisted across runs, so the solver stops proposing recipes and machines the player cannot build.

**Architecture:** A source-agnostic `Availability` model in the solver holds a recipe-name set; `chain::select` grows a gated companion to `candidates_for` while the ungated one stays, because `solve` needs to tell "nothing produces this" (a raw resource) apart from "nothing you can build produces this" (an error). The technology graph is ingested for explanation only — it turns a refusal into "this needs `foundry`". The save-file import of #3382 becomes a second source later, replacing the set wholesale.

**Tech Stack:** Rust workspace, `serde`/`serde_json`, `OnceLock` registries, egui/eframe 0.33 (with the non-default `persistence` feature), `thiserror`.

**Design doc:** `.claude/plans/2026-08-13-recipe-availability-gate-design.md`

---

### Task 1: Technology data — schema, registry, ingestion

**Files:**
- Create: `crates/solver/src/tech.rs`
- Create: `crates/dump-ingest/src/technologies.rs`
- Create: `crates/solver/data/technologies.json` (generated — see Constraints)
- Modify: `crates/solver/src/lib.rs` (declare `tech`)
- Modify: `crates/dump-ingest/src/main.rs` (module, `--out-technologies` arg, write + count)
- Modify: `crates/dump-ingest/src/error.rs` (technology error variants)

**Contracts:**

```rust
// crates/solver/src/tech.rs — schema shared with dump-ingest, which
// serialises this very struct rather than a hand-written JSON shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Technology {
    pub name: String,
    pub display_name: Option<String>,
    pub prerequisites: Vec<String>,
    /// Recipe names from `effects[]` entries of type `unlock-recipe`.
    pub unlocks: Vec<String>,
}

pub fn registry() -> &'static HashMap<String, Technology>;
pub fn get(name: &str) -> Option<&'static Technology>;

/// Names of technologies whose `unlocks` contain `recipe`, sorted.
/// Backed by a reverse index built once beside the registry — this is
/// called per error, never per candidate, but a linear scan of 275
/// technologies per call is pointless when the index is free.
pub fn unlockers_for(recipe: &str) -> Vec<&'static str>;
```

```rust
// crates/dump-ingest/src/technologies.rs
pub fn to_technology(
    name: &str,
    raw: &Value,
    locale: &Locale,
    known_recipes: &HashSet<String>,
) -> Result<Technology, IngestError>;
```

**Test Cases:**

```rust
// dump-ingest — fixture Values, no real dump needed
#[test]
fn unlock_recipe_effects_become_unlocks() {
    // effects: [{type: "unlock-recipe", recipe: "casting-iron"}]
    // → unlocks == ["casting-iron"]
}

#[test]
fn non_unlock_effects_are_ignored() {
    // effects: [{type: "mining-drill-productivity-bonus", modifier: 0.1}]
    // → unlocks is empty, and this is NOT an error: 112 of 275 technologies
    //   unlock nothing and must still be ingested for their prerequisite edges
}

#[test]
fn an_unlock_naming_an_unknown_recipe_is_dropped_not_an_error() {
    // known_recipes lacks "parameter-0"; the recipe filter already discards
    // `parameter: true` placeholders by design, so the edge is legitimately
    // dangling → unlocks omits it, no error
}

#[test]
fn a_prerequisite_naming_an_unknown_technology_is_an_error() {
    // a broken graph would under-report silently later
}

#[test]
fn display_name_comes_from_the_locale() {
    // "automation-3" → "Automation 3"
}

// solver — against the placeholder file, so this task can pass on its own
#[test]
fn the_registry_loads_and_indexes_unlockers() {
    // Shape only. The real-data assertions (275 technologies, `foundry`
    // unlocking the four casting-* recipes) belong to Task 6, which is what
    // regenerates the file from a dump.
    let _ = tech::registry();
    assert!(tech::unlockers_for("no-such-recipe").is_empty());
}
```

**Constraints:**
- Match `recipes.rs` conventions exactly: `defaulted()` for optional fields, a hard error for a present-but-wrong-typed field, `IngestError` variants naming the technology. A silent partial write from a rarely-run tool poisons every downstream phase.
- Ingest **all** technologies including the 112 bonus-only ones — prerequisite chains run through them.
- `registry()` loads via `OnceLock` + `include_str!("../data/technologies.json")`, mirroring `recipe::registry()`.
- **Data generation is an orchestrator step, not the implementer's.** The dump lives only on the desktop (`~/.factorio/script-output/data-raw-dump.json`, 27 MB), so a delegated agent on the VPS cannot produce the real file. Commit a **minimal valid placeholder** `technologies.json` — a small hand-written array of two or three real entries — so `include_str!` resolves, the crate compiles and the tests run. Task 6 replaces it with the generated file. This mirrors what `.claude/plans/2026-08-11-game-data-foundation-plan.md` Task 2 did for `recipes.json`.
- Consequently this task's tests assert *shape*, never dump-derived content: no counts, no "`foundry` unlocks `casting-iron`". Those assertions live in Task 6, which is the first point they can be true.

**Verification:**
Run: `build-lock cargo test -p factorio-dump-ingest && build-lock cargo test -p factorio-solver tech`
Expected: all pass against the placeholder file.

**Commit after passing.** `[Mode: Delegated]`

---

### Task 2: The availability model

**Files:**
- Create: `crates/solver/src/availability.rs`
- Modify: `crates/solver/src/lib.rs` (declare `availability`)

**Contracts:**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// Default — today's behaviour, minus the never-unlockable recipes.
    Everything,
    /// Recipe names known to be unlocked, from any source.
    Unlocked(BTreeSet<String>),
}

impl Availability {
    pub fn allows(&self, recipe: &Recipe) -> bool;
    /// A machine prototype is available when no recipe produces its item at
    /// all, or when at least one recipe producing it is allowed.
    pub fn allows_machine(&self, machine: &str) -> bool;
}

/// Every recipe name available under `Everything` — the seed the UI uses when
/// switching modes, so the switch alone changes no result.
pub fn all_available_recipe_names() -> BTreeSet<String>;
```

**Test Cases:**

```rust
#[test]
fn enabled_recipes_are_available_under_an_empty_unlocked_set() {
    // iron-plate is `enabled: true` — no source should have to enumerate the
    // 323 starting recipes, and a save import's set is a superset anyway
    let a = Availability::Unlocked(BTreeSet::new());
    assert!(a.allows(recipe::get("iron-plate").unwrap()));
}

#[test]
fn a_locked_recipe_needs_to_be_in_the_set() {
    let empty = Availability::Unlocked(BTreeSet::new());
    assert!(!empty.allows(recipe::get("casting-iron").unwrap()));
    let with = Availability::Unlocked(["casting-iron".into()].into());
    assert!(with.allows(recipe::get("casting-iron").unwrap()));
}

#[test]
fn never_unlockable_recipes_are_excluded_under_everything() {
    // loader/pistol/infinity-chest: `enabled: false` with no unlocking tech,
    // so no playthrough reaches them. They are candidates today — this is a
    // live bug fix, deliberately changing the default's behaviour.
    for name in ["loader", "fast-loader", "express-loader", "turbo-loader",
                 "infinity-chest", "infinity-pipe", "heat-interface", "pistol"] {
        assert!(!Availability::Everything.allows(recipe::get(name).unwrap()), "{name}");
    }
}

#[test]
fn an_explicit_set_outranks_the_never_unlockable_table() {
    // With a real source the source is authoritative; the static table only
    // fills in for `Everything`, where there is nothing to consult.
    let a = Availability::Unlocked(["loader".into()].into());
    assert!(a.allows(recipe::get("loader").unwrap()));
}

#[test]
fn machine_availability_follows_its_own_recipe() {
    let empty = Availability::Unlocked(BTreeSet::new());
    assert!(!empty.allows_machine("electromagnetic-plant"));
    let with = Availability::Unlocked(["electromagnetic-plant".into()].into());
    assert!(with.allows_machine("electromagnetic-plant"));
}

#[test]
fn a_machine_no_recipe_produces_stays_available() {
    // missing data is not evidence of a lock
}
```

**Constraints:**
- `allows_machine` scans the **raw registry** for recipes whose results name the machine's item — not `candidates_for`, whose hidden/recycling/main-product filters answer a different question ("which recipe should make this inside a chain").
- No caching keyed on the set: `Availability` is cheap to compare and the scans run once per item per solve.

**Verification:**
Run: `build-lock cargo test -p factorio-solver availability`
Expected: all pass.

**Commit after passing.** `[Mode: Delegated]`

---

### Task 3: Gate selection and solving

**Files:**
- Modify: `crates/solver/src/chain/select.rs`
- Modify: `crates/solver/src/chain/solve.rs`
- Modify: `crates/solver/src/chain/error.rs`
- Modify: `crates/solver/src/chain/mod.rs` (`ChainGoal.availability`, `with_availability`)
- Modify: `crates/ui/src/chain_panel/mod.rs` (`build_goal` passes it through)

**Contracts:**

```rust
// select.rs — the old function keeps its signature and its meaning
pub fn candidates_for(item: &str) -> Vec<&'static Recipe>;

/// The subset of `candidates_for` that `availability` allows.
pub fn available_candidates_for(item: &str, availability: &Availability) -> Vec<&'static Recipe>;

pub fn select_recipe(
    item: &str,
    overrides: &HashMap<String, String>,
    availability: &Availability,
) -> Result<&'static Recipe, ChainError>;

pub fn select_machine(
    recipe: &Recipe,
    policy: &MachinePolicy,
    availability: &Availability,
) -> Result<&'static EntityPrototype, ChainError>;
```

```rust
// error.rs
#[error("'{item}' cannot be built yet — it needs {} — research it, or tick it \
         in the available-recipes list", .unlocked_by.join(" or "))]
NotUnlocked { item: String, unlocked_by: Vec<String> },

#[error("machine '{machine}' cannot be built yet — it needs {}", .unlocked_by.join(" or "))]
MachineNotUnlocked { machine: String, unlocked_by: Vec<String> },
```

**Test Cases:**

```rust
#[test]
fn a_locked_intermediate_errors_instead_of_becoming_a_bus_input() {
    // THE test this split exists for. Demand a locked item as an
    // intermediate, not as the goal. solve.rs:91 treats an empty candidate
    // set as "raw resource → bus input"; if that call were gated, this plan
    // would succeed and quietly ask the player to bus a Vulcanus product.
    // A passing plan here is the bug.
    let goal = /* chemical-science-pack, casting recipes locked, a step whose
                  only producer is locked */;
    assert!(matches!(solve(&goal), Err(ChainError::NotUnlocked { .. })));
}

#[test]
fn a_genuinely_raw_item_still_folds_into_the_bus() {
    // iron-ore under a restrictive set: nothing produces it under ANY
    // availability, so it is a bus input, not NotUnlocked
}

#[test]
fn the_locked_goal_names_its_technology() {
    // goal = casting-iron's product with casting locked
    // → NotUnlocked { unlocked_by: ["foundry"] }
}

#[test]
fn an_item_nothing_ever_produces_is_still_unreachable_boundary() {
    // the ungated-empty case at solve.rs:48 keeps its existing error
}

#[test]
fn locking_the_casting_recipes_removes_the_ambiguity() {
    // The headline case. iron-plate, copper-cable, iron-gear-wheel and pipe
    // each drop to exactly one candidate.
    assert_eq!(available_candidates_for("iron-plate", &avail).len(), 1);
}

#[test]
fn the_chemical_science_plan_solves_with_no_overrides() {
    // Currently needs three (copper-cable, iron-gear-wheel, pipe) — see
    // crates/ui/src/chain_panel/scroll_tests.rs. With the casting recipes
    // locked it must solve with `recipe_overrides` empty.
}

#[test]
fn fastest_available_skips_locked_machines() {
    // electromagnetic-plant locked → the chemical-science plan assigns none
}

#[test]
fn a_named_locked_machine_errors_rather_than_substituting() {
    // MachinePolicy::all("electromagnetic-plant") with it locked
    // → MachineNotUnlocked, not a silent fallback to an assembler
}

#[test]
fn only_locked_machines_for_a_category_names_the_lock() {
    // not NoMachineForCategory, whose message points at the machine policy
    // when the actual remedy is research
}
```

**Constraints:**
- `solve.rs:91` (the raw-resource test) **must keep calling the ungated `candidates_for`**. This is the single most important line in the task; getting it wrong produces plans that look successful and are wrong.
- `solve.rs:48` (goal check) takes the two-step: ungated-empty → `UnreachableBoundary`; ungated non-empty, gated empty → `NotUnlocked`.
- `unlocked_by` comes from `tech::unlockers_for`, sorted, and may be empty (the never-unlockable eight) — the message must still read sensibly then.
- Existing test `research_locked_recipes_are_still_candidates` keeps its **assertion** under `Everything`; its call site gains the new argument. Same for every other `candidates_for`/`select_recipe`/`select_machine` caller in `select.rs`'s and `solve.rs`'s test modules.
- `ChainGoal::availability` defaults to `Everything`, so every existing caller and test is behaviour-identical apart from the eight never-unlockable recipes.

**Verification:**
Run: `build-lock cargo test -p factorio-solver && build-lock cargo test --workspace`
Expected: all pass, including the pre-existing solver and UI suites.

**Commit after passing.** `[Mode: Delegated]`

---

### Task 4: The availability section in the chain panel

**Files:**
- Create: `crates/ui/src/chain_panel/availability_controls.rs`
- Modify: `crates/ui/src/chain_panel/mod.rs` (state fields, `ui()` wiring, `build_goal`)
- Modify: `crates/ui/src/chain_panel/controls.rs` (product picker filtered by availability)
- Create: `crates/ui/src/chain_panel/availability_tests.rs`

**Contracts:**

```rust
// chain_panel/mod.rs — new state
pub(super) enum AvailabilityMode { Everything, OnlyAvailable }
// on ChainPanel:
//   availability_mode: AvailabilityMode,
//   available_recipes: BTreeSet<String>,
//   available_query: String,
//
// NOT `available` — `ChainPanel::available: Vec<String>` already exists and
// holds the *bus* contents (mod.rs:56), read by `build_goal` and
// `controls::available_list`. Reusing the name would collide outright.

// availability_controls.rs
pub(super) fn show(panel: &mut ChainPanel, ui: &mut egui::Ui);
```

**Test Cases:**

```rust
#[test]
fn switching_to_only_available_seeds_the_set_from_everything() {
    // the switch alone must change no solve result — editing is subtractive
    // because the user's complaint is subtractive
}

#[test]
fn untick_all_matching_removes_exactly_the_searched_recipes() {
    // query "casting" → the four casting-* recipes leave the set, nothing else
}

#[test]
fn the_section_paints_its_controls_and_count() {
    // via the existing harness::painted_text: mode labels, search box, and a
    // count line like "412 of 649 available"
}

#[test]
fn build_goal_carries_the_availability() {
    // Everything → ChainGoal::availability == Availability::Everything
    // OnlyAvailable → Unlocked(set)
}

#[test]
fn the_product_picker_hides_unavailable_recipes() {
    // an unbuildable product should not be offered as a goal in the first place
}
```

**Constraints:**
- Seed on mode switch comes from `availability::all_available_recipe_names()` — not a UI-side reimplementation of the same rule.
- Search matches display name or internal name, case-insensitive, reusing `logic::matches_query`'s behaviour rather than a second implementation.
- The list is bounded by its own `ScrollArea` with an `id_salt`, like the product picker; the panel body is already scrollable, so this must not fight it.
- `crates/ui/src/chain_panel/mod.rs` is at 260 lines against the project's 300 cap — put the section's logic in `availability_controls.rs` and its tests in `availability_tests.rs`, following how `render_tests`/`topology_tests`/`harness` already split.

**Verification:**
Run: `build-lock cargo test -p factorio-ui`
Expected: all pass.

**Commit after passing.** `[Mode: Delegated]`

---

### Task 5: Persistence

**Files:**
- Modify: `crates/ui/Cargo.toml` — `eframe = { version = "0.33", features = ["persistence"] }` **and** an explicit `serde = { version = "1", features = ["derive"] }`. eframe's feature pulls serde in for eframe's own use; a crate deriving `Serialize` must still declare it, and `factorio-ui` currently has no serde line at all (every other workspace crate does).
- Create: `crates/ui/src/persist.rs`
- Modify: `crates/ui/src/app.rs` — `App::save`, and `FactorioApp::new(cc: &eframe::CreationContext<'_>)` so it can read `cc.storage`. The signature change is the point of the task; it takes no parameters today.
- Modify: `crates/ui/src/main.rs` — the call site, currently `Box::new(|_cc| Ok(Box::new(FactorioApp::new())))` (main.rs:24), which discards the `CreationContext` and will not compile once `new` takes it.
- Modify: `crates/ui/src/chain_panel/mod.rs` (expose state for capture/apply)
- Modify: `crates/solver/src/layout/cell.rs` — derive `Serialize`/`Deserialize` on **both** `CellTopology` (cell.rs:22) and `Side` (cell.rs:15), since `CellTopology.ingredients_on: Side`. `layout/mod.rs` only re-exports them, so the derives cannot go there.

**Contracts:**

```rust
// persist.rs
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]     // every field independently optional — see Constraints
pub struct PanelState { /* product, rate_value, rate_unit, belt_tier,
                           available (bus), machine, show_hidden_recycling,
                           overrides, layout_*, topology,
                           availability_mode, available_recipes */ }

impl PanelState {
    pub fn capture(panel: &ChainPanel) -> Self;
    pub fn apply(self, panel: &mut ChainPanel);
}

const STORAGE_KEY: &str = "factorio_solver_panel";
```

**Test Cases:**

```rust
#[test]
fn a_captured_panel_round_trips_through_serde() {
    // capture → to_string → from_str → apply → fields match
}

#[test]
fn a_blob_missing_fields_restores_those_fields_to_defaults() {
    // `{"product":"iron-plate"}` must restore the product and default the
    // rest, not discard the whole blob — adding a field later cannot wipe a
    // saved setup
}

#[test]
fn an_unknown_field_in_the_blob_is_ignored() {
    // a field removed in a later version must not fail the restore
}

#[test]
fn recipe_names_that_no_longer_exist_are_dropped_on_apply() {
    // a game update can retire recipes; the set must not carry ghosts
}

#[test]
fn derived_state_is_not_persisted() {
    // result / generated / pending_grid absent from PanelState — restoring a
    // plan that no longer matches the inputs beside it would be a lie
}
```

**Constraints:**
- `#[serde(default)]` at container level plus `Default` on every field — leniency is the requirement, not a nicety.
- Persist only inputs. `result`, `generated`, `pending_grid` are derived.
- Tested at the serde layer, not through eframe's storage, which needs a real app lifecycle.
- eframe's `persistence` feature pulls `ron` and `serde` — confirm the workspace still builds with `--workspace` after the feature flip.

**Verification:**
Run: `build-lock cargo test --workspace && build-lock cargo build --release -p factorio-ui`
Expected: all pass; release build clean.

**Commit after passing.** `[Mode: Delegated]`

---

### Task 6: Regenerate data and verify in the app

**Files:**
- Modify: `crates/solver/data/technologies.json` (generated)
- Modify: `CLAUDE.md` (data-file list, decisions from this phase)
- Create: `phases/008-recipe-availability-gate.md`
- Modify: `phases/current.md`

**Constraints:**
- Copy `~/.factorio/script-output/data-raw-dump.json` from the desktop, run `dump-ingest` with all four `--out-*` paths, confirm all three data files regenerate with unchanged counts for the existing two (169 prototypes, 649 recipes) and ~275 technologies.
- Then run the app on the desktop and drive the reported case by hand: chemical science pack, five-item bus, switch to "only what I can build", search `casting`, untick all matching, Solve. Expect no ambiguity prompts and no electromagnetic plants in the steps table. Close and relaunch; expect the setup to still be there.
- Close idea #3356; note #3380 as folded in.

**Verification:**
Run: `build-lock cargo test --workspace`, then the manual pass above.
Expected: green suite; the reported symptom gone; state survives a restart.

**Commit after passing.** `[Mode: Direct]`

---
## Execution
**Skill:** Subagent Dev (if included in your instructions)
- Mode A tasks: orchestrator implements directly
- Mode B tasks: Dispatched to subagents
