# Phase 2: Grid Engine Implementation Plan

**Goal:** Implement the `factorio-grid` crate — a 2D spatial engine that places entities on an integer grid with collision detection, importing layouts from decoded blueprints.

**Architecture:** HashMap-based sparse grid where cells map to entity ownership. Entity prototypes define physical sizes; the grid converts f64 center-based blueprint positions to integer cell coordinates. Unknown entities are gracefully skipped during import so real-world blueprints work out of the box.

**Tech Stack:** Rust, `factorio-blueprint` (path dep), `thiserror` 2

---

### Task 1: Core Types and Error Types [Mode: Direct]

**Files:**
- Create: `crates/grid/src/types.rs`
- Create: `crates/grid/src/error.rs`
- Modify: `crates/grid/Cargo.toml` (add `thiserror = "2"`)

**Contracts:**

```rust
// types.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridPos { pub x: i32, pub y: i32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    Occupied { entity_id: EntityId },
    // Empty = absence from HashMap. Reserved deferred to solver phase.
}

#[derive(Debug, Clone)]
pub struct PlacedEntity {
    pub id: EntityId,
    pub prototype_name: &'static str,
    pub position: GridPos,                        // top-left cell
    pub center: factorio_blueprint::Position,     // original f64 center
    pub direction: factorio_blueprint::Direction,
    pub size: (u32, u32),                         // effective (w, h) after rotation
    pub recipe: Option<String>,
    pub entity_type: Option<String>,              // "input"/"output" for underground belts
}

// error.rs
#[derive(Debug, Error)]
pub enum GridError {
    UnknownPrototype(String),
    Collision { x: i32, y: i32, occupant: EntityId },
    EntityNotFound(EntityId),
    OutOfBounds { x: i32, y: i32, max_x: i32, max_y: i32 },
}
```

**Test Cases:**

```rust
fn test_grid_pos_equality_and_hash() { /* GridPos(1,2) == GridPos(1,2), works as HashMap key */ }
fn test_entity_id_newtype() { /* EntityId(0) != EntityId(1) */ }
fn test_grid_error_display() { /* all variants produce meaningful messages */ }
```

**Verification:**
Run: `cargo test -p factorio-grid`
Expected: All tests pass

**Commit after passing.**

---

### Task 2: Entity Prototype Registry [Mode: Direct]

**Files:**
- Create: `crates/grid/src/prototype.rs`

**Contracts:**

```rust
pub struct EntityPrototype {
    pub name: &'static str,
    pub tile_width: u32,    // width in North orientation
    pub tile_height: u32,   // height in North orientation
}

/// Width/height accounting for rotation. Non-square entities swap on East/West.
pub fn effective_size(proto: &EntityPrototype, direction: Direction) -> (u32, u32);

/// Lookup by entity name. Returns None for unknown entities.
pub fn lookup(name: &str) -> Option<&'static EntityPrototype>;

/// All registered prototype names.
pub fn all_names() -> Vec<&'static str>;
```

**~30 core entities:**

| Entity | W | H | Notes |
|--------|---|---|-------|
| transport-belt, fast-transport-belt, express-transport-belt | 1 | 1 | |
| underground-belt, fast-underground-belt, express-underground-belt | 1 | 1 | |
| splitter, fast-splitter, express-splitter | 2 | 1 | swaps on rotation |
| inserter, fast-inserter, long-handed-inserter, stack-inserter, bulk-inserter | 1 | 1 | |
| assembling-machine-1/2/3 | 3 | 3 | |
| stone-furnace, steel-furnace | 2 | 2 | |
| electric-furnace | 3 | 3 | |
| chemical-plant | 3 | 3 | |
| oil-refinery | 5 | 5 | |
| pipe, pipe-to-ground | 1 | 1 | |
| small-electric-pole, medium-electric-pole | 1 | 1 | |
| big-electric-pole, substation | 2 | 2 | |
| beacon | 3 | 3 | |
| arithmetic-combinator, decider-combinator | 1 | 2 | swaps on rotation |
| constant-combinator | 1 | 1 | |

**Test Cases:**

