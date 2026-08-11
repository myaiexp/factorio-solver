# Game Data Foundation Implementation Plan

**Goal:** Replace the hand-written 85-entity prototype table with 169 entities and 659 recipes ingested from the player's own Factorio install, and render real game icons in the viewport.

**Architecture:** A standalone `dump-ingest` binary reads `data-raw-dump.json` (produced by the mod-free `factorio --dump-data`) and emits two committed JSON files: an expanded `prototypes.json` for `grid` and a new `recipes.json` for `solver`. It depends on `grid` and `solver` for their types so the emitted shape can't drift from the consuming serde definitions. Icons are read live from the install at runtime by `ui` — never dumped, never committed. No change to the app's runtime crate graph.

**Tech Stack:** Rust, serde/serde_json, `image` (PNG decode), egui/eframe, `clap` (ingest CLI).

**Design doc:** `.claude/plans/2026-08-11-game-data-foundation-design.md` — read it first. It records measured facts (counts, defaults, traps) that these tasks assume.

---

## File Structure

**Create:**
- `crates/dump-ingest/Cargo.toml` — package name **`factorio-dump-ingest`** (workspace convention: every package is `factorio-*`, directories are shortened). Dev tool, not a runtime dependency.
- `crates/dump-ingest/src/main.rs` — CLI entry, arg parsing, orchestration
- `crates/dump-ingest/src/dump.rs` — access to the raw dump's nested shape
- `crates/dump-ingest/src/entities.rs` — entity filter + tile-size derivation + field mapping
- `crates/dump-ingest/src/recipes.rs` — recipe extraction + the default/shape rules
- `crates/dump-ingest/src/locale.rs` — locale file loading
- `crates/dump-ingest/tests/fixtures/mini-dump.json` — small hand-built dump fragment
- `crates/solver/src/recipe.rs` — `Recipe`/`ItemAmount`/`ItemKind` + registry
- `crates/solver/data/recipes.json` — generated, committed
- `crates/ui/src/icons.rs` — install detection, path resolution, decode/crop, texture cache

**Modify:**
- `crates/grid/src/prototype.rs` — additive fields on `EntityPrototype`
- `crates/grid/data/prototypes.json` — regenerated (85 → 169 entries)
- `crates/solver/src/lib.rs` — export `recipe`
- `crates/ui/src/app.rs` — draw icons at `LodLevel::Full`
- `Cargo.toml` — add `crates/dump-ingest` to workspace members
- `CLAUDE.md` — update entity/recipe counts and the data-provenance note

---

### Task 1: Extend `EntityPrototype` with dump-derived fields

**Files:**
- Modify: `crates/grid/src/prototype.rs`
- Test: `crates/grid/src/prototype.rs` (inline `#[cfg(test)]`, matching existing convention)

**Contracts:**

Add to `EntityPrototype`, every field `#[serde(default)]` so the current 85-entry file still deserializes unchanged:

```rust
pub display_name: Option<String>,
pub icon_path: Option<String>,          // mod-relative, e.g. "__base__/graphics/icons/x.png"
pub icon_size: Option<u32>,             // absent → caller uses 64
pub crafting_categories: Vec<String>,
pub underground_max_distance: Option<u32>,
pub pickup_position: Option<(f64, f64)>,
pub insert_position: Option<(f64, f64)>,
```

**Test Cases:**

**Existing public API — use it, do not add to it.** `grid::prototype` exposes
`pub fn lookup(name: &str) -> Option<&'static EntityPrototype>` and
`pub fn all_names() -> Vec<&'static str>`; the `REGISTRY` static is private and there is
**no** `registry()` accessor. Tests use `lookup()` and `all_names().len()`. Do not widen the
public surface just for tests.

```rust
#[test]
fn existing_prototypes_json_still_loads() {
    // The committed 85-entry file predates every new field.
    let p = lookup("transport-belt").expect("transport-belt present");
    assert_eq!(p.belt_throughput, Some(15.0));
}

#[test]
fn new_fields_default_when_absent() {
    let p: EntityPrototype =
        serde_json::from_str(r#"{"name":"x","tile_width":1,"tile_height":1}"#).unwrap();
    assert_eq!(p.display_name, None);
    assert_eq!(p.icon_size, None);
    assert!(p.crafting_categories.is_empty());
}
```

