# Blueprint Codec and Grid Decisions

Lasting decisions about the blueprint string codec, the grid engine, and the dump-derived prototype data.

> The sections below were moved verbatim out of `CLAUDE.md` on 2026-09-18 (idea #4770), which now keeps only a pointer to each. This doc is the record: add new detail here, not to `CLAUDE.md`.

## Decisions

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
- **`use_filters` must be emitted alongside `filters`** — Factorio defaults it to `false`, so a filter array on its own is stored and ignored, giving an inserter that reads as filtered in the blueprint and grabs anything in game. `index` is 1-based; `quality`/`comparator` are omitted so the filter matches *any* quality (pinning `"normal"` makes the inserter refuse a legendary output once quality modules are in the machine). Taken from `lua-api.factorio.com/2.0.77`, the version the registries were built from, and pinned key-for-key by a test in `export.rs`
- **`BLUEPRINT_VERSION` stamps 2.0.77 into every generated blueprint**: `from_blueprint` reads the major version to decide whether directions are 1.x-encoded, so a too-low stamp gets our own 2.0 directions rewritten on re-import
