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
│   ├── solver/             # Layout composition (stub — re-exports templates)
│   └── ui/                 # egui frontend — viewport, culling, LOD, colors, tooltips
```

### Crate Dependency Graph

```
ui → solver → templates → grid → blueprint
```

Each crate is independently testable. UI is the thinnest layer — all logic lives in lower crates.

## Key Patterns

- **Blueprint string format**: version byte (`0`) + base64 + zlib + JSON. Round-trip fidelity is critical.
- **Entity positions**: center-based with 0.5 offsets for odd-width, integer for even-width entities.
- **Direction enum**: Factorio 2.0 16-direction scheme — North=0, East=4, South=8, West=12 (0–15 total).
- **Entity data**: 85 vanilla + Space Age prototypes, data-driven from `crates/grid/data/prototypes.json` (loaded once via `OnceLock`) — belts, inserters, assemblers, furnaces, pipes, poles, chests, combinators, defense, power, mining, logistics, Space Age buildings.
- **Spatial index**: 16×16-cell chunk buckets (`grid/spatial.rs`) back `query_rect`/`get_neighbors` so range queries scale with the queried area, not total entity count.
- **Crate naming**: `factorio-blueprint`, `factorio-grid`, `factorio-templates`, `factorio-solver`, `factorio-ui` (directory names shortened to `blueprint/`, `grid/`, etc.)

---

## Current Phase

No active phase. The workspace compiles and tests green (`cargo test --workspace`). What actually exists today:

- **blueprint** — Factorio blueprint string codec (version byte + base64 + zlib + JSON) with round-trip fidelity, plus a CLI.
- **grid** — 2D spatial engine: placement/collision, chunk-based spatial index, A* routing (`find_path`), ASCII render, blueprint `import`/`export`, entity classification (`EntityCategory`), and the data-driven prototype registry.
- **templates** — template _extraction_ from a grid region (`extract_template`), the `Template`/`TemplateEntity`/`IoPoint`/`IoRole` model, and JSON persistence (`save_to_json`/`load_from_json`). There is **no** built-in template library or UI browser (previously documented but never implemented).
- **solver** — stub; re-exports `factorio_templates`. A recipe database and production-chain calculator were declared but never implemented (removed; see backlog).
- **ui** — egui viewport with pan/zoom, frustum culling, level-of-detail rendering (`lod.rs`), entity coloring, and hover tooltips.

Next logical step: solver crate — recipe database, production-chain calculator, and template-based layout composition.

> **Note (2026-07):** A code audit found the committed HEAD referenced ~10 phantom module/data files (`spatial`, `astar`, `lod`, `recipe`, `calculator`, `control_behavior`, `wire_extraction`, `prototypes.json`, `to_blueprint`) that were documented as complete but had never been committed to any branch — the workspace did not compile. The engine pieces the tests actually exercise (spatial index, A*, LOD, prototypes registry, grid→blueprint export, `EntityCategory` declaration) were reconstructed; the unconsumed recipe/calculator/wire modules were stripped and backlogged.

### Decisions from previous phases

- **Blueprint envelope**: `BlueprintData` uses Option fields (not enum) to match JSON shape; wraps `Blueprint` and `BlueprintBook`
- **Loose typing for complex fields**: `connections`, `control_behavior`, `items`, `wires`, `schedules` typed as `Option<serde_json::Value>` — full typing deferred
- **Unknown field preservation**: `#[serde(flatten)] extra: HashMap<String, Value>` on Entity, Blueprint, BlueprintBook for round-trip fidelity
- **Direction serialization**: serialized as u8, omitted when North (matches Factorio's own behavior)
- **Sparse grid**: `HashMap<(i32, i32), CellState>` — cells only exist when occupied, unbounded coordinates
- **Tombstone removal**: entity vec uses `Option<PlacedEntity>`, removed entities become None, IDs never reused; O(1) live count via counter
- **Graceful import**: unknown entity prototypes are skipped (collected as `SkippedEntity`) rather than failing the whole blueprint
- **85 entity prototypes**: data-driven registry loaded from `crates/grid/data/prototypes.json` via `serde_json` + `OnceLock` — base game (assemblers, inserters, belts, furnaces, splitters, underground belts, pipes, poles, chests, turrets, power, mining, logistics, combinators) + Space Age DLC (turbo belts, biochamber, recycler, foundry, electromagnetic plant, cryogenic plant, heating tower)
- **Spatial index + A***: `Grid` holds a 16×16 chunk `SpatialIndex` for O(area) range queries; `find_path` is a bounded 4-directional A* over unoccupied cells (occupied = wall; endpoints always walkable)
- **Template extraction**: `extract_template` copies entities overlapping a grid rectangle into a `Template`, remapping positions to a `(0,0)` origin; `IoPoint`/`IoRole` describe boundary connections (filled in via UI). No built-in template library exists yet.
- **Grid → Blueprint export**: `to_blueprint(grid, label, version)` rebuilds a `Blueprint` from live entities (center position, direction, recipe, type preserved), enabling grid→string round-trips

---

## Doc Management

This project splits documentation to minimize context usage. Follow these rules:

### File layout

| File                           | Purpose                                                        | When to read                                                  |
| ------------------------------ | -------------------------------------------------------------- | ------------------------------------------------------------- |
| `CLAUDE.md` (this file)        | Project identity, structure, patterns, current phase pointer   | Auto-loaded every session                                     |
| `phases/current.md`    | Symlink → active phase file                                    | Read when starting phase work                                 |
| `phases/NNN-name.md`   | Phase files (active via symlink, completed ones local-only)    | Only if you need historical context                           |
| `ideas.md`             | Future feature ideas, tech debt, and enhancements              | When planning next phase or brainstorming                     |
| `.claude/plans/`               | Design docs and implementation plans from brainstorming        | When implementing or reviewing designs                        |
| `.claude/references/`          | Domain reference material (specs, external docs, data sources) | When you need domain knowledge                                |
| `.claude/references/factorio-solver-plan.md` | Full concept/architecture doc with all phases and tech details | Reference for architecture decisions, data structures, solver |
| `.claude/[freeform].md`        | Project-specific context docs (architecture, deployment, etc.) | As referenced from this file                                  |

### Phase transitions

When a phase is completed:

1. **Condense** — extract lasting decisions from the active phase file and add to "Decisions from previous phases". Keep each to 1-2 lines.
2. **Archive** — remove the `current.md` symlink. The completed phase file stays but is no longer committed.
3. **Start fresh** — create a new numbered phase file from `~/.claude/phase-template.md`, then symlink `current.md` → it.
4. **Update this file** — update the "Current Phase" section above.
5. **Prune** — remove anything from this file that was phase-specific and no longer applies.

### What goes where

- **This file**: project-wide truths (stack, structure, patterns, conventions). Things that are true regardless of which phase you're in.
- **Phase doc**: goals, requirements, architecture decisions, implementation notes, and anything specific to the current body of work.
- **Concept doc** (`.claude/references/factorio-solver-plan.md`): full architecture reference — crate details, data structures, phased build order, technical risks.
- **Process rules**: delegation and modularization standards live in `~/.claude/process.md` (global, not per-project).
