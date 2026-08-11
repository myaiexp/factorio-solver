# Phase 4: Game Data Foundation — Design

> Replace the hand-written entity registry with data ingested from the player's own
> Factorio install, and render real game icons in the viewport.

**Date:** 2026-08-11
**Status:** Approved (architecture), pending implementation plan
**Depends on:** `blueprint`, `grid` (existing)

---

## Context

The app is a **native egui desktop app** and stays that way. It runs on the same PC as
the game, on a second monitor. A web frontend was considered and rejected — see
"Alternatives rejected".

Today `crates/grid/data/prototypes.json` holds **85 hand-written entity prototypes** and
there is no recipe data at all. The solver crate is a stub. Everything downstream
(calculator, generator) needs real recipe and entity data, and the viewport currently
renders colored rectangles because it has no icons.

**Hard constraint from the user: no mods.** Mods disable Steam achievements, and this is
an achievement run. Every mechanism below is mod-free.

---

## Verified facts

All confirmed against the live install (Factorio **2.0.77**, linux64, Steam, full Space
Age: `base` + `elevated-rails` + `quality` + `space-age`). These are measured, not assumed.

| Fact | Value |
| --- | --- |
| Dump command | `factorio --dump-data` → `~/.factorio/script-output/data-raw-dump.json` (27.8 MB) |
| Locale command | `factorio --dump-prototype-locale` → `entity-locale.json`, `recipe-locale.json`, … |
| Icon sprites | `--dump-icon-sprites` exists but is **not needed** (see Icons) |
| Dump top-level shape | dict of ~250 **prototype-type** keys (`"assembling-machine"`, `"furnace"`, `"recipe"`, …), each mapping name → object. Not a flat entity list. |
| Recipes in dump | **659** |
| Recipes missing `energy_required` | **63** (must default to `0.5`) |
| Recipes missing `enabled` | **242** (must default to **`true`** — see warning) |
| Recipes missing `category` | **152** (must default to `"crafting"`) |
| Recipes with non-array `ingredients` | **12** — `{}` or `null` (incl. the real `biter-egg`) |
| Recipes `hidden` / `parameter` | 319 hidden (kept) / 10 parameter (skipped) |
| Placeable entities | **169** via `flags ∋ "player-creation"` && `selection_box != null` |
| Entities with single `icon` | 162 |
| Entities with layered `icons` | 7 |
| Icon files on disk | 580 in `data/base/graphics/icons/` alone |
| Icon file layout | 153 of 156 are 120×64 mipmap strips (64+32+16+8); **3 are plain 64×64** |
| `icon_size` declared | **1 of 169** — defaults to 64 for the rest |
| Icon path format | `__<mod>__/…` → `<install>/data/<mod>/…`; **156/156 resolve, zero missing** |
| Icon path location | not always `graphics/icons/` — some are entity sprites (`graphics/entity/one-way-valve/…`), so the declared path must be used verbatim |
| Locale shape | `{"names": {name: display}, "descriptions": {...}}` |
| Locale coverage | **100%** — 169/169 entities and 659/659 recipes have display names |
| `fluid_connections` consumers | **none** — declared on the struct, read by no code (see scoping note) |
| Install path | `/home/mse/.local/share/Steam/steamapps/common/Factorio` — note the username is `mse` on the gaming desktop, **not** the `mase` used on the dev VPS. Different machines; this is not a typo. |
| `energy_usage` format | strings: `"150kW"`, `"2000kW"`, `"0kW"` |
| Belt throughput derivation | `speed × 480` (`0.03125 × 480 = 15` ✓ matches existing hand-written value) |
| Tile-size derivation | explicit `tile_width`/`tile_height` when declared (3 entities), else `ceil(collision_box − 0.01)` — reproduces 80 of the 85 committed entries |
| Wrong sizes in the committed table | **5** Space Age buildings (see below) |

Dumping does **not** touch saves or achievements — it loads prototypes in a separate
process and exits. `--mod-directory PATH` can point at an empty dir to guarantee clean
vanilla data.

---

## Architecture

