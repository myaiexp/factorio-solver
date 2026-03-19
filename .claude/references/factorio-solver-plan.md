# Factorio Layout Solver — Project Plan

## Vision

A native desktop application that automatically generates Factorio blueprints from high-level goals. The user specifies what they want ("green circuits, 1 belt output, inputs from south on a 4-belt bus") and the tool produces a valid, pasteable blueprint string with a visual preview.

The core philosophy: the tool handles the tedious spatial planning (grid placement, belt routing, inserter alignment) while the user makes the interesting decisions (what to build, how big, what constraints). It's an architect, not a blueprint book.

## Stack

- **Language:** Rust
- **GUI Framework:** egui (immediate-mode native GUI)
- **Target Platform:** Linux (Arch), with cross-platform as a bonus from Rust
- **Future:** Logic may be ported to a Factorio mod (Lua) once proven

## Architecture

The project is split into independent crates (Rust workspace):

```
factorio-solver/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── blueprint/          # Blueprint parsing/encoding
│   ├── grid/               # 2D spatial engine
│   ├── templates/          # Template library + extraction
│   ├── solver/             # Layout composition + routing
│   └── ui/                 # egui frontend
```

### Crate Dependency Graph

```
ui → solver → templates → grid → blueprint
```

Each crate can be developed and tested independently. The UI is the thinnest layer — all logic lives in the lower crates.

---

## Crate Details

### `factorio-blueprint`

Handles encoding/decoding Factorio blueprint strings.

**Blueprint string format:**
1. First byte is a version character (currently `0`)
2. Remaining bytes are base64-encoded
3. Decoded bytes are zlib-compressed
4. Decompressed data is JSON

**Key JSON structure:**
```json
{
  "blueprint": {
    "item": "blueprint",
    "label": "My Blueprint",
    "icons": [...],
    "entities": [
      {
        "entity_number": 1,
        "name": "assembling-machine-2",
        "position": { "x": 0.5, "y": 0.5 },
        "direction": 4,
        "recipe": "electronic-circuit"
      },
      {
        "entity_number": 2,
        "name": "inserter",
        "position": { "x": 1.5, "y": 0.5 },
        "direction": 6
      },
      {
        "entity_number": 3,
        "name": "transport-belt",
        "position": { "x": 2.5, "y": 0.5 },
        "direction": 4
      }
    ],
    "tiles": [...],
    "version": 281479278886912
  }
}
```

**Entity properties to parse:**
- `name` — entity prototype name (e.g., `"assembling-machine-3"`)
- `position` — center position as `{x, y}` floats (entities align to 0.5 offsets for odd-width, integer for even-width)
- `direction` — 0=North, 2=East, 4=South, 6=West (8-direction for rail signals, but not relevant for MVP)
- `recipe` — only on assemblers/chemical plants/etc
- `connections` — circuit wire connections (can be deferred)
- `items` — module insertions
- `type` — input/output for underground belts and loaders

**Rust types:**
```rust
pub struct Blueprint {
    pub label: Option<String>,
    pub entities: Vec<Entity>,
    pub tiles: Vec<Tile>,
    pub icons: Vec<Icon>,
    pub version: u64,
}

pub struct Entity {
    pub entity_number: u32,
    pub name: String,
    pub position: Position,
    pub direction: Direction,
    pub recipe: Option<String>,
    pub items: Option<HashMap<String, u32>>,  // modules
    pub connections: Option<Connections>,
    pub type_field: Option<String>,  // "input"/"output" for underground belts
}

pub struct Position {
    pub x: f64,
    pub y: f64,
}

pub enum Direction {
    North = 0,
    East = 2,
    South = 4,
    West = 6,
}
```

**Functions:**
- `decode(blueprint_string: &str) -> Result<Blueprint>` — full decode pipeline
- `encode(blueprint: &Blueprint) -> Result<String>` — full encode pipeline
- Round-trip fidelity is critical: decode then encode should produce a functionally identical blueprint

**Dependencies:** `serde`, `serde_json`, `base64`, `flate2` (zlib)

---

### `factorio-grid`

The spatial simulation layer. Represents a 2D grid where entities occupy cells according to their collision boxes.

**Entity data (needs a static registry):**

The solver needs to know physical properties of every entity. This data comes from Factorio's prototype definitions. For MVP, hardcode vanilla entities. Later, this could be loaded from a mod-exported data file.

Key properties per entity prototype:
- `collision_box` — bounding box relative to center, e.g., assembler-3 is `[[-1.2, -1.2], [1.2, 1.2]]` (effectively 3×3 tiles)
- `tile_width` / `tile_height` — derived from collision box, snapped to grid
- `ingredient_count` — max input slots (determines how many inserters needed)
- `crafting_speed` — for ratio calculations
- `allowed_effects` — which modules can go in

