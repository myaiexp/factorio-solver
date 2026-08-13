# Save-Derived Machine Availability Implementation Plan

**Goal:** Read the player's unlocked recipes from a Factorio save file so the solver only proposes recipes and machines they can actually build.

**Architecture:** A new dependency-free `factorio-save` crate decodes the save zip — parsing the uncompressed `level-init.dat` for prototype ID tables, then lazily inflating only the leading `level.datN` chunks needed to reach the player force's recipe-unlock array, which it locates by searching for the alignment that satisfies a zero-violation invariant. `solver` supplies that invariant from its own recipe registry and exposes the result as an `Availability` carried on `ChainGoal`; `select_recipe` and `select_machine` filter against it. The UI gains a save dropdown sorted newest-first.

**Tech Stack:** Rust (edition 2024), `zip`, `flate2`, egui.

**Design doc:** `.claude/plans/2026-08-13-save-derived-availability-design.md`

---

## File Structure

```
crates/save/                      NEW — no workspace-crate dependencies
  Cargo.toml
  src/lib.rs                      crate root, SaveFile, re-exports
  src/error.rs                    SaveError
  src/init.rs                     level-init.dat: version, mods, prototype ID tables
  src/chunks.rs                   zip access + lazy level.datN inflation
  src/force.rs                    player-force location + calibration + unlock decode
  src/testsupport.rs              synthetic save-zip fixture builder
  tests/real_save.rs              opt-in ground-truth test (env-gated)

crates/solver/
  src/availability.rs             NEW — save → Availability adapter
  src/chain/mod.rs                MODIFY — Availability type, ChainGoal field
  src/chain/select.rs             MODIFY — availability filtering
  src/chain/error.rs              MODIFY — RecipeLocked, MachineLocked
  src/chain/solve.rs              MODIFY — pass availability to selection
  src/lib.rs                      MODIFY — expose availability module

crates/ui/
  src/chain_panel/save_picker.rs  NEW — saves-dir scan + dropdown state
  src/chain_panel/mod.rs          MODIFY — panel state, build_goal
  src/chain_panel/controls.rs     MODIFY — render the picker
```

---

### Task 1: `factorio-save` — container and `level-init.dat` [Mode: Delegated]

**Files:**
- Create: `crates/save/Cargo.toml`, `crates/save/src/lib.rs`, `crates/save/src/error.rs`, `crates/save/src/init.rs`, `crates/save/src/chunks.rs`, `crates/save/src/testsupport.rs`
- Modify: `Cargo.toml` (workspace members)

**Contracts:**

```rust
pub struct Version { pub major: u16, pub minor: u16, pub patch: u16 }

/// Prototype name ↔ numeric id for one category, as stored in level-init.dat.
pub struct IdTable(HashMap<u16, String>);
impl IdTable {
    pub fn len(&self) -> usize;
    pub fn name(&self, id: u16) -> Option<&str>;
    pub fn names(&self) -> impl Iterator<Item = &str>;
}

pub struct SaveFile { /* archive handle + parsed init + lazily inflated stream */ }

impl SaveFile {
    pub fn open(path: &Path) -> Result<Self, SaveError>;
    pub fn version(&self) -> Version;
    /// Mod names only. Version bytes in level-init are not decoded — the
    /// encoding was never verified, and names alone answer "is this vanilla".
    pub fn mods(&self) -> &[String];
    pub fn recipes(&self) -> &IdTable;
    pub fn technologies(&self) -> &IdTable;
    /// Explicit, non-default: inflates every chunk and checks the total against
    /// level.datmetadata. Not on the load path — see design, lazy inflation.
    pub fn verify_total_size(&mut self) -> Result<(), SaveError>;
}
```

`open` reads `level-init.dat` and `level.datmetadata` eagerly (both are small and uncompressed) and inflates **no** chunks.

Category-table format, verified against real saves: `[u8 name_len][category name][u16 count]` then `count` × `[u8 name_len][name][u16 id]`. Parse the `recipe` and `technology` categories by locating their length-prefixed category name.

Lazy inflation: chunks are the zip entries named `level.dat<N>` where `<N>` is all digits, ordered by **numeric** `N` (not lexicographic — `level.dat10` sorts before `level.dat2` as a string). Each is zlib-compressed and stored uncompressed in the zip. Expose an internal `ensure_inflated(&mut self, at_least: usize)` that inflates further chunks until the buffer reaches `at_least` bytes or chunks run out.

**Test Cases:**

