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
│   ├── grid/               # 2D spatial engine, entity placement, collision
│   ├── templates/          # Template library — TemplateLibrary, 10+ built-in templates
│   ├── solver/             # Layout composition (stub)
│   └── ui/                 # egui frontend — viewport, colors, tooltips
├── docs/
│   └── factorio-solver-plan.md   # Full concept/architecture doc
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
- **Entity data**: 79 hardcoded vanilla + Space Age prototypes in registry (belts, inserters, assemblers, furnaces, pipes, poles, chests, combinators, defense, power, mining, logistics, Space Age buildings).
- **Crate naming**: `factorio-blueprint`, `factorio-grid`, `factorio-templates`, `factorio-solver`, `factorio-ui` (directory names shortened to `blueprint/`, `grid/`, etc.)

---

## Current Phase

**Phase 4: Built-in Template Library** — COMPLETE. `factorio-templates` crate ships ≥10 built-in production templates (belt balancers, smelter arrays, circuit lines, science packs) with a `TemplateLibrary` registry and egui browser panel.

No active phase. Next logical step: Phase 5 (solver crate — layout composition using templates).

Completed phase details: `.claude/phases/current.md`

### Decisions from previous phases

- **Blueprint envelope**: `BlueprintData` uses Option fields (not enum) to match JSON shape; wraps `Blueprint` and `BlueprintBook`
- **Loose typing for complex fields**: `connections`, `control_behavior`, `items`, `wires`, `schedules` typed as `Option<serde_json::Value>` — full typing deferred
- **Unknown field preservation**: `#[serde(flatten)] extra: HashMap<String, Value>` on Entity, Blueprint, BlueprintBook for round-trip fidelity
- **Direction serialization**: serialized as u8, omitted when North (matches Factorio's own behavior)
- **Sparse grid**: `HashMap<(i32, i32), CellState>` — cells only exist when occupied, unbounded coordinates
- **Tombstone removal**: entity vec uses `Option<PlacedEntity>`, removed entities become None, IDs never reused; O(1) live count via counter
- **Graceful import**: unknown entity prototypes are skipped (collected as `SkippedEntity`) rather than failing the whole blueprint
- **79 entity prototypes**: registered in hardcoded registry — base game (assemblers, inserters, belts, furnaces, splitters, underground belts, pipes, poles, chests, turrets, power, mining, logistics, combinators) + Space Age DLC (turbo belts, biochamber, recycler, foundry, electromagnetic plant, cryogenic plant, heating tower)
- **TemplateLibrary**: static `OnceLock<Vec<Template>>` registry; `all()`, `by_category()`, `find()` APIs; `Template::build_grid()` places entities on a fresh `Grid`
- **Built-in templates (≥10)**: balancer-2-2, balancer-4-4, balancer-8-8, smelter-stone-8, smelter-steel-8, smelter-electric-4, circuit-green-4, circuit-red-2, science-red-2, science-green-2
- **UI template browser**: left `SidePanel` grouped by category; clicking a template loads its grid into the viewport; description tooltip + I/O port counts shown per entry

---

## Doc Management

This project splits documentation to minimize context usage. Follow these rules:

### File layout

| File                           | Purpose                                                        | When to read                                                  |
| ------------------------------ | -------------------------------------------------------------- | ------------------------------------------------------------- |
| `CLAUDE.md` (this file)        | Project identity, structure, patterns, current phase pointer   | Auto-loaded every session                                     |
| `.claude/phases/current.md`    | Symlink → active phase file                                    | Read when starting phase work                                 |
| `.claude/phases/NNN-name.md`   | Phase files (active via symlink, completed ones local-only)    | Only if you need historical context                           |
| `.claude/ideas.md`             | Future feature ideas, tech debt, and enhancements              | When planning next phase or brainstorming                     |
| `.claude/plans/`               | Design docs and implementation plans from brainstorming        | When implementing or reviewing designs                        |
| `.claude/references/`          | Domain reference material (specs, external docs, data sources) | When you need domain knowledge                                |
| `docs/factorio-solver-plan.md` | Full concept/architecture doc with all phases and tech details | Reference for architecture decisions, data structures, solver |
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
- **Concept doc** (`docs/factorio-solver-plan.md`): full architecture reference — crate details, data structures, phased build order, technical risks.
- **Process rules**: delegation and modularization standards live in `~/.claude/process.md` (global, not per-project).
