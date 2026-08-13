# Factorio Layout Solver

> Native desktop app that generates Factorio blueprints from high-level goals — user specifies what to build, the solver handles spatial layout and belt routing.

## Stack

- **Language**: Rust (workspace with multiple crates)
- **GUI Framework**: egui (immediate-mode native GUI)
- **Target Platform**: Linux (Arch), cross-platform via Rust
- **Key Dependencies**: serde, serde_json, base64, flate2 (zlib)

## Project Structure

```
factorio-solver/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── blueprint/          # Blueprint string parsing/encoding + CLI
│   ├── grid/               # 2D spatial engine: placement, collision, spatial index, A*, import/export
│   ├── templates/          # Template extraction from grid regions + IoPoint model + JSON persistence
│   ├── save/               # Factorio save reader: level-init.dat + the player force's unlocked recipes
│   ├── solver/             # Recipe registry + chain calculator + block generator
│   ├── dump-ingest/        # Dev tool: Factorio data dump -> prototypes.json + recipes.json
│   └── ui/                 # egui frontend — viewport, culling, LOD, colors, tooltips
```

`dump-ingest` is a manually-run developer tool, deliberately outside the app's
runtime graph — it depends "upward" on `grid` and `solver` so it serializes their
real structs rather than a hand-written JSON shape that could drift. The app
builds and runs with no Factorio install present.

### Crate Dependency Graph

```
ui → solver → templates → grid → blueprint
       └────→ save
```

Each crate is independently testable. UI is the thinnest layer — all logic lives in lower crates.

`save` depends on no workspace crate. The edge points solver→save rather than
the reverse because save's calibration invariant needs the default-enabled
recipe set, which lives in solver's registry — so save takes it as a parameter
and the graph stays acyclic.

## Running It

`cargo run -p factorio-ui` from the workspace root, or `scripts/launch.sh` — the
same thing, but it fast-forwards a clean `master` to `origin/master` and rebuilds
in release only when the binary is stale, so a desktop entry always starts the
newest code. `scripts/install-desktop-entry.sh` writes that entry (paths derived
from the checkout, so it is re-runnable and machine-independent). The window's
`app_id`, the entry's basename, and its `StartupWMClass` are all
`factorio-solver` and must stay equal — that triple is what pairs the window
with the launcher icon on Wayland.

## Where Sessions Run

**On the desktop, not the VPS** (`projects.execution='remote-native'`, `machine=desktop`,
`remote_path=/home/mse/Projects/factorio-solver`). Helm SSH-spawns `claude -p` there in a
worktree under `.worktrees/<id>`. The reason is not disk: the deliverable is an egui window
that reads icons live from a Factorio install and is verified against real saves, so on the
VPS every UI change was unverifiable in principle. Full rationale, including the tooling the
desktop needed and what it still lacks (`rtk`): `.claude/plans/2026-08-13-remote-native-desktop-design.md`.

Two consequences that bite:

- **`deploy` does not exist on the desktop.** Land by hand: commit on the session branch,
  merge to `master` in the main checkout, `git push origin`. No service, no update logging.
- **Helm's archive pre-flight fails open for remote projects**, so nothing refuses to archive
  a session holding unlanded commits — it deletes the branch. Land before you finish, every
  time; there is no net behind you here.