Three pieces, no change to the crate dependency graph
(`ui → solver → templates → grid → blueprint`).

### 1. Ingestion tool — `crates/dump-ingest` (new binary crate)

A standalone binary, **run manually** when the game updates. Deliberately not a build
step: the build must not depend on a Factorio install being present, or on a 27.8 MB
input that never belongs in git.

```
dump-ingest --dump <data-raw-dump.json> --locale-dir <dir> --out-prototypes <path> --out-recipes <path>
```

Reads the dump, emits two committed JSON files. The raw dump stays out of the repo.

**Entity selection is derived, not listed:** `flags ∋ "player-creation" && selection_box != null`.
No hardcoded entity list to maintain — a game update that adds entities picks them up on
the next run.

Seven of the 169 are editor/debug-only prototypes that legitimately carry the flag:
`bottomless-chest`, `infinity-chest`, `infinity-pipe`, `infinity-cargo-wagon`,
`electric-energy-interface`, `proxy-container`, `dummy-rail-support`. They are harmless to
include — the grid can hold them and blueprint import already skips unknowns gracefully —
and this note exists so a later reader doesn't mistake them for a filter bug and "fix" it.

**`dump-ingest` depends on `grid` and `solver` for their types.** It serializes the real
`EntityPrototype` and `Recipe` structs rather than hand-writing the output JSON shape.
Otherwise the emitted file and the consuming serde definitions can drift, and the only
symptom is the existing `.expect()` panic at runtime. This makes the ingest crate the one
place that depends "upward", which is acceptable for a dev tool outside the app's runtime
graph.

### 2. Data model

**Prototypes stay in `grid`** (it already owns entity geometry). The existing
`EntityPrototype` struct is extended **additively**, so every field is `#[serde(default)]`
and no existing `grid` code changes:

| Existing | Source in dump |
| --- | --- |
| `name` | `.name` |
| `tile_width` / `tile_height` | explicit `tile_width`/`tile_height` if declared, else `ceil(collision_box − 0.01)` — see "Tile-size derivation" |
| `crafting_speed` | `.crafting_speed` |
| `power_kw` | parsed from `.energy_usage` (`"150kW"` → `150.0`) |
| `module_slots` | `.module_slots` |
| `belt_throughput` | `.speed × 480` (belt-family types only) |
| `fluid_connections` | `.fluid_boxes` (8 entities) **and** `.fluid_box` (14) — best-effort, see below |

| New | Source in dump | Needed by |
| --- | --- | --- |
| `display_name` | `entity-locale.json.names[name]` | UI labels |
| `icon_path` | `.icon` / `.icons[0].icon` (mod-relative) | Icon rendering |
| `crafting_categories` | `.crafting_categories` | Calculator (machine ↔ recipe match) |
| `underground_max_distance` | `.max_distance` | Generator (belt routing) |
| `pickup_position` / `insert_position` | `.pickup_position` / `.insert_position` | Generator (inserter reach) |

#### Tile-size derivation

Neither naive rule works, and this was validated against the dump rather than assumed:

| Candidate rule | Result vs the 85 committed entries |
| --- | --- |
| `round(selection_box)` | 6 mismatches — breaks `train-stop` (selection 1.8×1.8, really 2×2) |
| `ceil(collision_box)` | 7 mismatches — breaks `train-stop` (collision 1×1, really 2×2) |
| **explicit `tile_*` else `ceil(collision_box − 0.01)`** | **5 differences, all of them corrections** |

Three prototypes declare `tile_width`/`tile_height` outright, and those are authoritative:
`train-stop` (2×2 despite a 1×1 collision box, because trains pass through it) and
`offshore-pump` (1×1 despite a 1.2×1.98 selection box). Everything else falls back to the
collision box. The `− 0.01` guards float noise — collision extents in the dump carry values
like `2.39999999999999996447…`.

#### `fluid_connections` is best-effort, deliberately

`grep` confirms nothing outside `prototype.rs` reads `fluid_connections` — it is declared
on the struct and populated for 4 entities, and no code consumes it.

**The existing hand-written values are not a target to reproduce.** They are internally
inconsistent, so no single transform maps the dump onto them:

| Entity | Committed | Dump | Note |
| --- | --- | --- | --- |
| `chemical-plant` | 4 conns, input at `dx 0.0, dy −1.5` | 4 boxes, input at center-relative `[−1,−1]` | implies `x+1` |
| `oil-refinery` | 5 conns, input at `dx −1.0, dy 2.5` | 5 boxes, input at `[−1, 2]` | implies `x+0` — contradicts the above |
| `pump` | 2 | 2 | count agrees |
| `storage-tank` | **1** | **4** | count is simply wrong in the committed file |

**Decision:** derive from the dump using **one documented convention** — the raw
center-relative `pipe_connections[].position`, reading both field shapes (`fluid_boxes`
plural on 8 entities, `fluid_box` singular on 14). Do not attempt to match the old numbers
and do not sink time into a "correct" coordinate transform: with no consumer there is
nothing to validate against, and Phase 6 (the generator) is the first code that will care.

**Expected test change:** `storage-tank` goes 1 → 4 connections, so
`test_fluid_connections_chemical_plant`'s sibling assertions need updating. `chemical-plant`
(4), `oil-refinery` (5) and `pump` (2) keep their counts;
`test_no_fluid_connections_belt` stays green (belts have no fluid boxes).

#### The committed table has five wrong footprints

The five remaining differences are **bugs in the current hand-written data**, not in the
derivation — every one is a Space Age building, i.e. exactly what someone would guess at
without the game open. All base-game entities match.

| Entity | Committed (wrong) | Derived (correct) |
| --- | --- | --- |
| `big-mining-drill` | 3×3 | **5×5** |
| `foundry` | 4×4 | **5×5** |
| `biolab` | 4×4 | **5×5** |
| `rocket-turret` | 2×2 | **3×3** |
| `recycler` | 2×2 | **2×4** |

This is not cosmetic: a wrong footprint means wrong collision detection and misplaced
entities when importing any blueprint containing these buildings. Phase 4 fixes it as a
side effect of deriving the data instead of writing it by hand.

**Recipes go in `solver`** — recipes are solver-domain, and `grid` has no use for them:

```rust
pub struct Recipe {
    pub name: String,
    pub display_name: Option<String>,
    pub category: String,               // absent in dump → "crafting" (152 recipes)
    pub energy_required: f64,           // seconds; absent → 0.5 (63 recipes)
    pub ingredients: Vec<ItemAmount>,   // {} or null → empty (see Lua empty-table note)
    pub results: Vec<ItemAmount>,
    pub enabled: bool,                  // absent → TRUE (242 recipes) — see warning
    pub hidden: bool,                   // 319 recipes; kept, not filtered
}

pub struct ItemAmount {
    pub name: String,
    pub kind: ItemKind,                 // Item | Fluid
    pub amount: f64,
}
```

#### Recipe field defaults — the dominant correctness risk in this phase

Most recipe fields are **omitted** rather than defaulted in the dump, and Rust's natural
`#[serde(default)]` gives the wrong answer for the most important one. Measured counts:

| Field | Absent | Factorio default | Trap |
| --- | --- | --- | --- |
| `enabled` | **242 / 659 (37%)** | **`true`** | `#[serde(default)]` on `bool` yields `false` — the **opposite**. Would silently mark 242 recipes research-locked, including `iron-plate`, `copper-plate`, `iron-gear-wheel`, `transport-belt` and `stone-furnace`. |
| `category` | 152 / 659 (23%) | `"crafting"` | Absent on real recipes such as `bulk-inserter`, not just cosmetic ones. |
| `energy_required` | 63 / 659 | `0.5` | Already noted above. |

**`enabled` must not use `#[serde(default)]`.** Use `#[serde(default = "default_true")]` or
`Option<bool>` resolved to `true`. This is the single most damaging silent failure available
in this phase: it type-checks, it round-trips, and it quietly makes a third of the game
look unresearched.

#### Empty Lua tables serialize as `{}`, not `[]`

Factorio dumps an empty Lua table as `{}` because it cannot distinguish an empty array from
an empty map. So `ingredients`/`results` are **not reliably arrays**:

| Recipe(s) | `ingredients` | Nature |
| --- | --- | --- |
| `biter-egg` | `{}` | **A real recipe** — captive-spawner-process, 10s, yields 5 biter eggs, genuinely ingredient-free |
| `recipe-unknown` | `{}` | Placeholder, `hidden: true` |
| `parameter-0` … `parameter-9` (10) | `null` | UI signal placeholders, `parameter: true` |

Deserialization must accept **array, `{}`, or `null`** for these fields and yield an empty
vec. Handle it as a general rule, **not** by special-casing `biter-egg` by name — any future
ingredient-free recipe hits the same path. `#[serde(default)]` alone is insufficient: it
covers a *missing* key, not a key present with the wrong JSON type, so `{}` still hard-fails
a `Vec<ItemAmount>`.

**Filtering policy:** skip the 10 `parameter: true` recipes (not craftable). **Keep** the 319
`hidden: true` recipes — Space Age's recycling recipes are hidden but real, and a
recycler-aware solver needs them. Expose the flag so the UI can filter its own picker.

Loaded via `OnceLock` from a committed `recipes.json`, mirroring the existing prototype
registry pattern.

### 3. Icons — `crates/ui`

Icons are read **directly from the install at runtime**. Not dumped, not committed.

Rationale: the app runs on the same machine as the game, so the files are already there.
This avoids committing Wube's copyrighted art to a repo licensed 0BSD, keeps binaries out
of git, needs no dump step, and automatically matches the user's version and DLC.

Pipeline:

1. **Install detection** — probe known Steam/standalone paths; a config file entry
   overrides when detection fails.
2. **Path resolution** — strip the `__<mod>__` prefix and map it to `<install>/data/<mod>/`,
   keeping the rest of the declared path verbatim. Do **not** assume `graphics/icons/`:
   three icons point at entity sprites such as `graphics/entity/one-way-valve/one-way-valve-east.png`.
   Verified against the live install: **156/156 paths resolve, zero missing**.
3. **Decode + crop** — `image` crate; take the leftmost `icon_size × icon_size` square,
   where `icon_size` comes from the prototype and **defaults to 64** (only 1 of 169 declares
   it). This single rule covers both observed layouts: it strips the mipmap tail from the
   153 files that are 120×64, and is a no-op on the 3 that are already 64×64. If the image
   is smaller than `icon_size`, use it whole rather than cropping.
4. **Layered icons (7 entities)** — composite the layers in order; on any failure fall
   back to layer 0.
5. **Texture cache** — lazily upload to egui textures keyed by prototype name.
6. **Fallback** — missing install, missing file, or decode failure renders today's colored
   rectangle. Icons are an enhancement, never a hard dependency.

LOD is preserved: icons draw at `LodLevel::Full` only; `Medium` and `Minimal` keep flat
color, so large blueprints don't regress.

---

## Data flow

```
Factorio install ──(manual, on game update)──> factorio --dump-data
                                                      │
                                                      ▼
                                            data-raw-dump.json (27.8 MB, not committed)
                                                      │
                                                  dump-ingest
                                                      │
                                    ┌─────────────────┴──────────────────┐
                                    ▼                                    ▼
                    grid/data/prototypes.json              solver/data/recipes.json
                          (169 entities, committed)          (659 recipes, committed)
                                    │                                    │
                                    ▼                                    ▼
                          grid::prototype registry            solver::recipe registry
                                    │
                                    ▼
                        ui ──> icons read live from <install>/data/<mod>/graphics/icons/
```

---

## Error handling

- **Ingestion is strict and loud.** It is a manual developer tool, run rarely; a silent
  partial write would poison every downstream phase. Unknown/malformed prototypes abort
  with the offending name. Entities missing a required field are reported by name, not
  skipped silently.
- **Runtime loading is strict.** Malformed committed JSON is a programming error — the
  existing registry already panics via `.expect()` on bad JSON, and that stays.
- **Icons are best-effort.** Every failure path degrades to the colored rectangle. A
  missing install must never prevent the app from starting.