```rust
fn test_lookup_known_entity() { /* lookup("transport-belt") returns Some with correct size */ }
fn test_lookup_unknown_entity() { /* lookup("modded-thing") returns None */ }
fn test_effective_size_square() { /* 3x3 assembler stays 3x3 in all directions */ }
fn test_effective_size_splitter_rotation() { /* splitter: North=(2,1), East=(1,2) */ }
fn test_effective_size_combinator_rotation() { /* arithmetic: North=(1,2), East=(2,1) */ }
fn test_all_names_count() { /* all_names().len() matches expected ~30 */ }
fn test_all_prototypes_valid() { /* all have tile_width >= 1 and tile_height >= 1 */ }
```

**Verification:**
Run: `cargo test -p factorio-grid`
Expected: All tests pass

**Commit after passing.**

---

### Task 3: Grid Core — Placement and Collision [Mode: Delegated]

**Files:**
- Create: `crates/grid/src/grid.rs`

**Contracts:**

```rust
pub struct Grid {
    cells: HashMap<(i32, i32), CellState>,
    entities: Vec<Option<PlacedEntity>>,  // Option for tombstones on removal
    bounds: Option<(i32, i32, i32, i32)>, // optional placement constraints
}

impl Grid {
    pub fn new() -> Self;
    pub fn with_bounds(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Self;

    // Core placement
    pub fn can_place(&self, prototype_name: &str, center: &Position, direction: Direction) -> Result<bool, GridError>;
    pub fn place(&mut self, prototype_name: &str, center: &Position, direction: Direction, recipe: Option<String>, entity_type: Option<String>) -> Result<EntityId, GridError>;
    pub fn remove(&mut self, id: EntityId) -> Result<PlacedEntity, GridError>;

    // Queries
    pub fn get_at(&self, x: i32, y: i32) -> Option<&PlacedEntity>;
    pub fn get_entity(&self, id: EntityId) -> Option<&PlacedEntity>;
    pub fn entities(&self) -> impl Iterator<Item = &PlacedEntity>;
    pub fn entity_count(&self) -> usize;
    pub fn cell_count(&self) -> usize;
    pub fn bounding_box(&self) -> Option<(GridPos, GridPos)>;
    pub fn get_neighbors(&self, center: GridPos, radius: i32) -> Vec<&PlacedEntity>;
}
```

**Position mapping formula:**
`top_left = ((center_x - width/2.0).round() as i32, (center_y - height/2.0).round() as i32)`

This handles all parity cases:
- 1x1 at (0.5, 0.5) → top_left (0, 0)
- 3x3 at (0.5, 0.5) → top_left (-1, -1)
- 2x2 at (1.0, 1.0) → top_left (0, 0)
- 2x1 splitter at (0.0, 0.5) → top_left (-1, 0)

**Test Cases:**

```rust
fn test_place_1x1_entity() { /* single cell occupied */ }
fn test_place_3x3_entity() { /* 9 cells occupied, correct positions */ }
fn test_place_2x2_entity() { /* 4 cells occupied */ }
fn test_can_place_collision() { /* returns Ok(false) when overlapping */ }
fn test_place_collision_error() { /* returns GridError::Collision */ }
fn test_remove_frees_cells() { /* place, remove, can_place returns true again */ }
fn test_get_at_occupied() { /* returns correct entity */ }
fn test_get_at_empty() { /* returns None */ }
fn test_get_entity_by_id() { /* returns correct PlacedEntity */ }
fn test_bounding_box_empty() { /* returns None */ }
fn test_bounding_box_single() { /* returns entity footprint */ }
fn test_bounding_box_multiple() { /* returns enclosing box */ }
fn test_get_neighbors() { /* entities within radius returned */ }
fn test_splitter_north_vs_east() { /* 2x1 North, 1x2 East */ }
fn test_combinator_rotation() { /* 1x2 North, 2x1 East */ }
fn test_entity_count_and_cell_count() { /* track correctly through place/remove */ }
fn test_center_to_topleft_all_parities() { /* verify mapping for odd/even width/height */ }
fn test_with_bounds_rejects_out_of_bounds() { /* placement outside bounds fails */ }
```

**Constraints:**
- Unknown prototype name → `GridError::UnknownPrototype`
- Collision → `GridError::Collision` with occupant info
- All cells occupied by an entity point back to it via `CellState::Occupied`

**Verification:**
Run: `cargo test -p factorio-grid`
Expected: All tests pass

