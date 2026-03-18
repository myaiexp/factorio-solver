# Blueprint Crate (Phase 1) Implementation Plan

**Goal:** Implement `factorio-blueprint` crate — decode and encode Factorio blueprint strings with round-trip fidelity.

**Architecture:** Four modules (error, types, codec, lib) plus a CLI binary. Data structures use `serde` with `#[serde(flatten)]` extra fields to preserve unknown/modded content through round-trips. Complex sub-structures (connections, control_behavior, wires) use `serde_json::Value` — full typing deferred to later phases.

**Tech Stack:** Rust 1.93.1, edition 2024. Dependencies already in Cargo.toml: serde, serde_json, base64 0.22, flate2, thiserror 2.

---

### Task 1: Error types + core data structures [Mode: Direct]

**Files:**
- Create: `crates/blueprint/src/error.rs`
- Create: `crates/blueprint/src/types.rs`
- Modify: `crates/blueprint/src/lib.rs`

**Contracts — `error.rs`:**
- `BlueprintError` enum with `thiserror::Error` derive
- Variants: `MissingVersionByte`, `UnsupportedVersion(char)`, `Base64(#[from] base64::DecodeError)`, `Zlib(#[from] std::io::Error)`, `Json(#[from] serde_json::Error)`, `InvalidData(String)`

**Contracts — `types.rs`:**
- `Position { x: f64, y: f64 }` — Serialize/Deserialize
- `Color { r: f64, g: f64, b: f64, a: Option<f64> }` — skip_serializing_if None
- `SignalId { name: String, signal_type: Option<String> }` — `signal_type` renamed to `"type"` in JSON
- `Icon { index: u32, signal: SignalId }`
- `Direction` enum (North=0 through NorthWest=7) — custom Serialize/Deserialize as u8, Default=North
- `Entity` — all fields from spec, `entity_type` renamed to `"type"`, direction default+skip when North, `#[serde(flatten)] extra: HashMap<String, Value>` for round-trip
- `Tile { name: String, position: Position }`
- `Blueprint` — item, label, label_color, description, icons, entities (default vec, always serialize), tiles, wires/schedules/snap_to_grid/absolute_snapping/position_relative_to_grid as optional, version, `#[serde(flatten)] extra`
- `BlueprintBook` — item, label, label_color, description, icons, blueprints (Vec<BlueprintBookEntry>), active_index, version, `#[serde(flatten)] extra`
- `BlueprintBookEntry { index: u32, blueprint: Blueprint }`
- `BlueprintData` — top-level envelope struct with `blueprint: Option<Blueprint>`, `blueprint_book: Option<BlueprintBook>` (not an enum — matches JSON shape naturally)

**Key design decisions:**
- `skip_serializing_if` on all Option fields and empty Vecs (except `entities` — always emitted)
- `#[serde(flatten)] extra` on Entity, Blueprint, BlueprintBook for unknown field preservation
- Direction serialized as u8, skipped when North (matches Factorio behavior)
- Complex fields (`connections`, `control_behavior`, `items`, `wires`, `schedules`, `tags`) as `Option<serde_json::Value>`

**Test Cases:**

```rust
// Direction serde
fn test_direction_serializes_as_u8()        // East -> 2
fn test_direction_deserializes_from_u8()    // 4 -> South
fn test_direction_default_is_north()        // absent field -> North
fn test_direction_zero_deserializes()       // explicit 0 -> North
fn test_direction_invalid_value_errors()    // 8+ -> error

// Entity round-trip
fn test_entity_with_all_fields()            // populate every field, serialize + deserialize, assert equal
fn test_entity_none_fields_omitted()        // None fields absent from JSON, not null
fn test_entity_unknown_fields_preserved()   // extra JSON fields survive round-trip via flatten
fn test_entity_type_rename()                // "type": "input" <-> entity_type: Some("input")

// Blueprint/BlueprintData
fn test_blueprint_data_with_blueprint()     // {"blueprint": {...}} round-trips
fn test_blueprint_data_with_book()          // {"blueprint_book": {...}} round-trips
fn test_blueprint_entities_always_emitted() // empty entities Vec still produces "entities": []
```

**Verification:**
Run: `cargo test -p factorio-blueprint`
Expected: All tests pass

**Commit after passing.**

---

### Task 2: Codec pipeline [Mode: Direct]