- **Install detection failure is surfaced**, not swallowed: the UI states that icons are
  unavailable and names the config key to set.

---

## Testing

- **Regression against the current 85, minus five pinned corrections.** The ingestion must
  reproduce the existing hand-written `tile_width`/`tile_height`/`belt_throughput` for **80**
  of the 85 current entries, and must produce the corrected footprints for the five listed
  above. Both halves are asserted explicitly — the 80 catch a broken derivation rule, and
  the 5 are pinned by name so a future rule change can't silently revert them. This is the
  primary correctness check; it already caught two wrong candidate rules during design.
- **Recipe field defaults, by named case** — these are the phase's highest-risk paths:
  - `energy_required` absent → `0.5`, asserted on `electronic-circuit`.
  - `enabled` absent → **`true`**, asserted on `iron-plate` and `transport-belt`.
    **Not** `electronic-circuit` — it declares `"enabled": false` explicitly, so it is a
    valid `energy_required` case but the wrong case for this one. (`jq`'s `//` operator
    treats `false` as null-ish, which is what originally hid this; use `has()`.) Also
    assert the aggregate: of the 649 kept recipes exactly **326** end up `enabled == false`
    and 323 do not (91 explicit `true` + 232 absent), so a regression to `bool::default()`
    — which would report 558 locked — fails loudly rather than subtly.
  - `category` absent → `"crafting"`, asserted on `bulk-inserter`.
- **Non-array `ingredients`/`results`** — `{}`, `null`, and a populated array all
  deserialize, yielding an empty vec for the first two. Asserted on `biter-egg` (a real
  recipe: 10s, 5 results, zero ingredients), not just on the `recipe-unknown` placeholder.
- **Filtering** — the 10 `parameter: true` recipes are absent from the output; the 319
  `hidden: true` recipes are present with the flag preserved.
- **Fixture-based unit tests** — a small checked-in dump fragment (a handful of entities
  and recipes) drives ingestion tests without the 27.8 MB file.
- **Count assertions** — for 2.0.77: **169** entities, and **649** recipes in the output
  (659 in the dump minus the 10 `parameter` ones). A canary for a filter that silently
  stops matching.
- **`energy_usage` parsing** — `"150kW"`, `"2000kW"`, `"0kW"`.
- **Icon path resolution** — `__base__/x.png` and `__space-age__/x.png` map to the right
  install subdirectory, and a non-`graphics/icons/` path survives verbatim.
- **Icon crop** — unit-tested on both observed layouts: a synthetic 120×64 strip (crops to
  the leftmost 64×64) and a 64×64 square (passes through untouched), plus an undersized
  image (used whole, not cropped).
- **UI verification** — run against a headless compositor on the desktop machine and
  screenshot with `grim`, so nothing appears on the user's monitors.

---

## Out of scope

- Recipe→machine assignment logic and the production-chain calculator (Phase 5).
- Any layout generation (Phase 6).
- Reading live game state (what's built, research progress, inventory). The only mod-free
  route is parsing `level.dat` inside the save zip — undocumented binary that changes shape
  most versions. Rejected as fragile.
- Quality tiers, module/beacon effects.
- Mod support. Comes free later — dumping with mods enabled yields modded data — but is not
  a Phase 4 goal.

---

## Alternatives rejected

- **Web frontend (Rust core → wasm + TS).** Considered seriously; rejected because the
  user wants it on a second monitor on the same PC as the game, native needs no rewrite of
  the working phase-3 viewport, and only native can read the install directly. The option
  stays open: all logic lives in the lower crates and those four are wasm-clean today (no
  `fs`, no `Instant`, no I/O).
- **Committing an icon atlas (~1–2 MB).** Works on a fresh clone, but puts Wube's art in a
  0BSD-licensed repo and needs regenerating per game version. Reading from the install is
  strictly better here.
- **`--dump-icon-sprites`.** Redundant with reading the install, and produces thousands of
  files.
- **Hand-writing ~50 recipes** (the original concept doc's plan). 659 are available for
  free and stay correct across patches.
- **Ingestion as a build step.** Would make the build depend on a Factorio install.