```rust
#[test]
fn parses_version_and_mod_names() {
    let zip = FixtureSave::new().with_version(2, 0, 77).with_mods(&["base", "space-age"]).build();
    let s = SaveFile::open_bytes(&zip).unwrap();
    assert_eq!(s.version(), Version { major: 2, minor: 0, patch: 77 });
    assert_eq!(s.mods(), &["base".to_string(), "space-age".to_string()]);
}

#[test]
fn parses_recipe_and_technology_id_tables() {
    let zip = FixtureSave::new()
        .with_recipes(&["iron-plate", "copper-plate", "electronic-circuit"])
        .with_technologies(&["automation", "electronics"])
        .build();
    let s = SaveFile::open_bytes(&zip).unwrap();
    assert_eq!(s.recipes().len(), 3);
    assert_eq!(s.recipes().name(1), Some("iron-plate"));
    assert_eq!(s.technologies().len(), 2);
}

#[test]
fn chunks_are_ordered_numerically_not_lexicographically() {
    // 12 chunks forces the level.dat10 / level.dat2 ambiguity.
    let zip = FixtureSave::new().with_chunk_count(12).build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    s.verify_total_size().expect("chunks reassemble in numeric order");
}

#[test]
fn open_inflates_no_chunks() {
    let zip = FixtureSave::new().with_chunk_count(8).build();
    let s = SaveFile::open_bytes(&zip).unwrap();
    assert_eq!(s.inflated_chunk_count(), 0);
}

#[test]
fn size_mismatch_is_an_error() {
    let zip = FixtureSave::new().with_corrupt_metadata_total().build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    assert!(matches!(s.verify_total_size(), Err(SaveError::SizeMismatch { .. })));
}

#[test]
fn missing_level_init_is_an_error() {
    let zip = FixtureSave::new().without_level_init().build();
    assert!(matches!(SaveFile::open_bytes(&zip), Err(SaveError::MissingEntry { .. })));
}
```

**Constraints:**
- `crates/save` depends on `zip` and `flate2` only — **no** workspace crate. A dependency on `solver` would invert the graph (see design).
- `SaveError` variants: `Io`, `Zip`, `MissingEntry { name }`, `Decompress { chunk }`, `SizeMismatch { declared, actual }`, `MalformedInit { reason }`. Each names the offending file or chunk.
- Every file carries a first-line description comment; no file over 300 lines.
- Add `open_bytes(&[u8])` alongside `open(&Path)` so tests need no temp files.
- `inflated_chunk_count(&self) -> usize` is test-facing but part of the public
  surface — Task 2's lazy-inflation guard asserts on it.

**Fixture builder** (`testsupport.rs`) — the full deliverable, since Task 2's
tests are written against it:

```rust
pub struct FixtureSave { /* … */ }

impl FixtureSave {
    pub fn new() -> Self;                                   // 2.0.77, one chunk, minimal tables
    pub fn with_version(self, major: u16, minor: u16, patch: u16) -> Self;
    pub fn with_mods(self, names: &[&str]) -> Self;
    pub fn with_recipes(self, names: &[&str]) -> Self;      // ids assigned 1..=n
    pub fn with_technologies(self, names: &[&str]) -> Self;
    pub fn with_unlocked(self, names: &[&str]) -> Self;     // the rest decode as locked
    pub fn with_stride(self, stride: usize) -> Self;        // default 6
    pub fn with_force_padding(self, bytes: usize) -> Self;  // shifts the array offset
    pub fn with_chunk_count(self, n: usize) -> Self;        // pads the stream to n chunks
    pub fn without_level_init(self) -> Self;
    pub fn without_player_force(self) -> Self;
    pub fn with_corrupt_metadata_total(self) -> Self;       // level.datmetadata disagrees
    pub fn with_duplicate_satisfying_alignment(self) -> Self;
    pub fn build(self) -> Vec<u8>;                          // zip bytes
}
```

`with_duplicate_satisfying_alignment` is the only one with a non-obvious
construction. It must yield a stream where two distinct `(stride, offset)` pairs
both satisfy the invariant. The straightforward route: emit a run of records
whose flag bytes are **all `1`** and long enough to cover the search window at
the chosen stride, so every offset in that run decodes as "everything enabled"
and therefore trivially satisfies "every default-enabled recipe is enabled". The
decoder must refuse rather than pick the first.

**Verification:**
Run: `build-lock cargo test -p factorio-save` and `build-lock cargo clippy -p factorio-save`
Expected: all tests pass, no warnings

**Commit after passing.**

---