**Commit after passing.**

---

### Task 4: Blueprint Import [Mode: Delegated]

**Files:**
- Create: `crates/grid/src/import.rs`

**Contracts:**

```rust
pub struct ImportResult {
    pub grid: Grid,
    pub skipped: Vec<SkippedEntity>,
}

pub struct SkippedEntity {
    pub entity_number: u32,
    pub name: String,
    pub reason: String,
}

/// Build a Grid from a decoded Blueprint.
/// Unknown entities are skipped (collected in ImportResult.skipped).
pub fn from_blueprint(blueprint: &Blueprint) -> ImportResult;
```

Iterates `blueprint.entities`, looks up prototype, calls `grid.place()`. Unknown prototypes → SkippedEntity. This should never fail hard — real blueprints are non-overlapping, and unknown entities are gracefully skipped.

**Test Cases (integration tests in `tests/blueprint_import.rs`):**

```rust
fn test_import_single_belt() { /* 1 entity, 1 cell, no skipped */ }
fn test_import_assembler_setup() { /* verify entity count, assembler at expected position */ }
fn test_import_underground_belts() { /* entity_type preserved ("input"/"output") */ }
fn test_import_complex_circuit() { /* combinators placed, some entities may be skipped */ }
fn test_import_all_unknown() { /* grid empty, all entities in skipped */ }
fn test_no_collisions_in_real_blueprints() { /* verify all real blueprints import cleanly */ }
```

Test data: copy blueprint string constants from `crates/blueprint/tests/roundtrip.rs`.

**Verification:**
Run: `cargo test -p factorio-grid`
Expected: All tests pass

**Commit after passing.**

---

### Task 5: ASCII Renderer and Public API [Mode: Delegated]

**Files:**
- Create: `crates/grid/src/render.rs`
- Modify: `crates/grid/src/lib.rs` (module declarations + re-exports)

**Contracts:**

```rust
// render.rs
/// Render grid as ASCII. Entity type → character:
/// B=belt, I=inserter, A=assembler, F=furnace, S=splitter,
/// U=underground belt, P=pipe, E=electric pole, C=chemical plant,
/// R=refinery, K=beacon, X=combinator, .=empty
pub fn render_ascii(grid: &Grid) -> String;

// lib.rs re-exports:
pub use error::GridError;
pub use grid::Grid;
pub use import::{from_blueprint, ImportResult, SkippedEntity};
pub use prototype::{EntityPrototype, lookup as lookup_prototype};
pub use render::render_ascii;
pub use types::{CellState, EntityId, GridPos, PlacedEntity};
pub use factorio_blueprint;
```

**Test Cases:**

```rust
fn test_render_empty() { /* empty string or minimal output */ }
fn test_render_single_belt() { /* "B\n" */ }
fn test_render_3x3_assembler() { /* 3 lines of "AAA" */ }
fn test_render_mixed_entities() { /* belts + assembler, verify spatial layout */ }
fn test_render_imported_blueprint() { /* decode ASSEMBLER_SETUP, import, render, verify ASCII */ }
```

**Verification:**
Run: `cargo test -p factorio-grid`
Expected: All tests pass

**Commit after passing.**

---

### Task Dependency Graph

```
Task 1 (types/errors) ──┐
                         ├── Task 3 (grid core) → Task 4 (import) → Task 5 (render + API)
Task 2 (prototypes) ─────┘
```

Tasks 1 and 2 are independent and can run in parallel.

### File Structure After Implementation

```
crates/grid/
├── Cargo.toml
├── src/
│   ├── lib.rs          (~25 lines — modules + re-exports)
│   ├── types.rs        (~60 lines)
│   ├── error.rs        (~30 lines)
│   ├── prototype.rs    (~180 lines — mostly data table)
│   ├── grid.rs         (~250 lines — core logic)
│   ├── import.rs       (~80 lines)
│   └── render.rs       (~80 lines)
└── tests/
    └── blueprint_import.rs  (~150 lines — integration tests with real blueprints)
```

---

## Execution
**Skill:** superpowers:subagent-driven-development
- Mode A tasks: Opus implements directly (Tasks 1, 2)
- Mode B tasks: Dispatched to subagents (Tasks 3, 4, 5)