`.helmcontext` bridges `target/debug` from the main checkout into each worktree, so sessions
share one 2.1 GB build cache instead of 4.6 GB each (idea #3381, three VPS disk alerts).
Release is deliberately **not** bridged — `scripts/launch.sh` runs `target/release/factorio-ui`
behind the desktop icon, and sharing it would let a session's half-built binary launch as master.

**Fallback when the desktop is down** (it reaches the VPS over a reverse SSH tunnel — no
tunnel, no sessions): the VPS clone at `~/Projects/solvers/factorio-solver` is kept for this.
`UPDATE projects SET execution='local' WHERE name='factorio-solver';` switches to it —
`path` and `remote_path` are separate columns — and `'remote-native'` switches back. Fetch
first; neither checkout pulls automatically. A VPS session can do all crate logic and the
full suite (`real_save.rs` early-returns without `FACTORIO_SAVE_FIXTURE`), but cannot run the
UI, read icons, or test a save — and it builds its own 4.6 GB target, since `worktreeBridge`
is remote-only.

## The Build Gate

`scripts/install-hooks.sh` (once per clone) sets `core.hooksPath` to the tracked
`scripts/hooks/`, so committing runs `cargo clippy --workspace --all-targets --
-D warnings` and `cargo test --workspace` — about 5s warm. `--no-verify`
bypasses it.

pre-commit first refuses **untracked or unstaged** files, and that is the part
that matters: the 2026-07 break was ~10 modules that existed on disk but in no
commit, so the workspace built for its author and not for anyone else. Checking
the working tree only means something once the working tree is provably what the
commit contains.

Which hook covers which of `deploy`'s land paths was **measured, not assumed**:
a fast-forward fires nothing (the tree is the branch tip's, already gated), a
`--no-ff` merge fires `pre-merge-commit`, and a **cherry-pick fires no hook at
all** — so a single commit replayed onto a moved base is the one ungated path,
and `pre-push` warns when it sees a tree nothing verified. `pre-push` does no
building on purpose: `deploy` allows a push 15 seconds, an earlier version ran
the full gate there, and it killed a real deploy — the branch landed on master
and the push never happened.

The gate remembers **trees**, not commits (`scripts/hooks/stamp.sh`, stored in
the shared git dir). A merge onto an unmoved master is a new sha over the same
tree pre-commit just built, so the land costs nothing instead of a cold rebuild
in the main checkout.

Both cargo invocations pass `--locked`, and **`Cargo.lock` is tracked** (the
scaffold `.gitignore` ignored it, which is the library default; this workspace
ships a binary). The two go together: cargo rewrites the lockfile during
*resolution*, before it compiles and long after pre-commit's unstaged check has
passed — so staging a manifest change alone used to land the old lockfile
beside a build done against the new one, a commit describing a dependency graph
nothing verified. `--locked` makes cargo refuse instead, and the hook prints the
remedy (`cargo metadata … && git add Cargo.lock`) since cargo's own advice is to
drop the flag.

## Key Patterns

- **Blueprint string format**: version byte (`0`) + base64 + zlib + JSON. Round-trip fidelity is critical.
- **Entity positions**: center-based with 0.5 offsets for odd-width, integer for even-width entities.
- **Direction enum**: Factorio 2.0 16-direction scheme — North=0, East=4, South=8, West=12 (0–15 total).
- **Game data is derived, never hand-written**: `crates/grid/data/prototypes.json` (169 entities), `crates/solver/data/recipes.json` (649 recipes) and `crates/solver/data/technologies.json` (275 technologies) are generated by `crates/dump-ingest` from a mod-free `factorio --dump-data` dump of the player's own install (currently 2.0.77 + Space Age). Regenerate all three together on a game update; never edit them by hand. Each loads once via `OnceLock`.
- **Icons are read live from the install**, never dumped or committed — this keeps Wube's art out of a 0BSD repo and automatically matches the user's version and DLC.
- **Spatial index**: 16×16-cell chunk buckets (`grid/spatial.rs`) back `query_rect`/`get_neighbors` so range queries scale with the queried area, not total entity count.
- **Crate naming**: `factorio-blueprint`, `factorio-grid`, `factorio-templates`, `factorio-solver`, `factorio-ui` (directory names shortened to `blueprint/`, `grid/`, etc.)

---

## Current Phase

**Phase 9 — Recipe Availability Gate** (complete; see
`.claude/plans/2026-08-13-recipe-availability-gate-design.md` and
`phases/009-recipe-availability-gate.md`). The app goes goal → plan → grid →
pasteable blueprint string end to end, and restricts itself to what the player
can actually build — from a save file, from a hand-edited tick list, or both,
since the two fill the same recipe-name set. A refusal names the technology to
research. The chain panel's inputs survive a restart.

> **Verified against a real save 2026-08-13.** The reader opens the player's
> current save (2.0.77) and decodes 659 recipes, 275 technologies and 369
> unlocked recipes — matching the ground truth in
> `crates/save/tests/real_save.rs` exactly. Getting there took three fixes
> (idea #3399): entries are nested under a folder named after the save, the
> `zip` crate needs its non-default `deflate` feature because Factorio
> compresses `level-init.dat`, and every `ZipError` was being reported as
> `MissingEntry`, which hid the second behind the first. Run it yourself with
> `FACTORIO_SAVE_FIXTURE=<a save> cargo test -p factorio-save`.
>
> One piece is still wrong: `SaveFile::mods()` parses empty, because a
> variable-length scenario section sits between the version header and the mod
> list (idea #3400, which carries the decoded layout). Contained by design —
> `init.rs` locates the id tables by search, so a mod-list mismatch corrupts
> only `mods()`, and that is used solely to answer "is this vanilla".

What exists today:

- **blueprint** — Factorio blueprint string codec (version byte + base64 + zlib + JSON) with round-trip fidelity, plus a CLI. `ItemFilter` + `Entity.use_filters`/`filters` mirror Factorio's `BlueprintItemFilter` for filtered inserters.
- **grid** — 2D spatial engine: placement/collision, chunk-based spatial index, A* routing (`find_path`), ASCII render, blueprint `import`/`export`, entity classification (`EntityCategory`), per-entity filter slots (`PlacedEntity.filters` / `Grid::set_filters`, read and written on both the import and export sides), and the dump-derived prototype registry (169 entities).
- **templates** — template _extraction_ from a grid region (`extract_template`), the `Template`/`TemplateEntity`/`IoPoint`/`IoRole` model, and JSON persistence (`save_to_json`/`load_from_json`). There is **no** built-in template library or UI browser (previously documented but never implemented).
- **save** — `SaveFile::open` reads a save zip's `level-init.dat` (version, mod names, prototype id tables) and inflates its `level.dat<N>` chunks lazily in numeric order; `unlocked_recipes(&default_enabled)` locates the player force's recipe-unlock array by calibration search. Entry names are resolved through the save's own directory, which Factorio names after the save. No workspace dependency, and `testsupport::FixtureSave` builds synthetic save zips — nested and deflated the way Factorio writes them — so the tests need no real save.
- **solver** — the dump-derived recipe (649) and technology (275) registries, `availability` (the `Availability` model, `allows`/`allows_machine`, and `from_save`, the save→set bridge), `tech` (the unlock graph, for explaining a refusal — never for selecting), `chain` (`solve(&ChainGoal) -> ProductionPlan`, gated recipe/machine selection, the rate solver) and `layout` (`generate(&ProductionPlan, &LayoutConfig) -> Grid`): `lane` (the far-lane rule), `cell` (sizing a cell from belt throughput, ingredient lanes and product belts alike), `place` (one cell's entities, product inserters filtered when a step has two products), `tile` (cells into bands), `power`, and pre-emit validation including a per-product delivered-rate check and a mixed-belt check.
- **dump-ingest** — the manual ingest tool that generates all three data files.
- **ui** — egui viewport with pan/zoom, frustum culling, level-of-detail rendering (`lod.rs`), entity coloring, hover tooltips, and the chain panel (`chain_panel/`) with the save picker, the editable "Available recipes" tick list, belt/pole/inserter/topology controls, Generate + copy-to-clipboard; `clipboard/` watches the system clipboard so an in-game export loads with no paste; `persist.rs` saves the panel's inputs and the app settings between runs.

Next logical step: belt routing *between* steps (idea #3362) — the generator
stacks a producer directly above its consumer but does not connect them, so the
player wires the block by hand. `crates/grid/src/astar.rs` already has
`find_path`. See `phases/current.md` for the other candidates.

> **Note (2026-07):** A code audit found the committed HEAD referenced ~10 phantom module/data files (`spatial`, `astar`, `lod`, `recipe`, `calculator`, `control_behavior`, `wire_extraction`, `prototypes.json`, `to_blueprint`) that were documented as complete but had never been committed to any branch — the workspace did not compile. The engine pieces the tests actually exercise (spatial index, A*, LOD, prototypes registry, grid→blueprint export, `EntityCategory` declaration) were reconstructed; the unconsumed recipe/calculator/wire modules were stripped and backlogged.

### Decisions from previous phases

- **Blueprint envelope**: `BlueprintData` uses Option fields (not enum) to match JSON shape; wraps `Blueprint` and `BlueprintBook`
- **Loose typing for complex fields**: `connections`, `control_behavior`, `items`, `wires`, `schedules` typed as `Option<serde_json::Value>` — full typing deferred
- **Unknown field preservation**: `#[serde(flatten)] extra: HashMap<String, Value>` on Entity, Blueprint, BlueprintBook for round-trip fidelity
- **Direction serialization**: serialized as u8, omitted when North (matches Factorio's own behavior)
- **Legacy 1.x directions**: `from_blueprint` upgrades 0/2/4/6 cardinals when `directions_look_legacy(dirs, version)` is true — major version `< 2` always upgrades pure `{0,2,4,6}` sets (covers pure-South / N+S-only); major `≥ 2` requires a definitive East/West marker (decoded 2 or 6) so true 2.0 North+East is not rewritten
- **Blueprint book entries**: `BlueprintBookEntry` has optional `blueprint` and optional nested `blueprint_book` (empty index-only slots allowed), matching Factorio's nested-book wire shape
- **Sparse grid**: `HashMap<(i32, i32), CellState>` — cells only exist when occupied, unbounded coordinates
- **Tombstone removal**: entity vec uses `Option<PlacedEntity>`, removed entities become None, IDs never reused; O(1) live count via counter
- **Graceful import**: unknown entity prototypes are skipped (collected as `SkippedEntity`) rather than failing the whole blueprint
- **85 entity prototypes**: data-driven registry loaded from `crates/grid/data/prototypes.json` via `serde_json` + `OnceLock` — base game (assemblers, inserters, belts, furnaces, splitters, underground belts, pipes, poles, chests, turrets, power, mining, logistics, combinators) + Space Age DLC (turbo belts, biochamber, recycler, foundry, electromagnetic plant, cryogenic plant, heating tower)
- **Spatial index + A***: `Grid` holds a 16×16 chunk `SpatialIndex` for O(area) range queries; `find_path` is a bounded 4-directional A* over unoccupied cells (occupied = wall; endpoints always walkable)
- **Template extraction**: `extract_template` copies entities overlapping a grid rectangle into a `Template`, remapping positions to a `(0,0)` origin; `IoPoint`/`IoRole` describe boundary connections (filled in via UI). No built-in template library exists yet.
- **Grid → Blueprint export**: `to_blueprint(grid, label, version)` rebuilds a `Blueprint` from live entities (center position, direction, recipe, type preserved), enabling grid→string round-trips
- **Entity selection is derived, not listed**: `flags ∋ "player-creation" && selection_box != null` yields the 169 placeable entities, so a game update picks up new ones for free. Seven results are editor/debug prototypes that legitimately carry the flag — harmless, and not a filter bug to "fix"
- **Tile size**: explicit `tile_width`/`tile_height` when declared (checked **per axis** — some prototypes declare only one), else `max(1, ceil(collision_box extent − 0.01))`. The −0.01 absorbs dump float noise, the floor of 1 keeps sub-tile colliders at a full tile. Reproduces 80 of the 85 old hand-written entries; the other 5 were wrong Space Age footprints (`big-mining-drill`/`foundry`/`biolab` 5×5, `rocket-turret` 3×3, `recycler` 2×4)
- **`belt_throughput` keys on prototype-type, not on `.speed`** — robots have `.speed` too
- **`power_kw` is consumption only** (`energy_usage`). Generators have none; the old table mixed production into the same field. Deriving generator output is backlogged
- **`fluid_connections` use the dump's raw centre-relative `pipe_connections[].position`**, typed from per-connection `flow_direction` falling back to box-level `production_type`. Both `fluid_boxes` (plural) and `fluid_box` (singular) shapes are read. The old hand-written coordinates were internally inconsistent and are not a reproduction target; `storage-tank` went 1 → 4 connections (the old count was simply wrong)
- **Recipe defaults come from Factorio, not Rust**: `enabled` absent → **`true`** (`#[serde(default)]` on a bool gives the opposite and would mark 242 recipes research-locked), `category` absent → `"crafting"`, `energy_required` absent → `0.5`. `ingredients`/`results` accept an array, an empty object `{}` (Factorio's empty-Lua-table serialization, hit by the real `biter-egg`) or null; a *populated* object is an error
- **Recipe filtering**: the 10 `parameter: true` placeholders are skipped, the 319 `hidden: true` ones are kept with the flag (Space Age recycling recipes are hidden but real)
- **Yields are `amount × probability`, never `amount`** (`ItemAmount::effective_amount`): 103 recipes weight a result, and `uranium-processing` gives both outputs `amount: 1` with the whole 0.007/0.993 split in `probability`. Absent → 1.0; a present-but-wrong-typed or out-of-range value is an ingest error, since defaulting past it turns a 0.7% chance into a certainty
- **`main_product` is a demotion signal, not a selector**: only 8 of 649 recipes declare a non-empty one (Factorio's `""` means "no single main product" and stores as `None`). It can prove a result is *secondary*; it can never identify a primary. Selection rests on the 420 single-result recipes instead — a filter keyed on `main_product == item` would exclude `uranium-processing` from its own outputs
- **The goal declares a boundary, not a recipe**: `ChainGoal.available` is the bus, and resolution walks back from the product until it reaches it. "One assembler" and "the whole chain" are the same code path — only `available` differs. Fluids fall out of this for free: a fluid ingredient not in `available` is an error telling the user to declare that recipe's product instead
- **Recycling is filtered by `category.starts_with("recycling")`, never `== "recycling"`**: 310 recipes are exactly `recycling`, but `scrap-recycling` is `recycling-or-hand-crafting` and outputs 10+ common items. Exact equality makes nearly every common item look ambiguous. Applies to both `chain::select` and the UI recipe picker
- **Ambiguity is always an error, never a guess** (`AmbiguousRecipe` listing candidates, resolved via `recipe_overrides`). 43 items have several producers — `copper-cable`, `iron-plate` and `copper-plate` among them — so the headline green-circuit case needs an override, which the UI offers as a button per candidate
- **The rate solver nets a recipe's own item against itself before dividing** (`chain::solve::net_yield`). That single subtraction is what makes a self-consuming recipe (kovarex: 40 U-235 in, 41 out) resolve in one division instead of iterating toward a limit. Cross-recipe cycles have no closed form and hit the iteration cap as `DidNotConverge` — never a silent partial plan
- **`enabled` is not a selection filter**: research-locked recipes (`uranium-processing`) are legitimate goals
- **Ingest is strict and loud**: a missing required field, an unparseable `energy_usage`, or a present-but-wrong-typed field aborts naming the entity/recipe. A silent partial write from a rarely-run tool would poison every downstream phase
- **`supply_area_distance` is gated on the `electric-pole` prototype type**, not on the key being present: `beacon` declares the same key for its module *effect* radius, and ungated a layout reads a beacon as a 6×6 power pole and emits an unpowered block. Same trap as `belt_throughput` vs. robot `.speed`. It is the supply half-width, never the wire reach (medium pole: 3.5 supply, 9 wire)
- **An inserter drops on the belt's FAR lane, always — there is no near-lane fallback.** Confirmed in-game, and the fact the whole block topology follows from: a belt filled by our own inserters needs a machine column on *both* sides to use both of its lanes, while a belt filled from the bus does not. Stated once, as geometry over two cells, in `layout::lane::drop_lane` — never re-derived from an inserter's `Direction`, which would re-import the orientation question that helper exists to settle. Answers at distance 2 as well as 1, because a long-handed inserter reaching the outer belt of a pair obeys the same rule
- **Inserter orientation is derived from `pickup_position`/`insert_position`**, searched across the four cardinals, never written down. Factorio's unrotated inserter picks from the **north** and drops to the **south**, so an inserter's `direction` points at what it takes *from* — the opposite of most intuitions, and the reason this is derived
- **Pole coverage uses the game's rule — footprint *overlaps* the supply rectangle, not containment.** Requiring containment would make a 3-wide machine impossible to cover with a small pole (reach 2.5 clears 2 cells past the pole's column) and retire small poles for no safety gain. Each pole is scored by its own prototype's reach, not the config's
- **The layout refuses fluids itself**: `chain::solve` only rejects a fluid *ingredient* that is off the bus, so a fluid the user declared available — or any fluid a recipe *produces* — reaches the layout looking like an ordinary item rate and would be belted
- **Ingredient lanes are per cell; product lanes are per column** — picking shares a lane, dropping owns one. So `cell_cap` divides `2 × belts` ingredient lanes among the whole cell while `column_cap` gives each column the product side's belt count outright, and `machines_per_cell = min(floor(cell_cap), 2 × floor(column_cap))`. Both binding cases fall out of the same `min` with no special case: 45/s green circuits is input-bound on copper cable at 15 machines/cell, and the cable feeding it is output-bound on its own 2× yield at 14
- **Sizing starts at the belt, never at `machines_needed`** — a belt's throughput caps how many machines a column can feed, so the old direction (machines → how many belts?) is what produced belts provisioned at twice their reachable capacity
- **Lane allocation is exhaustive search, never proportional-with-rounding**: at most 4 lanes among at most 4 ingredients, so searching for the maximum is trivially cheap — and the rounding rule proportional allocation would need is exactly where the arithmetic would quietly stop being optimal. Ties go to the earlier ingredient, for determinism
- **A machine gets one inserter per (stream, *belt*), not per stream**: an ingredient allocated 3 of a cell's 4 lanes sits on one belt outright plus a shared lane of the next one out, and a machine drawing it from the near belt alone would get 2 of the 3 lanes the sizing promised. Symmetrically, each machine drops onto *every* product belt, because `column_cap` assumes the column's output spreads across all of them
- **`CellTopology` is configuration because middle-feed and outer-feed have opposite strengths**: sharing the spine helps an input-bound step, sharing the edge an output-bound one, and which a step is depends on the recipe. Moving copper cable's product to the wider side turns 14 machines/cell into 30
- **A cell column reserves a pole row every `2 × floor(supply_area_distance) / mh` machines.** A *vertical* pole column is impossible — every column in a cell is load-bearing, since an inserter must sit beside its machine and a belt at exactly slot-0 or slot-1 distance from its gutter — so a horizontal row cut through the machine columns is the only place a pole can ever stand. Without it the flagship green-circuit step (3 ingredient inserters against a 3-tall machine) leaves both ingredient gutters solid for the column's full height and `place_poles` fails the whole block. Derived from the configured pole, never hardcoded; costs height, never throughput
- **A second product gets a whole belt, never a second lane** (idea #3377): a column reaches only each belt's far lane, so a belt cannot be split between two items the way an ingredient belt can. Products claim whole belts through the *same* search that divides lanes among ingredients (`allocate_lanes`, belts and lanes being the same unit on a side a column reaches once), and each product's output inserters carry a **filter** — without one an inserter takes whatever is in the output slot and both belts get a random mix, which is the row topology's original bug. `spine_belts`/`edge_belts` cap at 2, so three or more products still refuse (`TooManyProductsForBelts`), and no topology widens past it
- **Product belts are addressed physically — ascending x across the group — never by a column's own distance from its gutter.** The two columns face the group from opposite sides, so slot `k` reaches belt `spine_x0 + k` for one and `spine_x0 + s - 1 - k` for the other; with the two belts a two-product step needs, that reversal has no fixed point. Addressing by slot puts both products on both belts. `place_cell` converts physical index to each column's own slot (`if dir > 0 { p } else { n - 1 - p }`); the same reversal governs the edge belt shared between adjacent cells. Folding that conversion back into `CellPlan::product_belts` makes it column-specific and silently wrong for whichever column did not drive the change
- **A mixed-belt check keys on the belt *run*, never on a tile or a `(run, lane)` pair** — the same trap as #3364, and it was measured recurring: with the mirror bug reintroduced, the two columns drop different products onto one belt column at different *rows* and on different *lanes*, so both narrower keys report nothing and the whole suite passes green. A belt's two lanes are one belt here, because a downstream inserter picks from both
- **`validate::is_machine` keys on the prototype's `crafting_categories`, not on `EntityCategory`.** `EntityCategory` is name-substring matching ("assembling", "furnace", "chemical", "refinery") and misses nine real crafting machines — centrifuge, foundry, biochamber, electromagnetic-plant, cryogenic-plant, recycler, crusher, rocket-silo, captive-biter-spawner — so both the connectivity and delivered-rate checks silently skipped every block built on one. Discovered because `uranium-processing`'s own centrifuge was invisible to the checks meant to validate it. `EntityCategory` is a rendering taxonomy; semantic questions go to the dump-derived prototype data
- **`use_filters` must be emitted alongside `filters`** — Factorio defaults it to `false`, so a filter array on its own is stored and ignored, giving an inserter that reads as filtered in the blueprint and grabs anything in game. `index` is 1-based; `quality`/`comparator` are omitted so the filter matches *any* quality (pinning `"normal"` makes the inserter refuse a legendary output once quality modules are in the machine). Taken from `lua-api.factorio.com/2.0.77`, the version the registries were built from, and pinned key-for-key by a test in `export.rs`
- **`BLUEPRINT_VERSION` stamps 2.0.77 into every generated blueprint**: `from_blueprint` reads the major version to decide whether directions are 1.x-encoded, so a too-low stamp gets our own 2.0 directions rewritten on re-import
- **Layout validation re-derives rather than trusts**: the connectivity and delivered-rate checks recompute each inserter's reach from prototype data and deliberately do not share `place`'s rotation helper — a bug in a shared helper would pass both the placement and the check meant to catch it
- **Delivered capacity is counted per belt *run*, never per belt tile**: a whole run carries one lane's throughput in total and every inserter along it shares that, so a per-tile count would scale with belt length and let every block pass trivially — which is precisely how the half-rate bug shipped
- **The unlock array's stride and offset are never hardcoded, and the calibration invariant must not be weakened.** Stride was measured at 7 on 2.0.8/2.0.28/2.0.32 and 6 on 2.0.60/2.0.77; the offset varies per save *within* one version (+67, +181, +134). The layout is found by searching for the `(stride, offset)` where **every** default-enabled recipe decodes as enabled — which yields exactly one hit on every 2.0 save measured. A weaker check ("the common starting recipes are present") is satisfied by an alignment off by exactly one record, and the investigation's first decode failed in precisely that way and rationalised its 38-recipe residual as correct gating. Zero candidates is `CalibrationFailed`, more than one is `CalibrationAmbiguous`; there is no best-guess fallback
- **`default_enabled` is a parameter of `save`, never something it reads**: the set lives in `solver`'s registry, so reading it inside `save` would invert the crate graph. This is the whole reason `save` has no workspace dependency
- **Availability filters inside `select_recipe`, never inside `candidates_for`.** `solve` uses `candidates_for(&item).is_empty()` as its *raw-resource* test, so filtering there would make an intermediate whose only recipes are locked look like iron ore and get silently billed to `plan.inputs` — a silent wrong answer instead of a loud one. The asymmetry that hides it: the goal item has its own emptiness check, so only intermediates would be corrupted. Filtering after the count is also a gain — a locked alternative no longer makes an item look ambiguous, so availability sometimes *resolves* an `AmbiguousRecipe`
- **A locked *named* machine is an error (`MachineLocked`), a locked *fallback* candidate is skipped**: the fastest-available search silently picking assembling-machine-2 over a locked 3 is the point of the feature, while a machine the user named is a claim worth surfacing — the same rule as a named machine that cannot craft the category. `MachineLocked` is deliberately distinct from `NoMachineForCategory`, whose remedy ("pick a machine for that category") is wrong advice when the category is fine
- **An explicit recipe override wins even over a locked recipe** — an override is a deliberate user statement, and the override path bypasses candidate selection entirely
- **`level-init.dat`'s category-table format is measured; its header is not.** `[u8 len][category][u16 count]` then `count` × `[u8 len][name][u16 id]` parsed on ~50 real saves from 1.0.0 to 2.0.77. The version field and mod-list encoding in front of it were never confirmed, which is why the recipe/technology tables are located by **searching** for their length-prefixed category name rather than by an offset the header parse computed — a wrong header assumption can then only corrupt `mods()`, never the id tables
- **Chunks are ordered by numeric suffix, never lexicographically** (`level.dat10` sorts before `level.dat2` as a string), and inflated lazily — the force's array completes inside the first chunk on every save measured (1 of 62 through 1 of 132), so inflating the whole stream would do 60–130× the needed work. That deliberately makes the total byte count unknowable, which is why the `level.datmetadata` size check is an explicit `verify_total_size` rather than a load precondition
- **Availability is a set of recipe *names*, never of technologies**: a save exposes per-recipe unlocked flags while researched technologies are not readable, so a hand edit, a save import and a future projection all produce the same thing. That single decision is what let two independently-built gates (phases 8 and 9) merge instead of one replacing the other
- **"No recipe" and "no recipe you can build" must stay distinguishable**: `solve`'s raw-resource test keeps calling the *ungated* `candidates_for`. Gated in place, a locked intermediate two levels down reports as a bus requirement — a plan telling the player to belt 45/s of a Vulcanus casting product, looking entirely successful. `available_candidates_for` is a second function for exactly this reason, and each call site is explicit about which question it asks
- **`Availability::allows` ORs with `Recipe::enabled`**: without it, unticking one recipe silently locks the 323 the game grants at the start. Free for the save source, whose own calibration invariant asserts a save marks every default-enabled recipe on — pinned by `from_save`'s `an_imported_set_contains_every_default_enabled_recipe`
- **The technology graph explains, it never gates**: `tech::unlockers_for` turns "you cannot build this" into "this needs `foundry`". Nothing selects on it, which is why 112 bonus-only technologies are ingested anyway (prerequisite chains run through them) and why a dangling unlock edge is dropped while a dangling *prerequisite* is a hard error
- **`Everything` excludes what no playthrough can reach**, derived from the graph rather than a list of the eight names — but this changes no chain result, since all eight are also `hidden: true` and `candidates_for` had always dropped them. What it protects is the UI's seeded tick-list. (The design doc claims otherwise; it is wrong)
- **An override outranks availability, in `solve` as well as in `select_recipe`**: an override already bypasses the hidden/recycling/main-product filters, so naming a locked recipe means the user meant it. `solve` defers through `locked_without_an_override` rather than refusing first — the two disagreed once, and `solve` won the race
- **Editing the set is subtractive**: entering "only what I can build" seeds it from what is already available, so the switch alone changes no result and the first edit is a removal. A save import replaces the set wholesale and hand edits then correct it — one set, three sources, never two constraints that can disagree
- **Persist inputs, never derived state**: `result`/`generated`/`pending_grid` come from a solve, and restoring them beside restored inputs they no longer match is a lie on screen. Restore is lenient by construction (`#[serde(default)]` at container level, unknown fields ignored, `Default` routed through `ChainPanel::new()`) so adding a field later cannot wipe a saved setup
- **The clipboard watcher is gated on what the viewport shows, never on where a string came from**: `FactorioApp::displayed` holds the blueprint string currently on screen whatever loaded it, and a candidate equal to it is declined. That one rule covers both re-copying something already loaded and the self-copy loop — "Copy blueprint" hands the watcher the generated block's own string while the viewport is showing that block. A guard written against the copy button specifically would need re-adding at every future call site that writes to the clipboard
- **A clipboard load never writes `blueprint_input`**: showing the user what arrived would also overwrite a string they were part-way through typing, and a background poller must not be able to destroy typing. Both load paths go through `load_string`, differing only in the `LoadSource` they report
- **`arboard` needs its non-default `wayland-data-control` feature**: plain Wayland hands the clipboard only to the *focused* surface, so without it the watcher reads nothing while the game has focus — which is the entire use case. egui-winit already pulls arboard in without it; the ui crate re-declares it to switch the feature on for the shared copy
- **Clipboard reads happen off the UI thread**: both arboard backends round-trip to another process, and a stall there is a dropped frame. The thread offers candidates over a channel and calls `request_repaint`, so an idle app still notices; `poll` returns the *newest* of a backlog, since replaying them would flash each blueprint through the viewport on the way to the one the user actually copied

---

## Doc Management

This project splits documentation to minimize context usage. Follow these rules:

### File layout

| File                           | Purpose                                                        | When to read                                                  |
| ------------------------------ | -------------------------------------------------------------- | ------------------------------------------------------------- |
| `CLAUDE.md` (this file)        | Project identity, structure, patterns, current phase pointer   | Auto-loaded every session                                     |
| `phases/current.md`    | Index: which phase is active, what is done, what is next       | Read when starting phase work                                 |
| `phases/NNN-name.md`   | One file per phase, kept after completion                      | Only if you need historical context                           |
| `ideas.md`             | Future feature ideas, tech debt, and enhancements              | When planning next phase or brainstorming                     |
| `.claude/plans/`               | Design docs and implementation plans from brainstorming        | When implementing or reviewing designs                        |
| `.claude/references/`          | Domain reference material (specs, external docs, data sources) | When you need domain knowledge                                |
| `.claude/references/factorio-solver-plan.md` | Full concept/architecture doc with all phases and tech details | Reference for architecture decisions, data structures, solver |
| `.claude/[freeform].md`        | Project-specific context docs (architecture, deployment, etc.) | As referenced from this file                                  |

### Phase transitions

When a phase is completed:

1. **Condense** — extract lasting decisions from the active phase file and add to "Decisions from previous phases". Keep each to 1-2 lines.
2. **Archive** — move the phase out of `current.md`'s "Next up" into its "Completed phases" list. The phase file stays.
3. **Start fresh** — create the next numbered phase file and point `current.md` at it.
4. **Update this file** — update the "Current Phase" section above.
5. **Prune** — remove anything from this file that was phase-specific and no longer applies.

### What goes where

- **This file**: project-wide truths (stack, structure, patterns, conventions). Things that are true regardless of which phase you're in.
- **Phase doc**: goals, requirements, architecture decisions, implementation notes, and anything specific to the current body of work.
- **Concept doc** (`.claude/references/factorio-solver-plan.md`): full architecture reference — crate details, data structures, phased build order, technical risks.
- **Process rules**: delegation and modularization standards live in `~/.claude/process.md` (global, not per-project).