### Task 2: `factorio-save` — calibration and unlock decode [Mode: Delegated]

**Files:**
- Create: `crates/save/src/force.rs`, `crates/save/tests/real_save.rs`
- Modify: `crates/save/src/lib.rs`, `crates/save/src/error.rs`, `crates/save/src/testsupport.rs`

**Contracts:**

```rust
pub struct Calibration { pub stride: usize, pub offset: usize }

impl SaveFile {
    /// The player force's unlocked recipe names.
    ///
    /// `default_enabled` is the set of recipe names the game enables without
    /// research, supplied by the caller. It is the calibration invariant, not a
    /// hint: the located array must mark every one of them enabled.
    pub fn unlocked_recipes(
        &mut self,
        default_enabled: &HashSet<String>,
    ) -> Result<HashSet<String>, SaveError>;

    pub fn calibration(&self) -> Option<Calibration>;
}
```

Algorithm, in order:

1. Inflate chunks until the byte pattern `01 06 "player"` is found, or chunks run out (`ForceNotFound`).
2. For each `stride` in `5..=8` and each `offset` in `force_base + 16 ..= force_base + 1024`, inflating further as needed: read `recipes().len() + 1` records; reject the candidate unless every record's first byte is `0` or `1`.
3. Keep candidates where **every** name in `default_enabled ∩ recipes().names()` decodes as enabled.
4. Exactly one candidate → decode and return. Zero → `CalibrationFailed`. More than one → `CalibrationAmbiguous`.

Index 0 of the array is the `recipe-unknown` sentinel and has no name in the table; skip ids absent from the table rather than erroring.

**Test Cases:**

```rust
#[test]
fn decodes_unlocked_recipes_at_a_shifted_offset_and_stride() {
    // Offset and stride are both non-default — a hardcoded pair must fail this.
    let zip = FixtureSave::new()
        .with_recipes(&["iron-plate", "copper-plate", "advanced-circuit", "beacon"])
        .with_unlocked(&["iron-plate", "copper-plate"])
        .with_stride(7)
        .with_force_padding(113)
        .build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    let defaults = set(&["iron-plate", "copper-plate"]);
    let on = s.unlocked_recipes(&defaults).unwrap();
    assert_eq!(on, set(&["iron-plate", "copper-plate"]));
    assert_eq!(s.calibration().unwrap().stride, 7);
}

#[test]
fn rejects_an_alignment_off_by_exactly_one_record() {
    // The trap from the design: a one-record shift still looks plausible under
    // a weak check. The invariant must reject it, and the decode must be exact.
    let zip = FixtureSave::new()
        .with_recipes(&["a-plate", "b-plate", "c-plate", "d-plate", "e-plate"])
        .with_unlocked(&["a-plate", "b-plate", "c-plate"])
        .build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    let on = s.unlocked_recipes(&set(&["a-plate", "b-plate", "c-plate"])).unwrap();
    assert!(!on.contains("d-plate"), "off-by-one would leak the next recipe in");
}

#[test]
fn zero_candidates_is_a_calibration_error_naming_the_version() {
    // default_enabled names a recipe the save has locked — no alignment can satisfy it.
    let zip = FixtureSave::new()
        .with_recipes(&["iron-plate", "beacon"])
        .with_unlocked(&["iron-plate"])
        .build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    match s.unlocked_recipes(&set(&["iron-plate", "beacon"])) {
        Err(SaveError::CalibrationFailed { version, .. }) => {
            assert_eq!(version.major, 2);
        }
        other => panic!("expected CalibrationFailed, got {other:?}"),
    }
}

#[test]
fn multiple_candidates_are_refused_never_guessed() {
    let zip = FixtureSave::new().with_duplicate_satisfying_alignment().build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    assert!(matches!(
        s.unlocked_recipes(&set(&["iron-plate"])),
        Err(SaveError::CalibrationAmbiguous { .. })
    ));
}

#[test]
fn reads_only_the_chunks_it_needs() {
    // Guards the lazy-inflation optimisation against silent regression.
    let zip = FixtureSave::new().with_chunk_count(40).build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    s.unlocked_recipes(&set(&["iron-plate"])).unwrap();
    assert!(s.inflated_chunk_count() < 40, "must not inflate the whole stream");
}

#[test]
fn force_not_found_is_an_error() {
    let zip = FixtureSave::new().without_player_force().build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    assert!(matches!(
        s.unlocked_recipes(&set(&["iron-plate"])),
        Err(SaveError::ForceNotFound)
    ));
}
```

