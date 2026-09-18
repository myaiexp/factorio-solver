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
│   ├── repo-docs/          # Tests only: CLAUDE.md size limits (no app code)
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

`cargo run -p factorio-ui`, or `scripts/launch.sh` behind the desktop entry
(fast-forwards a clean `master`, rebuilds release only when stale). The window's
`app_id`, the entry's basename and its `StartupWMClass` must all stay
`factorio-solver`. Details: `docs/running-and-sessions.md`.

## Where Sessions Run

**On the desktop, not the VPS** (`projects.execution='remote-native'`): the
deliverable is an egui window verified against a real install and real saves,
which the VPS cannot do. Rationale and setup: `docs/running-and-sessions.md`.

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

**Fallback when the desktop is down**: the VPS clone at
`~/Projects/solvers/factorio-solver`, switched with one `UPDATE projects` — fetch
first; it cannot run the UI or read icons. Steps: `docs/running-and-sessions.md`.

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

Every land path is gated except a cherry-pick, which `pre-push` only warns about.
`pre-push` must never build: `deploy` gives a push 15 seconds. Which hook fires
on which path: `docs/build-gate.md`.

The gate remembers **trees**, not commits (`scripts/hooks/stamp.sh`, stored in
the shared git dir). A merge onto an unmoved master is a new sha over the same
tree pre-commit just built, so the land costs nothing instead of a cold rebuild
in the main checkout.

Both cargo invocations pass `--locked` and **`Cargo.lock` is tracked** — after a
manifest change run `cargo metadata … && git add Cargo.lock`; never drop the
flag. Why: `docs/build-gate.md`.

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

> The save reader is verified against a real 2.0.77 save
> (`FACTORIO_SAVE_FIXTURE=<a save> cargo test -p factorio-save`). `SaveFile::mods()`
> still parses empty (idea #3400) — contained, since it only answers "is this
> vanilla". Details: `docs/save-and-availability.md`.

What exists today:

- **blueprint** — Factorio blueprint string codec (version byte + base64 + zlib + JSON) with round-trip fidelity, plus a CLI. `ItemFilter` + `Entity.use_filters`/`filters` mirror Factorio's `BlueprintItemFilter` for filtered inserters.
- **grid** — 2D spatial engine: placement/collision, chunk-based spatial index, A* routing (`find_path`), ASCII render, blueprint `import`/`export`, entity classification (`EntityCategory`), per-entity filter slots (`PlacedEntity.filters` / `Grid::set_filters`, read and written on both the import and export sides), and the dump-derived prototype registry (169 entities).
- **templates** — template _extraction_ from a grid region (`extract_template`), the `Template`/`TemplateEntity`/`IoPoint`/`IoRole` model, and JSON persistence (`save_to_json`/`load_from_json`). There is **no** built-in template library or UI browser (previously documented but never implemented).
- **save** — reads a save's `level-init.dat` and the player force's unlocked recipes by calibration search; no workspace dependency, and `testsupport::FixtureSave` builds synthetic saves. Detail: `docs/crates.md`.
- **solver** — the recipe and technology registries, `availability`, `tech`, `chain` (`solve`) and `layout` (`generate`, plus pre-emit validation). Module map: `docs/crates.md`.
- **dump-ingest** — the manual ingest tool that generates all three data files.
- **ui** — egui viewport, the chain panel (save picker, tick list, Generate + copy), the clipboard watcher, persistence and the build stamp. Module map: `docs/crates.md`.

Next logical step: belt routing *between* steps (idea #3362) — the generator
stacks a producer directly above its consumer but does not connect them, so the
player wires the block by hand. `crates/grid/src/astar.rs` already has
`find_path`. See `phases/current.md` for the other candidates.

> **Note (2026-07):** a code audit found ~10 documented modules that had never been committed, so the workspace did not compile; the build gate exists because of it. Full note: `docs/build-gate.md`.

### Decisions from previous phases

Each topic's decisions live in its subdoc, which is the record. Add new
decisions there, not here; a new topic doc gets one pointer below.

- **Blueprint codec and grid** → `docs/blueprint-and-grid.md`. Round-trip fidelity comes first: unknown fields survive via `#[serde(flatten)] extra`, `use_filters` is always emitted beside `filters`, and `BLUEPRINT_VERSION` never drops below 2.0, or re-import rewrites our directions as 1.x. Prototype fields are derived from the dump and keyed on the prototype *type*, never on a key being present.
- **Recipes and the chain solver** → `docs/chain-solver.md`. Recipe defaults come from Factorio, not Rust; yields are `amount × probability`; recycling is `category.starts_with("recycling")`; ambiguity is always an error, never a guess; ingest is strict and loud.
- **Block layout** → `docs/layout.md`. An inserter always drops on the belt's far lane; sizing starts at the belt; product belts are addressed physically, never by a column's slot; capacity and mixed-belt checks key on the belt *run*; validation re-derives from prototype data instead of sharing `place`'s helpers.
- **Save reading and availability** → `docs/save-and-availability.md`. The unlock array's stride and offset come from the calibration search, never a constant, and that invariant must not be weakened. Availability filters in `select_recipe`, never in `candidates_for`; an override outranks availability.
- **UI** → `docs/ui.md`. Persist inputs, never derived state; a background poller (clipboard, save reload) must never destroy user input; every `git` subprocess strips `GIT_DIR` and its eight siblings.

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
| `docs/<topic>.md`              | Topic records moved out of this file: decisions, rationale, verified facts | When a pointer here names one                                 |

### Phase transitions

When a phase is completed:

1. **Condense** — extract lasting decisions from the active phase file into the matching `docs/<topic>.md` subdoc, 1-2 lines each. This file gets a pointer only when a new topic doc is created, and must stay within the limits `crates/repo-docs` tests.
2. **Archive** — move the phase out of `current.md`'s "Next up" into its "Completed phases" list. The phase file stays.
3. **Start fresh** — create the next numbered phase file and point `current.md` at it.
4. **Update this file** — update the "Current Phase" section above.
5. **Prune** — remove anything from this file that was phase-specific and no longer applies.

### What goes where

- **This file**: project-wide truths (stack, structure, patterns, conventions). Things that are true regardless of which phase you're in.
- **Topic docs** (`docs/*.md`): the record for each topic this file points to. New detail goes there, never back into this file.
- **Phase doc**: goals, requirements, architecture decisions, implementation notes, and anything specific to the current body of work.
- **Concept doc** (`.claude/references/factorio-solver-plan.md`): full architecture reference — crate details, data structures, phased build order, technical risks.
- **Process rules**: delegation and modularization standards live in `~/.claude/process.md` (global, not per-project).