**Grid representation:**
```rust
pub struct Grid {
    width: i32,
    height: i32,
    cells: HashMap<(i32, i32), CellState>,
    entities: Vec<PlacedEntity>,
}

pub enum CellState {
    Empty,
    Occupied { entity_index: usize },
    Reserved,  // e.g., space needed for inserter swing
}

pub struct PlacedEntity {
    pub prototype: &'static EntityPrototype,
    pub position: GridPos,
    pub direction: Direction,
    pub recipe: Option<String>,
}
```

**Key operations:**
- `can_place(entity, position, direction) -> bool` — collision check
- `place(entity, position, direction) -> Result<EntityId>` — place with validation
- `remove(entity_id)` — remove and free cells
- `get_neighbors(position, radius) -> Vec<PlacedEntity>` — spatial queries
- `bounding_box() -> (GridPos, GridPos)` — smallest box containing all entities

**Inserter logic:**
Inserters have a pickup position and a drop position determined by their direction. This is critical for the solver to understand connectivity:
- Standard inserter: pickup 1 tile behind, drop 1 tile ahead
- Long inserter: pickup 2 tiles behind, drop 2 tiles ahead

---

### `factorio-templates`

Stores and manages reusable layout patterns extracted from community blueprints.

**Template definition:**
```rust
pub struct Template {
    pub id: String,
    pub name: String,
    pub recipe: String,                    // what this produces
    pub throughput: f64,                   // items/sec output
    pub inputs: Vec<TemplatePort>,         // where inputs enter
    pub outputs: Vec<TemplatePort>,        // where outputs exit
    pub entities: Vec<PlacedEntity>,       // the actual layout
    pub bounding_box: (GridPos, GridPos),
    pub tileable: Option<TileDirection>,   // can it be repeated?
    pub tags: Vec<String>,                 // "compact", "beacon-friendly", etc.
    pub source: TemplateSource,
    pub quality_score: f64,
}

pub struct TemplatePort {
    pub position: GridPos,       // relative to template origin
    pub side: Direction,         // which edge of the template
    pub item: String,            // what item flows here
    pub belt_type: BeltTier,     // yellow/red/blue
    pub lanes: Vec<LaneUsage>,   // which belt lanes are used
}

pub enum TileDirection {
    Horizontal,
    Vertical,
    Both,
}

pub enum TemplateSource {
    Extracted { blueprint_hash: String, source_url: Option<String> },
    Manual,
}
```

**Template extraction pipeline:**

1. **Parse** blueprint into grid representation
2. **Identify production clusters:**
   - Start from each assembler/furnace/chemical plant
   - Follow inserter chains to find all directly connected entities
   - Include the belts that directly feed/output from these inserters
   - Stop at belt junctions or where belts serve multiple machines
3. **Detect tileability:**
   - Check if the blueprint can be decomposed into repeating units
   - Try shifting the pattern horizontally and vertically
   - Find the minimal repeating unit
4. **Extract I/O ports:**
   - Identify belt endpoints at the cluster boundary
   - Determine direction, item type, and belt tier
   - Classify as input or output based on belt direction relative to the cluster
5. **Calculate throughput:**
   - Use recipe times, crafting speed, and module effects
   - Verify against belt capacity at I/O ports
6. **Score quality:**
   - Compactness (entities per tile)
   - Ratio correctness (are the assembler counts right for the throughput?)
   - Beacon-friendliness (if relevant)
   - Belt utilization efficiency

**Template storage:** Serialize to JSON files. A template library is just a directory of these files, indexed by recipe name for fast lookup.

**Community blueprint ingestion:**
- Source: factorioprints.com (has an API), Reddit r/factorio, Factorio School
- Bulk download blueprint strings
- Run extraction pipeline on each
- Deduplicate by structural similarity (not exact match, since positions may differ)
- Keep highest-scored template per recipe per constraint profile

---

### `factorio-solver`

The core intelligence. Takes a user goal and produces a valid layout.

**User goal specification:**
```rust
pub struct LayoutGoal {
    pub product: String,           // "electronic-circuit"
    pub throughput: ThroughputSpec,
    pub input_constraints: Vec<InputConstraint>,
    pub bounds: Option<BoundingBox>,    // max allowed area
    pub style: LayoutStyle,
}

pub enum ThroughputSpec {
    BeltCount(u32, BeltTier),     // "2 red belts of output"
    ItemsPerSecond(f64),
    ItemsPerMinute(f64),
    SciencePerMinute(f64),        // for end-to-end SPM goals
}

pub enum LayoutStyle {
    Compact,
    BusFriendly { bus_direction: Direction, bus_width: u32 },
    CityBlock { block_size: (u32, u32) },
}
```