**Constraints:**
- Purely additive. No existing field changes type or name; no call site in `grid`/`ui` changes.
- Do NOT populate the new fields here — Task 5 regenerates the data file.

**Verification:**
Run: `build-lock cargo test -p factorio-grid`
Expected: all pass, including the pre-existing suite.

**Commit after passing.** `[Mode: Direct]`

---

### Task 2: `Recipe` types and registry in `solver`

**Files:**
- Create: `crates/solver/src/recipe.rs`
- Modify: `crates/solver/src/lib.rs`
- Modify: `crates/solver/Cargo.toml` — **add `serde` (derive) and `serde_json`**; the crate
  currently depends only on `factorio-templates` and `thiserror` and has no serde at all
- Test: `crates/solver/src/recipe.rs` (inline `#[cfg(test)]`)

**Contracts:**

```rust
pub enum ItemKind { Item, Fluid }

pub struct ItemAmount {
    pub name: String,
    pub kind: ItemKind,
    pub amount: f64,
}

pub struct Recipe {
    pub name: String,
    pub display_name: Option<String>,
    pub category: String,
    pub energy_required: f64,
    pub ingredients: Vec<ItemAmount>,
    pub results: Vec<ItemAmount>,
    pub enabled: bool,
    pub hidden: bool,
}

/// Loaded once from the committed recipes.json, mirroring grid's prototype registry.
pub fn registry() -> &'static HashMap<String, Recipe>;
pub fn get(name: &str) -> Option<&'static Recipe>;
```

**Serde requirements — these are the phase's highest-risk paths (see design doc):**

- `enabled`: absent → **`true`**. `#[serde(default)]` on a `bool` yields `false` and is WRONG. Use `#[serde(default = "default_true")]`.
- `category`: absent → `"crafting"`.
- `energy_required`: absent → `0.5`.
- `ingredients` / `results`: must accept a JSON **array**, an empty **object `{}`**, or **`null`**, all yielding an empty vec for the latter two. `#[serde(default)]` is insufficient — it covers a missing key, not a key present with a mismatched type.

**Test Cases:**

```rust
#[test]
fn enabled_defaults_to_true_when_absent() {
    let r: Recipe = serde_json::from_str(
        r#"{"name":"iron-plate","category":"smelting","energy_required":3.2,
            "ingredients":[],"results":[]}"#).unwrap();
    assert!(r.enabled, "absent `enabled` means available, not research-locked");
}

#[test]
fn category_and_energy_defaults() {
    let r: Recipe = serde_json::from_str(
        r#"{"name":"x","ingredients":[],"results":[]}"#).unwrap();
    assert_eq!(r.category, "crafting");
    assert_eq!(r.energy_required, 0.5);
}

#[test]
fn empty_lua_table_object_parses_as_empty_vec() {
    // Factorio dumps an empty Lua table as `{}`. biter-egg is a REAL recipe
    // that is genuinely ingredient-free and hits this path.
    let r: Recipe = serde_json::from_str(
        r#"{"name":"biter-egg","ingredients":{},
            "results":[{"type":"item","name":"biter-egg","amount":5}]}"#).unwrap();
    assert!(r.ingredients.is_empty());
    assert_eq!(r.results.len(), 1);
    assert_eq!(r.results[0].amount, 5.0);
}

#[test]
fn null_ingredients_parses_as_empty_vec() {
    let r: Recipe = serde_json::from_str(
        r#"{"name":"parameter-0","ingredients":null,"results":null}"#).unwrap();
    assert!(r.ingredients.is_empty() && r.results.is_empty());
}

#[test]
fn fluid_ingredients_keep_their_kind() {
    let r: Recipe = serde_json::from_str(
        r#"{"name":"x","ingredients":[{"type":"fluid","name":"water","amount":50}],
            "results":[]}"#).unwrap();
    assert!(matches!(r.ingredients[0].kind, ItemKind::Fluid));
}
```

**Constraints:**
- `solver` must not gain a dependency on `grid` beyond what it already has via `templates`.
- Registry load mirrors `grid::prototype`'s `OnceLock` + `include_str!` pattern, and keeps the same strict `.expect()` on malformed JSON (a committed bad file is a programming error).
- Task 5 generates `recipes.json`. Until then, commit a **minimal valid placeholder** (a two-recipe file) so the crate compiles and tests run.

