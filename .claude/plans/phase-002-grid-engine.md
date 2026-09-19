# Phase 2: Grid Engine

> Implement the `factorio-grid` crate — place entities on a 2D grid with collision detection, built from decoded blueprints.

## Goals

- Define entity prototypes with physical properties (size, collision box)
- Build a 2D grid from decoded blueprints with entity placement
- Implement collision detection and spatial queries
- Produce ASCII representation of placed layouts

## Requirements

_What "done" looks like:_

- Hardcoded entity prototype registry for ~20 core vanilla entities (assemblers, inserters, belts, furnaces, splitters, underground belts, pipes, etc.)
- `Grid` struct that manages a spatial map of placed entities with collision tracking
- `can_place(entity, position, direction) -> bool` — collision check against occupied cells
- `place(entity, position, direction) -> Result<EntityId>` — place with validation
- `remove(entity_id)` — remove entity and free cells
- `from_blueprint(blueprint) -> Result<Grid>` — build grid from a decoded blueprint
- `bounding_box()` — smallest rectangle containing all entities
- Spatial queries: get entity at position, get neighbors within radius
- ASCII grid renderer for debugging/testing
- Tests using real blueprints: decode → build grid → verify entity placement

## Architecture / Design Notes

- **Sparse HashMap grid**: `HashMap<(i32, i32), CellState>` — cells only exist when occupied, unbounded coordinates
- **Position mapping**: `top_left = ((center - size/2.0).round() as i32)` — handles odd-width (0.5 centers) and even-width (integer centers) correctly
- **Rotation**: Non-square entities (splitters 2x1, combinators 1x2) swap width/height on East/West
- **Tombstone removal**: `Vec<Option<PlacedEntity>>` — removed entities become None, IDs never reused
- **O(1) entity_count**: live counter maintained on place/remove
- **Graceful import**: Unknown prototypes are skipped (collected as `SkippedEntity`) rather than failing, so real-world blueprints with modded entities still work
- **Shared validation**: `validate_placement()` extracts prototype lookup + bounds checking shared by `can_place` and `place`

## Notes

**Phase 2 complete.** All requirements met:

- 33 vanilla entity prototypes registered (exceeds the ~20 target)
- Grid with placement, collision detection, removal, bounds enforcement
- `from_blueprint()` imports real Factorio blueprints — all 4 test blueprints import with 0 skips
- ASCII renderer maps entity types to characters for debugging
- 60 tests (54 unit + 6 integration), all passing
- Public API with clean re-exports from lib.rs
- Code review applied: deduped validation logic, O(1) counts, Debug derives, removed double lookup