`tests/real_save.rs` — skipped unless `FACTORIO_SAVE_FIXTURE` names a real save:

```rust
#[test]
fn real_save_ground_truth() {
    let Ok(path) = std::env::var("FACTORIO_SAVE_FIXTURE") else { return };
    // Against 2026.zip (2.0.77, 369/659): assembling-machine-2 unlocked,
    // assembling-machine-3 and oil-refinery locked.
}
```

**Constraints:**
- New `SaveError` variants: `ForceNotFound`, `CalibrationFailed { version, checked: usize }`, `CalibrationAmbiguous { candidates: usize }`. `CalibrationFailed`'s message must state the likely cause — the committed dump does not match the save's game version — and name the remedy (regenerate via `dump-ingest`).
- Never fall back to a best-guess alignment. Ambiguity is an error, matching the project's existing rule.
- The search must not inflate the entire stream when the force is found early.

**Verification:**
Run: `build-lock cargo test -p factorio-save`
Then, for ground truth: `FACTORIO_SAVE_FIXTURE=<path to a real save> build-lock cargo test -p factorio-save -- --include-ignored`
Expected: all pass

**Commit after passing.**

---

### Task 3: solver — `Availability` and selection filtering [Mode: Delegated]

**Files:**
- Modify: `crates/solver/src/chain/mod.rs`, `crates/solver/src/chain/select.rs`, `crates/solver/src/chain/error.rs`, `crates/solver/src/chain/solve.rs`

**Contracts:**

```rust
/// What the player can actually build.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Availability {
    /// Everything. The default — preserves existing behaviour exactly.
    #[default]
    Unrestricted,
    /// Only these recipe names.
    Unlocked(HashSet<String>),
}

impl Availability {
    pub fn allows_recipe(&self, recipe: &str) -> bool;
    /// A machine is craftable when some allowed recipe produces an item named
    /// `machine`. Keyed on the produced item, not the recipe name.
    pub fn allows_machine(&self, machine: &str) -> bool;
}

// ChainGoal gains:
pub availability: Availability,
impl ChainGoal { pub fn with_availability(self, a: Availability) -> Self; }

// select.rs — signatures change:
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

**`candidates_for` keeps its current signature and stays purely structural.** This is the crux of the task — see Constraints.

**Test Cases:**

```rust
#[test]
fn locked_intermediate_errors_and_is_not_billed_to_the_bus() {
    // THE regression guard. An intermediate whose only recipes are locked must
    // NOT be silently treated as a raw resource by solve.rs's empty-candidates
    // shortcut. It must surface as RecipeLocked.
    let goal = ChainGoal::new("electronic-circuit", Rate::ItemsPerSec(1.0), &["iron-plate"])
        .with_availability(unlocked(&["electronic-circuit"])); // copper-cable locked
    match chain::solve(&goal) {
        Err(ChainError::RecipeLocked { item, .. }) => assert_eq!(item, "copper-cable"),
        Ok(plan) => panic!("copper-cable leaked into inputs: {:?}", plan.inputs),
        other => panic!("expected RecipeLocked, got {other:?}"),
    }
}

#[test]
fn genuinely_raw_items_still_resolve_as_inputs() {
    // The other side of the same coin: iron-ore has no recipe at all, so
    // restricting availability must not turn it into an error.
    let goal = ChainGoal::new("iron-plate", Rate::ItemsPerSec(1.0), &[])
        .with_availability(unlocked(&["iron-plate"]));
    let plan = chain::solve(&goal).expect("iron-ore is raw, not locked");
    assert!(plan.inputs.iter().any(|i| i.name == "iron-ore"));
}

#[test]
fn availability_resolves_an_otherwise_ambiguous_item() {
    // copper-cable has several producers; with only one unlocked the
    // AmbiguousRecipe error disappears without needing an override.
    let goal = ChainGoal::new("copper-cable", Rate::ItemsPerSec(1.0), &["copper-plate"])
        .with_availability(unlocked(&["copper-cable"]));
    assert!(chain::solve(&goal).is_ok());
}

#[test]
fn fastest_policy_falls_back_past_a_locked_machine() {
    let recipe = recipe::get("electronic-circuit").unwrap();
    let a = unlocked(&["assembling-machine-1", "assembling-machine-2"]);
    let m = select_machine(recipe, &MachinePolicy::fastest(), &a).unwrap();
    assert_eq!(m.name, "assembling-machine-2");
}