**Verification:**
Run: `build-lock cargo test -p factorio-solver`
Expected: all pass.

**Commit after passing.** `[Mode: Direct]`

---

### Task 3: `dump-ingest` crate — scaffolding and entity extraction

**Files:**
- Create: `crates/dump-ingest/Cargo.toml`, `src/main.rs`, `src/dump.rs`, `src/entities.rs`
- Modify: `Cargo.toml` (workspace members)
- Test: `crates/dump-ingest/src/entities.rs` (inline), `crates/dump-ingest/tests/fixtures/mini-dump.json`

**Contracts:**

```
dump-ingest --dump <path> --locale-dir <dir> --out-prototypes <path> --out-recipes <path>
```

```rust
/// Entities whose `flags` contain "player-creation" AND that have a `selection_box`.
pub fn placeable_entities(dump: &serde_json::Value) -> Vec<&serde_json::Value>;

/// explicit tile_width/tile_height if declared, else ceil(collision_box extent - 0.01)
pub fn tile_size(entity: &serde_json::Value) -> (u32, u32);

/// "150kW" -> 150.0
pub fn parse_power_kw(s: &str) -> Option<f64>;

pub fn to_prototype(entity: &serde_json::Value, locale: &Locale) -> EntityPrototype;
```

**Dump shape (load-bearing):** the top level is a dict of ~250 *prototype-type* keys (`"assembling-machine"`, `"furnace"`, `"recipe"`, …), each mapping entity name → object. Not a flat list.

**Field mapping:** `tile_width`/`tile_height` per `tile_size`; `crafting_speed`, `module_slots`, `crafting_categories` verbatim; `power_kw` from `energy_usage`; `belt_throughput` = `speed × 480` for belt-family types; `underground_max_distance` from `max_distance`; `icon_path` from `.icon` or `.icons[0].icon`; `icon_size` from `.icon_size`; `display_name` from locale.

**`fluid_connections` — do not omit this.** Two existing tests in `grid` assert on it
(`test_fluid_connections_chemical_plant`, `test_no_fluid_connections_belt`), so leaving it
unmapped silently zeroes the field and breaks them in Task 5. Read **both** shapes:
`fluid_boxes` (plural, 8 entities) and `fluid_box` (singular, 14). Emit the raw
center-relative `pipe_connections[].position` as `dx`/`dy` and map `production_type` →
`FluidConnectionType`. Use that one convention throughout — see the design doc for why the
old hand-written values are not a reproduction target.

**Test Cases:**

```rust
#[test]
fn tile_size_prefers_explicit_declaration() {
    // train-stop declares 2x2 despite a 1x1 collision box (trains pass through).
    let e = json!({"tile_width":2,"tile_height":2,
                   "collision_box":[[-0.5,-0.5],[0.5,0.5]]});
    assert_eq!(tile_size(&e), (2, 2));
}

#[test]
fn tile_size_falls_back_to_collision_box() {
    // assembling-machine-2: collision 2.4 -> 3x3
    let e = json!({"collision_box":[[-1.2,-1.2],[1.2,1.2]]});
    assert_eq!(tile_size(&e), (3, 3));
}

#[test]
fn tile_size_tolerates_float_noise() {
    // Real dump values look like 2.39999999999999996447...
    let e = json!({"collision_box":[[-1.19999999999999996,-1.19999999999999996],
                                    [1.19999999999999996,1.19999999999999996]]});
    assert_eq!(tile_size(&e), (3, 3));
}

#[test]
fn placeable_filter_requires_both_flag_and_selection_box() {
    // an entity with the flag but no selection_box is excluded, and vice versa
}

#[test]
fn parse_power_handles_observed_formats() {
    assert_eq!(parse_power_kw("150kW"), Some(150.0));
    assert_eq!(parse_power_kw("2000kW"), Some(2000.0));
    assert_eq!(parse_power_kw("0kW"), Some(0.0));
}

#[test]
fn belt_throughput_derives_from_speed() {
    // 0.03125 * 480 == 15.0, matching the committed transport-belt value
}
```

