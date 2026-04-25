# Phase 1: Blueprint Foundation

> Implement the `factorio-blueprint` crate — read and write Factorio blueprint strings with round-trip fidelity.

## Goals

- Decode Factorio blueprint strings into Rust structs
- Encode Rust structs back into valid blueprint strings
- Achieve round-trip fidelity (decode → encode produces functionally identical blueprints)
- CLI tool for testing round-trips

## Requirements

_What "done" looks like:_

- `decode(blueprint_string) -> Result<Blueprint>` handles the full pipeline: strip version byte → base64 decode → zlib decompress → JSON parse → typed Rust structs
- `encode(blueprint) -> Result<String>` handles the reverse pipeline
- All key entity properties are parsed: `name`, `position`, `direction`, `recipe`, `items` (modules), `type` (underground belt input/output), `connections` (circuit wires, can be loosely typed initially)
- Tests pass using real blueprint strings copied from the game
- A small CLI binary (`cargo run -p factorio-blueprint -- decode <string>`) that demonstrates round-tripping

## Architecture / Design Notes

**Blueprint string format:**
1. First byte is version character (currently `0`)
2. Remaining bytes are base64-encoded
3. Decoded bytes are zlib-compressed
4. Decompressed data is JSON

**Key data structures** (see `docs/factorio-solver-plan.md` for full details):
- `Blueprint` — top-level container with label, entities, tiles, icons, version
- `Entity` — entity_number, name, position, direction, recipe, items, connections, type_field
- `Position` — x/y as f64 (center-based, 0.5 offsets for odd-width entities)
- `Direction` — enum: North=0, East=2, South=4, West=6

**Dependencies**: `serde`, `serde_json`, `base64`, `flate2`

**Important**: The JSON structure nests under `{"blueprint": {...}}` at the top level. The `Blueprint` struct represents the inner object. Serde deserialization needs a wrapper or `#[serde(rename)]` to handle this.

**Round-trip considerations**:
- Preserve unknown/extra JSON fields (use `#[serde(flatten)]` with `HashMap<String, Value>` or similar) so blueprints with modded entities or newer fields still survive round-tripping
- Entity ordering in the output doesn't need to match input exactly, but the blueprint should be functionally identical when pasted into Factorio

## Notes

**Phase 1 complete.** All requirements met:

- `decode()`, `decode_to_json()`, `encode()` — full pipeline working
- All entity properties parsed: name, position, direction, recipe, items, type, connections, control_behavior, wires, tags
- Unknown fields preserved via `#[serde(flatten)] extra: HashMap<String, Value>` on Entity, Blueprint, BlueprintBook
- 25 tests: 12 type tests, 7 codec tests, 6 integration round-trip tests
- CLI binary: `decode` (pretty JSON) and `roundtrip` commands
- Complex fields (connections, control_behavior, items, wires, schedules) typed as `Option<serde_json::Value>` — full typing deferred to later phases
- `BlueprintData` envelope uses Option fields (not enum) to match JSON shape naturally
- Direction serialized as u8, skipped when North (matches Factorio behavior)