**Solver pipeline:**

1. **Recipe resolution:**
   - Look up the recipe for the target product
   - Recursively resolve sub-recipes if doing full production chains
   - Calculate exact machine counts at each tier based on throughput goal
   - Account for modules and beacons if specified

2. **Template selection:**
   - Query template library for matching recipe
   - Filter by throughput compatibility and I/O direction constraints
   - Select best-scoring template
   - If throughput exceeds single template, calculate how many copies are needed

3. **Placement:**
   - Position template copies on the grid according to tiling rules
   - Respect bounding box constraints
   - Handle non-tileable templates by arranging copies with spacing

4. **Routing (the hard part):**
   - Connect template I/O ports to each other and to external bus/inputs
   - Use A* pathfinding on the grid, treating occupied cells as obstacles
   - Belt routing rules:
     - Belts occupy 1 tile
     - Underground belts span up to 4/6/8 tiles depending on tier
     - Splitters are 1×2
     - Prefer straight runs over turns
     - Prefer underground belts for crossing perpendicular routes
     - Avoid unnecessary lane mixing
   - May need multiple routing passes with backtracking

5. **Validation:**
   - Verify all inserters can reach their source/destination
   - Verify belt throughput at every point
   - Verify no entity collisions
   - Verify recipe chain completeness (no missing inputs)

6. **Output:**
   - Convert grid state to Blueprint struct
   - Encode to blueprint string

**Belt pathfinding specifics:**

```rust
pub struct BeltRouter {
    grid: &Grid,
    routes: Vec<BeltRoute>,
}

pub struct BeltRoute {
    pub from: GridPos,
    pub to: GridPos,
    pub item: String,
    pub required_throughput: f64,
    pub belt_tier: BeltTier,
    pub path: Vec<BeltSegment>,
}

pub enum BeltSegment {
    Belt { pos: GridPos, direction: Direction },
    UndergroundEntry { pos: GridPos, direction: Direction, tier: BeltTier },
    UndergroundExit { pos: GridPos, direction: Direction, tier: BeltTier },
    Splitter { pos: GridPos, direction: Direction },
}
```

The A* heuristic should penalize:
- Turns (prefer straight lines)
- Length (shorter is better)
- Proximity to other routes (avoid congestion)
- Surface belt segments when underground is possible (cleaner)

---

### `factorio-ui`

egui-based native desktop frontend.

**Main panels:**

1. **Grid viewport** (center, largest area)
   - 2D scrollable/zoomable view of the grid
   - Entity rendering: colored rectangles with icons or letters initially, actual sprites later
   - Belt direction arrows
   - Inserter pickup/drop indicators
   - Grid lines toggle
   - Hover tooltip showing entity details

2. **Goal panel** (left sidebar)
   - Product selector (dropdown with search)
   - Throughput input (with unit selector: belts, items/s, items/min)
   - Input constraint editor
   - Style selector (compact / bus / city block)
   - "Generate" button

3. **Template browser** (right sidebar or tab)
   - List of available templates for selected recipe
   - Preview of each template
   - Quality score, dimensions, throughput
   - Import button (paste blueprint string to extract new template)

4. **Output panel** (bottom)
   - Generated blueprint string (click to copy)
   - Validation warnings/errors
   - Stats: entity count, dimensions, throughput achieved

**Key interactions:**
- Generate → solver runs → grid viewport updates with result
- Click entity in viewport → show details
- Drag to pan, scroll to zoom
- Export button → blueprint string to clipboard

**egui specifics:**
- Use `egui::CentralPanel` for the grid viewport
- Custom painting with `egui::Painter` for the grid (not standard widgets)
- `egui_extras` for the sidebar panels
- Render at native resolution, handle DPI scaling

---

## Recipe & Entity Data

The solver needs a knowledge base of Factorio's recipes and entity prototypes. For MVP, this is hardcoded for vanilla Factorio.

**Recipe data needed:**
```rust
pub struct Recipe {
    pub name: String,
    pub category: String,              // "crafting", "smelting", "chemistry", etc.
    pub energy: f64,                   // crafting time in seconds
    pub ingredients: Vec<ItemAmount>,
    pub results: Vec<ItemAmount>,       // usually one, but some have byproducts
}

pub struct ItemAmount {
    pub name: String,
    pub amount: f64,     // can be fractional for probabilities
}
```

**Source for this data:** Factorio's `data.raw` export. Can be dumped from the game with a small mod, or sourced from the Factorio wiki's machine-readable data. For MVP, manually define the ~50 most important recipes (science packs, intermediates, logistics).