**Constraints:**
- Depends on `factorio-grid` for `EntityPrototype` — serialize the real struct, never a hand-written JSON shape.
- **Strict and loud.** This is a rarely-run dev tool; a silent partial write poisons every later phase. An entity missing a required field aborts with its name. Never skip silently.
- Must not be a workspace default-run target or a build-time step — the app must build with no Factorio install present.
- Output JSON is a stable-ordered array (sort by name) so regenerating produces a reviewable diff rather than reshuffled noise.

**Verification:**
Run: `build-lock cargo test -p factorio-dump-ingest && build-lock cargo build --workspace`
Expected: all pass.

**Commit after passing.** `[Mode: Delegated]`

---

### Task 4: `dump-ingest` — recipe extraction and locale merge

**Files:**
- Create: `crates/dump-ingest/src/recipes.rs`, `src/locale.rs`
- Modify: `crates/dump-ingest/src/main.rs`
- Modify: `crates/dump-ingest/Cargo.toml` — add the `factorio-solver` path dependency (Task 3 creates the crate with only `factorio-grid`)
- Test: inline in both new modules

**Contracts:**

```rust
pub struct Locale { names: HashMap<String, String> }

/// Reads <locale-dir>/{entity,recipe}-locale.json, shape {"names":{..},"descriptions":{..}}
pub fn load_locale(dir: &Path, kind: &str) -> Result<Locale, IngestError>;

/// Applies every default and shape rule from the design doc.
pub fn to_recipe(name: &str, raw: &serde_json::Value, locale: &Locale) -> Option<Recipe>;
```

`to_recipe` returns `None` for `parameter: true` recipes (skipped). `hidden: true` recipes are **kept** with the flag preserved.

**Test Cases:**

```rust
#[test]
fn absent_enabled_becomes_true() {
    let raw = json!({"category":"smelting","energy_required":3.2,
                     "ingredients":[],"results":[]});
    assert!(to_recipe("iron-plate", &raw, &Locale::empty()).unwrap().enabled);
}

#[test]
fn absent_category_becomes_crafting() {
    let raw = json!({"ingredients":[],"results":[]});
    assert_eq!(to_recipe("bulk-inserter", &raw, &Locale::empty()).unwrap().category,
               "crafting");
}

#[test]
fn object_ingredients_become_empty() {
    let raw = json!({"ingredients":{},
                     "results":[{"type":"item","name":"biter-egg","amount":5}]});
    let r = to_recipe("biter-egg", &raw, &Locale::empty()).unwrap();
    assert!(r.ingredients.is_empty());
}

#[test]
fn parameter_recipes_are_skipped() {
    let raw = json!({"parameter":true,"ingredients":null,"results":null});
    assert!(to_recipe("parameter-0", &raw, &Locale::empty()).is_none());
}

#[test]
fn hidden_recipes_are_kept_with_flag() {
    let raw = json!({"hidden":true,"ingredients":[],"results":[]});
    let r = to_recipe("some-recycling", &raw, &Locale::empty()).unwrap();
    assert!(r.hidden);
}

#[test]
fn display_name_comes_from_locale() { /* locale hit populates display_name */ }
```

**Constraints:**
- Depends on `factorio-solver` for `Recipe` — serialize the real struct.
- Same strict-and-loud policy as Task 3.
- Stable-ordered output (sort by name).

**Verification:**
Run: `build-lock cargo test -p factorio-dump-ingest`
Expected: all pass.

**Commit after passing.** `[Mode: Delegated]`

---

### Task 5: Regenerate the committed data files

**Files:**
- Modify: `crates/grid/data/prototypes.json` (85 → 169 entries)
- Modify: `crates/solver/data/recipes.json` (placeholder → 649 recipes)
- Modify: `crates/grid/src/prototype.rs` — update `test_fluid_connections_*` (see constraints)
- Modify: `CLAUDE.md` (counts + provenance)
- Test: `crates/grid/tests/prototype_regression.rs` (new) — **grid assertions only**
- Test: `crates/solver/tests/recipe_regression.rs` (new) — **solver assertions only**

The two test files are deliberately separate. `grid` sits *below* `solver` in the crate
graph, so putting recipe assertions in a `grid` test would require adding `factorio-solver`
as a dev-dependency of `grid` — inverting the dependency direction and breaking the
"each crate is independently testable" principle.