#[test]
fn named_locked_machine_errors_rather_than_downgrading() {
    let recipe = recipe::get("electronic-circuit").unwrap();
    let a = unlocked(&["assembling-machine-1"]);
    match select_machine(recipe, &MachinePolicy::all("assembling-machine-3"), &a) {
        Err(ChainError::MachineLocked { machine, .. }) => {
            assert_eq!(machine, "assembling-machine-3")
        }
        other => panic!("expected MachineLocked, got {other:?}"),
    }
}

#[test]
fn an_explicit_override_to_a_locked_recipe_is_still_honoured() {
    // The machine must be unlocked or this fails for an unrelated reason:
    // with an empty unlocked set NOTHING is craftable, so the override would
    // die at machine selection rather than proving the recipe path works.
    let goal = ChainGoal::new("copper-cable", Rate::ItemsPerSec(1.0), &["copper-plate"])
        .with_availability(unlocked(&["assembling-machine-2"])) // copper-cable itself locked
        .with_override("copper-cable", "copper-cable");
    assert!(chain::solve(&goal).is_ok());
}

#[test]
fn unrestricted_availability_changes_nothing() {
    // The default must be inert: setting it explicitly yields the same plan as
    // not setting it. Every pre-existing chain and layout test is the broader
    // form of this assertion and must still pass untouched.
    let base = ChainGoal::new("electronic-circuit", Rate::ItemsPerSec(1.0),
                              &["iron-plate", "copper-plate"])
        .with_override("copper-cable", "copper-cable");
    let explicit = base.clone().with_availability(Availability::Unrestricted);
    let (a, b) = (chain::solve(&base).unwrap(), chain::solve(&explicit).unwrap());
    assert_eq!(a.steps.len(), b.steps.len());
    assert_eq!(a.inputs, b.inputs);
}
```

**Constraints:**
- **Do not put availability filtering inside `candidates_for`.** `solve.rs:91` uses `candidates_for(&item).is_empty()` as its raw-resource test; filtering there would make a locked intermediate look like iron ore and silently add it to `plan.inputs`. Filter inside `select_recipe` instead, per the table in the design doc.
- New `ChainError` variants, each naming the offender and the remedy, matching the existing style in `error.rs`:
  - `RecipeLocked { item: String, recipes: Vec<String> }`
  - `MachineLocked { machine: String, recipe: String }`
- `MachineLocked` is distinct from `NoMachineForCategory` — the latter's remedy ("pick a machine for that category") is wrong advice when the category is fine and the machine simply is not researched.
- `select_recipe`'s override path bypasses availability by design.
- `Availability::Unrestricted` is the `Default`, so every existing construction and test is unchanged.
- Callers to update: `solve.rs` (2 sites) and `select.rs`'s own test module.
- **An `Unlocked` set gates machines as well as recipes**, because machine
  craftability reads the same set. A fixture that lists only the recipes under
  test will fail at machine selection for an unrelated reason — every
  `Unlocked` fixture must also include the machine recipes it expects to use.
  This is correct behaviour, not a wrinkle to work around: a real save's
  unlocked set naturally contains machine recipes.
- Verified against `crates/solver/data/recipes.json` while writing this plan, so
  the test expectations above are sound: `copper-cable` has two non-recycling
  producers (`copper-cable`, `casting-copper-cable`), `electronic-circuit`
  consumes `iron-plate` and `copper-cable`, and `beacon`,
  `assembling-machine-2/3` and `oil-refinery` are all `enabled: false`.

**Verification:**
Run: `build-lock cargo test -p factorio-solver` and `build-lock cargo clippy -p factorio-solver`
Expected: all pass, including every pre-existing chain and layout test

**Commit after passing.**

---

### Task 4: solver — save adapter [Mode: Direct]

**Files:**
- Create: `crates/solver/src/availability.rs`
- Modify: `crates/solver/src/lib.rs`, `crates/solver/Cargo.toml`

**Contracts:**

```rust
/// Recipe names the game enables without research — the calibration invariant,
/// derived from the committed recipe registry.
pub fn default_enabled() -> HashSet<String>;