**Entity prototype data needed:**
```rust
pub struct EntityPrototype {
    pub name: String,
    pub tile_width: u32,
    pub tile_height: u32,
    pub crafting_speed: Option<f64>,
    pub crafting_categories: Vec<String>,
    pub module_slots: u32,
    pub ingredient_count: u32,      // max input slots
    pub belt_speed: Option<f64>,    // tiles/sec for belts
    pub underground_distance: Option<u32>,  // max gap for underground belts
    pub inserter_reach: Option<InsertReach>,
}
```

---

## Phased Build Order

### Phase 1: Blueprint Foundation
**Goal:** Read and write Factorio blueprint strings.
- Implement `factorio-blueprint` crate
- Decode pipeline: strip version byte → base64 decode → zlib decompress → JSON parse → Rust structs
- Encode pipeline: reverse of above
- Test with real blueprint strings from the game
- **Deliverable:** CLI tool that round-trips a blueprint string (decode then encode, verify identical)

### Phase 2: Grid Engine
**Goal:** Place entities on a 2D grid with collision detection.
- Implement `factorio-grid` crate
- Hardcode entity prototypes for ~20 core entities (assemblers, inserters, belts, furnaces)
- Build grid from a decoded blueprint (place all entities)
- Implement collision detection, entity queries
- **Deliverable:** Parse a blueprint → build grid → print ASCII representation of the layout

### Phase 3: Basic UI
**Goal:** Visualize blueprints in a native window.
- Implement `factorio-ui` crate with egui
- Render the grid as colored rectangles
- Pan and zoom
- Load a blueprint string → display it
- **Deliverable:** Paste a blueprint → see it rendered in a window

### Phase 4: Template Extraction
**Goal:** Identify reusable patterns from existing blueprints.
- Implement `factorio-templates` crate
- Cluster detection (group entities by inserter connectivity)
- I/O port identification
- Tileability detection
- Quality scoring
- **Deliverable:** Feed in a community blueprint → get extracted templates with metadata

### Phase 5: Community Blueprint Ingestion
**Goal:** Build a template library from public blueprints.
- Scrape/download from factorioprints.com
- Batch extraction pipeline
- Deduplication
- Template library storage and indexing
- **Deliverable:** A library of scored templates for common recipes

### Phase 6: Layout Solver (MVP)
**Goal:** Generate a layout from a goal specification.
- Implement `factorio-solver` crate
- Recipe resolution and machine count calculation
- Template selection and placement
- Basic belt routing with A* pathfinding
- Blueprint output
- **Deliverable:** "Give me 1 belt of green circuits from a south bus" → valid blueprint string

### Phase 7: UI Integration
**Goal:** Full interactive workflow.
- Goal input panel
- Template browser
- Live preview of generated layouts
- Blueprint export (copy to clipboard)
- Validation feedback
- **Deliverable:** Complete desktop app workflow from goal → generated blueprint

### Phase 8: Advanced Solver
**Goal:** Handle complex layouts.
- Multi-product layouts
- Full production chains (specify end product, solver handles all intermediates)
- City block-aware generation
- Beacon placement optimization
- Train station integration
- **Deliverable:** "Give me 45 SPM in city blocks" → full blueprint book

---

## Key Technical Risks

1. **Belt routing complexity:** A* pathfinding for belts in a congested grid may produce ugly or invalid results. Mitigation: start with simple cases (single product, lots of space), add routing quality heuristics iteratively.

2. **Template boundary ambiguity:** Extracting clean templates from complex blueprints requires good heuristics for "where does this module end." Mitigation: start with simple, clearly tileable blueprints (smelting arrays, simple assembler setups).

3. **Entity data completeness:** Hardcoding entity prototypes is tedious and error-prone. Mitigation: start with a small set (~20 entities), expand as needed. Eventually parse from data.raw export.

4. **egui rendering performance:** Large grids with thousands of entities may be slow to render. Mitigation: viewport culling (only render visible entities), level-of-detail (simplify distant entities).

5. **Solver convergence:** For complex goals, the solver may fail to find a valid layout. Mitigation: clear error messages about what failed, allow partial results, let the user relax constraints.

---

## External Data Sources

- **Factorio wiki** (wiki.factorio.com) — recipe data, entity properties
- **factorioprints.com** — community blueprints (has JSON API)
- **Factorio data.raw** — authoritative game data, can be exported with a small mod
- **Factorio blueprint format spec** — documented on the wiki at wiki.factorio.com/Blueprint_string_format

---

## Future Considerations (Post-MVP)

- **Space Age DLC support** — new recipes, entities, planets with different constraints
- **Mod support** — load recipe/entity data from modded data.raw exports
- **In-game mod** — port the solver logic to Lua, or have the mod call an external Rust process
- **Sharing** — export/import template libraries between users
- **Blueprint book generation** — output organized books, not just single blueprints
- **Circuit network logic** — auto-generate circuit conditions for things like oil cracking ratios