**Contracts:** Run the tool against the reference dump at
`/tmp/claude-1000/-home-mase-helm-worktrees-factorio-solver-cbde8721/cbde8721-deee-4701-a4a0-7e219b158e4c/scratchpad/dump/`
(or a fresh `factorio --dump-data`), then commit the two outputs.

**Test Cases:**

```rust
/// 80 of the 85 previously hand-written entries must be reproduced exactly.
#[test]
fn regenerated_data_preserves_known_good_sizes() {
    // transport-belt 1x1 @ 15.0, assembling-machine-2 3x3, electric-furnace 3x3,
    // train-stop 2x2 (explicit declaration), offshore-pump 1x1 (explicit declaration)
}

/// The five Space Age footprints that were WRONG in the hand-written table.
/// Pinned by name so a future rule change cannot silently revert them.
#[test]
fn regenerated_data_corrects_five_wrong_footprints() {
    use factorio_grid::prototype::lookup;
    let size = |n| { let p = lookup(n).unwrap(); (p.tile_width, p.tile_height) };
    assert_eq!(size("big-mining-drill"), (5, 5));
    assert_eq!(size("foundry"),          (5, 5));
    assert_eq!(size("biolab"),           (5, 5));
    assert_eq!(size("rocket-turret"),    (3, 3));
    assert_eq!(size("recycler"),         (2, 4));
}

#[test]
fn entity_count_matches_2_0_77() {
    assert_eq!(factorio_grid::prototype::all_names().len(), 169);
}
```

And in `crates/solver/tests/recipe_regression.rs`:

```rust
#[test]
fn recipe_count_matches_2_0_77() {
    // 659 in the dump minus the 10 `parameter` recipes.
    assert_eq!(factorio_solver::recipe::registry().len(), 649);
}

#[test]
fn enabled_aggregate_guards_against_bool_default_regression() {
    // Of the 649 kept recipes, exactly 326 are research-locked and 323 are not
    // (91 explicit `true` + 232 absent). A regression to bool::default() would
    // report 558 locked and fail loudly.
    let locked = factorio_solver::recipe::registry().values()
        .filter(|r| !r.enabled).count();
    assert_eq!(locked, 326);
}
```

**Constraints:**
- The five corrected footprints are **expected changes**, not regressions — the design doc explains why the old values were wrong.
- **`storage-tank` fluid connections go 1 → 4.** This is also an expected change: the committed value was simply wrong (the game has 4). `chemical-plant` (4), `oil-refinery` (5) and `pump` (2) keep their counts, and `test_no_fluid_connections_belt` stays green. Update the inline `test_fluid_connections_*` tests in `crates/grid/src/prototype.rs` accordingly. Note the *positions* will change too, since the derived data uses the dump's center-relative convention rather than the old inconsistent hand-written one — assert on counts and connection types, not on the old coordinates.
- Re-run the full existing suite: expanding 85 → 169 entities means blueprints that previously produced `SkippedEntity` entries now import. Any test asserting on skip behaviour may legitimately need updating — verify each change is correct rather than adjusting until green.
- Update `CLAUDE.md`'s "85 entity prototypes" line and record that the data is dump-derived from 2.0.77.

**Verification:**
Run: `test-suite` (full workspace run)
Expected: all pass.

**Commit after passing.** `[Mode: Direct]`

---

### Task 6: Icon loading from the Factorio install

**Files:**
- Create: `crates/ui/src/icons.rs`
- Modify: `crates/ui/src/main.rs` (module declaration)
- Test: inline `#[cfg(test)]`

**Contracts:**

```rust
/// Probes known install locations; `override_path` (from config) wins when set.
pub fn detect_install(override_path: Option<&Path>) -> Option<PathBuf>;

/// "__base__/graphics/icons/x.png" -> "<install>/data/base/graphics/icons/x.png"
/// The path after the prefix is preserved VERBATIM — not all icons live in
/// graphics/icons/ (some are entity sprites).
pub fn resolve_icon_path(install: &Path, mod_relative: &str) -> Option<PathBuf>;

/// Leftmost `icon_size` square; icon_size defaults to 64.
/// Handles both observed layouts: 120x64 mipmap strips and plain 64x64.
pub fn crop_icon(img: DynamicImage, icon_size: u32) -> DynamicImage;

pub struct IconCache { /* lazily loads + caches egui textures by prototype name */ }
impl IconCache {
    pub fn get(&mut self, ctx: &egui::Context, proto: &EntityPrototype)
        -> Option<egui::TextureHandle>;
}
```