**Files:**
- Create: `crates/blueprint/src/codec.rs`
- Modify: `crates/blueprint/src/lib.rs` (add module + re-exports)

**Contracts — `codec.rs`:**
- `decode(blueprint_string: &str) -> Result<BlueprintData, BlueprintError>` — strip version byte → base64 decode → zlib decompress → JSON parse
- `decode_to_json(blueprint_string: &str) -> Result<String, BlueprintError>` — same but returns raw JSON string (for pretty-printing/debugging)
- `encode(data: &BlueprintData) -> Result<String, BlueprintError>` — JSON serialize → zlib compress (level 9) → base64 encode → prepend `'0'`
- Version byte: `'0'` (only supported value)
- Base64: `BASE64_STANDARD` (with padding, matching Factorio)

**Contracts — `lib.rs`:**
- Re-export: `decode`, `decode_to_json`, `encode`, `BlueprintError`, and all types from `types.rs`

**Test Cases:**

```rust
// Error paths
fn test_decode_empty_string()           // -> MissingVersionByte
fn test_decode_bad_version()            // "1..." -> UnsupportedVersion('1')
fn test_decode_invalid_base64()         // "0!!!" -> Base64 error
fn test_decode_invalid_zlib()           // "0" + valid base64 of garbage -> Zlib error
fn test_decode_invalid_json()           // "0" + valid base64 of valid zlib of "not json" -> Json error

// Happy path
fn test_manual_encode_decode_roundtrip()
    // Build BlueprintData by hand -> encode -> decode -> assert_eq
fn test_manual_minimal_blueprint()
    // Hand-craft JSON '{"blueprint":{"item":"blueprint","entities":[],"version":0}}'
    // Manually zlib + base64 + prepend '0'
    // decode() it, verify fields match
```

**Verification:**
Run: `cargo test -p factorio-blueprint`
Expected: All tests pass

**Commit after passing.**

---

### Task 3: Integration tests with real blueprints [Mode: Delegated]

**Files:**
- Create: `crates/blueprint/tests/roundtrip.rs`

**Contracts:**
- `assert_roundtrip(s: &str)` helper — decode, encode, decode again, assert struct equality
- `assert_json_equivalent(s: &str)` helper — decode_to_json original vs decode→encode→decode_to_json, parse both as Value, assert equal

**Test Cases (each using a real Factorio blueprint string):**

```rust
fn test_single_belt()                   // minimal: one transport belt
fn test_assembler_setup()               // assembler + inserters + belts, tests recipe & direction
fn test_underground_belts()             // tests entity "type": "input"/"output"
fn test_blueprint_with_tiles()          // stone brick / concrete tiles
fn test_blueprint_book()                // book with 2-3 blueprints
fn test_complex_with_circuits()         // circuit connections, modules, control_behavior (Value fields survive)
```

Each test calls both `assert_roundtrip` and `assert_json_equivalent`.

**How to get test strings:** Copy from Factorio in-game export or factorioprints.com. Store as `const` string literals in the test file.

**Constraints:**
- Do NOT compare encoded strings byte-for-byte (compression is non-deterministic)
- Compare decoded structs (PartialEq) and parsed JSON Values

**Verification:**
Run: `cargo test -p factorio-blueprint`
Expected: All tests pass, including integration tests

**Commit after passing.**

---

### Task 4: CLI binary [Mode: Direct]

**Files:**
- Modify: `crates/blueprint/Cargo.toml` (add `[[bin]]` section)
- Create: `crates/blueprint/src/main.rs`

**Contracts:**
- `factorio-blueprint decode <string>` — prints pretty-printed JSON to stdout
- `factorio-blueprint roundtrip <string>` — decodes then re-encodes, prints blueprint string to stdout
- Bad usage / errors → stderr + exit code 1
- No clap dependency — use `std::env::args()` manual parsing

**Verification:**
Run: `cargo run -p factorio-blueprint -- decode <paste a real blueprint string>`
Expected: Pretty JSON output matching the blueprint

Run: `cargo run -p factorio-blueprint -- roundtrip <paste a real blueprint string>`
Expected: A valid blueprint string that Factorio can import

**Commit after passing.**

---

## Execution
**Skill:** superpowers:subagent-driven-development
- Mode A tasks (Direct): Tasks 1, 2, 4 — Opus implements directly
- Mode B tasks (Delegated): Task 3 — dispatched to subagent (needs to source real blueprint strings from the web)