/// Read a save and turn it into an availability constraint.
pub fn from_save(path: &Path) -> Result<Availability, SaveError>;
```

**Test Cases:**

```rust
#[test]
fn default_enabled_matches_the_registry() {
    let d = default_enabled();
    assert!(d.contains("iron-plate"));
    assert!(!d.contains("beacon")); // research-locked in the dump
    assert_eq!(d.len(), recipe::registry().values().filter(|r| r.enabled).count());
}
```

**Constraints:**
- This is the only place `solver` depends on `save`. Adding `factorio-save` to `solver`'s manifest keeps the graph acyclic: `ui → solver → save`.
- Do not re-export `SaveError` variants into `ChainError` — a save-reading failure is not a chain failure.

**Verification:**
Run: `build-lock cargo test -p factorio-solver`
Expected: passes

**Commit after passing.**

---

### Task 5: UI — save picker [Mode: Delegated]

**Files:**
- Create: `crates/ui/src/chain_panel/save_picker.rs`
- Modify: `crates/ui/src/chain_panel/mod.rs`, `crates/ui/src/chain_panel/controls.rs`

**Contracts:**

```rust
/// A save file discovered on disk.
pub struct SaveEntry { pub path: PathBuf, pub label: String, pub modified: SystemTime }

/// Scan a saves directory, newest first. Returns empty when the directory is
/// absent — the app must build and run with no Factorio installed.
pub fn scan_saves(dir: &Path) -> Vec<SaveEntry>;

/// `~/.factorio/saves`, or None when the home directory cannot be resolved.
pub fn default_saves_dir() -> Option<PathBuf>;

pub struct SavePickerState {
    pub entries: Vec<SaveEntry>,
    pub selected: Option<PathBuf>,
    pub manual_path: String,
    /// Decoded once, when a save is selected. `build_goal` reads this and never
    /// decodes: it runs on every Solve click, and re-deriving there would reopen
    /// and re-inflate the save each time for an answer that cannot have changed.
    pub availability: Availability,
    pub status: Option<Result<usize, String>>, // unlocked count, or error text
}
```

`ChainPanel::build_goal` reads `availability` straight from this state — it is
`Unrestricted` until a save is successfully decoded, and reset to `Unrestricted`
whenever the selection is cleared or a load fails.

**Test Cases:**

```rust
#[test]
fn scan_returns_saves_newest_first() {
    // Build a temp dir with three zips of known, distinct mtimes.
    let entries = scan_saves(&dir);
    assert_eq!(entries.iter().map(|e| e.label.as_str()).collect::<Vec<_>>(),
               vec!["newest", "middle", "oldest"]);
}

#[test]
fn scan_of_a_missing_directory_is_empty_not_an_error() {
    assert!(scan_saves(Path::new("/nonexistent")).is_empty());
}

#[test]
fn scan_ignores_non_zip_files() {
    // A dir holding "a.zip", "notes.txt" and a "b" subdirectory yields only a.zip.
    let entries = scan_saves(&dir);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].label, "a");
}

#[test]
fn build_goal_is_unrestricted_when_no_save_is_selected() {
    let panel = ChainPanel::default();
    assert_eq!(panel.build_goal().availability, Availability::Unrestricted);
}

#[test]
fn a_failed_load_surfaces_the_error_and_leaves_availability_unrestricted() {
    // Selecting an unreadable save must not half-apply: the error is shown and
    // the solver stays unrestricted rather than silently gaining an empty set,
    // which would make every recipe look locked.
    let mut state = SavePickerState::default();
    state.select(Path::new("/nonexistent/save.zip"));
    assert!(matches!(state.status, Some(Err(_))));
    assert_eq!(state.availability, Availability::Unrestricted);
}
```

**Constraints:**
- No new crate dependency. Deliberately not a native file dialog (`rfd`) — see design.
- Sort by modification time descending; ties broken by name for determinism.
- Label is the file stem, not the full path.
- A decode failure shows the error text in the panel and leaves availability unrestricted. It must never half-apply.
- The existing chain panel scroll behaviour must be preserved — the picker is added inside the scrolled region so Generate stays reachable (see commit 3778f8c).
- Follow the existing `chain_panel` module conventions; keep `mod.rs` under 300 lines by putting scan logic in `save_picker.rs`.

**Verification:**
Run: `build-lock cargo test -p factorio-ui` and `build-lock cargo clippy --workspace`
Then launch `cargo run -p factorio-ui`, pick a real save, and confirm the unlocked count appears and a green-circuit goal no longer proposes assembling-machine-3.
Expected: tests pass; manual check confirms the behaviour

**Commit after passing.**

---

## Final Verification

Run the full suite through `test-suite` (not a scoped run) before the final commit, then update:
- `CLAUDE.md` — crate list, dependency graph, current phase
- `phases/current.md` — new phase entry
- Close idea #3356; mark #3350 superseded

---

## Execution
**Skill:** Subagent Dev (if included in your instructions)
- Mode A tasks: orchestrator implements directly
- Mode B tasks: Dispatched to subagents