**Test Cases:**

```rust
#[test]
fn resolves_mod_prefix_to_install_subdir() {
    let p = resolve_icon_path(Path::new("/i"), "__base__/graphics/icons/x.png").unwrap();
    assert_eq!(p, Path::new("/i/data/base/graphics/icons/x.png"));
    let p = resolve_icon_path(Path::new("/i"), "__space-age__/graphics/icons/y.png").unwrap();
    assert_eq!(p, Path::new("/i/data/space-age/graphics/icons/y.png"));
}

#[test]
fn preserves_non_icons_directory_paths() {
    // 3 real icons are entity sprites, not in graphics/icons/
    let p = resolve_icon_path(Path::new("/i"),
        "__base__/graphics/entity/one-way-valve/one-way-valve-east.png").unwrap();
    assert!(p.ends_with("data/base/graphics/entity/one-way-valve/one-way-valve-east.png"));
}

#[test]
fn crops_mipmap_strip_to_leading_square() {
    let img = DynamicImage::new_rgba8(120, 64);   // 153 of 156 real icons
    assert_eq!(crop_icon(img, 64).dimensions(), (64, 64));
}

#[test]
fn passes_through_plain_square_icon() {
    let img = DynamicImage::new_rgba8(64, 64);    // the other 3
    assert_eq!(crop_icon(img, 64).dimensions(), (64, 64));
}

#[test]
fn undersized_image_is_used_whole_not_cropped() {
    let img = DynamicImage::new_rgba8(32, 32);
    assert_eq!(crop_icon(img, 64).dimensions(), (32, 32));
}

#[test]
fn missing_install_yields_none_and_does_not_panic() {
    assert!(detect_install(Some(Path::new("/nonexistent"))).is_none());
}
```

**Constraints:**
- **Every failure path degrades to `None`.** A missing install, missing file, or decode error must never prevent the app from starting or panic — the caller falls back to the existing colored rectangle.
- Textures load lazily on first draw and are cached; never decode per frame.
- Detection failure must be *surfaced* in the UI (state that icons are unavailable and name the config key), not swallowed silently.
- Add `image` to `crates/ui/Cargo.toml` with default features off plus `png` only — the app needs no other codec.

**Verification:**
Run: `build-lock cargo test -p factorio-ui`
Expected: all pass.

**Commit after passing.** `[Mode: Delegated]`

---

### Task 7: Draw icons in the viewport

**Files:**
- Modify: `crates/ui/src/app.rs`
- Test: manual + existing suite

**Contracts:** At `LodLevel::Full`, draw the cached icon texture in place of the label character, keeping the category color as the background fill. `Medium` and `Minimal` are unchanged.

**Constraints:**
- **LOD must not regress.** Icons draw at `Full` only; `Medium`/`Minimal` keep flat color so large blueprints stay fast.
- When `IconCache::get` returns `None`, render exactly today's output (color + label char). The fallback path must remain reachable and correct.
- `render_viewport` is already ~230 lines inside a 519-line file. Extract the per-entity draw into a helper rather than growing the function further.

**Verification:**
Run: `build-lock cargo test -p factorio-ui`
Then launch against a headless compositor on the desktop machine and screenshot:
```bash
ssh desktop 'cd <checkout> && cargo build --release'
# run under cage/sway headless, capture with grim, confirm icons render
```
Expected: assembler/belt/inserter icons visible at high zoom; flat colors at low zoom; no missing-icon panics.

**Commit after passing.** `[Mode: Direct]`

---

## Execution
**Skill:** Subagent Dev
- Mode A tasks (1, 2, 5, 7): orchestrator implements directly
- Mode B tasks (3, 4, 6): dispatched to subagents

**Ordering:** 1 → 2 → (3, 4 in sequence — 4 builds on 3's CLI) → 5 → 6 → 7. Task 5 gates on 3+4 producing output; Task 7 gates on 6.

**Note for the implementing session:** the reference dump is in the scratchpad path given in Task 5 and is *not* committed (27.8 MB). If it's gone, regenerate with `ssh desktop '<install>/bin/x64/factorio --dump-data --dump-prototype-locale'` — it is mod-free and carries no achievement risk.
