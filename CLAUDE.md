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
│   ├── templates/          # Template library (stub)
│   ├── solver/             # Layout composition (stub)
│   └── ui/                 # egui frontend — viewport, colors, tooltips
├── docs/
│   ├── factorio-solver-plan.md   # Full concept/architecture doc
│   └── plans/              # Session-specific implementation plans
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

**Phase 3: Basic UI** — COMPLETE. Blueprints visualized in native egui window with pan, zoom, tooltips, entity colors.

No active phase. Next logical step: Phase 4 (templates crate, or solver work).

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

---

## Doc Management

This project splits documentation to minimize context usage. Follow these rules:

### File layout

| File | Purpose | When to read |
|------|---------|-------------|
| `CLAUDE.md` (this file) | Project identity, structure, patterns, current phase pointer | Auto-loaded every session |
| `.claude/phases/current.md` | Phase status and next steps (no active phase currently) | Read when starting new work |
| `.claude/phases/NNN-name.md` | Archived phases (completed) | Only if you need historical context |
| `docs/factorio-solver-plan.md` | Full concept/architecture doc with all phases and technical details | Reference for architecture decisions, entity data structures, solver design |

### Phase transitions

When a phase is completed:

1. **Condense** — extract lasting decisions from `.claude/phases/current.md` (architecture choices, patterns established, conventions) and add them to the "Decisions from previous phases" section above. Keep each to 1-2 lines.
2. **Archive** — rename `.claude/phases/current.md` to `.claude/phases/NNN-name.md` (e.g., `001-blueprint-foundation.md`)
3. **Start fresh** — create a new `.claude/phases/current.md` from `~/.claude/phase-template.md`
4. **Update this file** — update the "Current Phase" section above
5. **Prune** — remove anything from this file that was phase-specific and no longer applies

### What goes where

- **This file**: project-wide truths (stack, structure, patterns, conventions). Things that are true regardless of which phase you're in.
- **Phase doc**: goals, requirements, architecture decisions, implementation notes, and anything specific to the current body of work.
- **Concept doc** (`docs/factorio-solver-plan.md`): full architecture reference — crate details, data structures, phased build order, technical risks.
- **Process rules**: delegation and modularization standards live in `~/.claude/process.md` (global, not per-project).
