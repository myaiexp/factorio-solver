# Phase 3: Basic UI — Implementation Plan

**Goal:** Build a native egui desktop app that visualizes Factorio blueprints as interactive, colored grid renders with pan, zoom, and entity tooltips.

**Architecture:** The `factorio-ui` crate becomes a binary app built on `eframe`. It decodes blueprint strings via `factorio-blueprint`, builds grids via `factorio-grid`, and renders entities as colored rectangles in a custom-painted viewport. Viewport transform math is isolated and testable; rendering is verified visually.

**Tech Stack:** Rust, eframe/egui (latest stable), factorio-blueprint, factorio-grid

---

### Task 1: Viewport transform and entity colors

**Files:**
- Create: `crates/ui/src/viewport.rs`
- Create: `crates/ui/src/colors.rs`

[Mode: Delegated]

**Contracts:**

`viewport.rs` — `ViewportTransform` struct with no egui dependency (plain `f32` math):
- `center: (f32, f32)` — world coordinate at screen center
- `zoom: f32` — pixels per grid cell
- `new() -> Self` — default center (0,0), zoom 32.0
- `world_to_screen(world: (f32, f32), screen_size: (f32, f32)) -> (f32, f32)` — `screen = (world - center) * zoom + screen_size/2`
- `screen_to_world(screen: (f32, f32), screen_size: (f32, f32)) -> (f32, f32)` — inverse
- `visible_world_rect(screen_size: (f32, f32)) -> (f32, f32, f32, f32)` — `(min_x, min_y, max_x, max_y)` in world coords
- `zoom_at(screen_point: (f32, f32), screen_size: (f32, f32), factor: f32)` — zoom centered on mouse: world point under cursor stays fixed
- `pan(screen_delta: (f32, f32))` — shift center by `delta / zoom` (inverted, drag moves view)
- `fit_to_bounds(min: (f32, f32), max: (f32, f32), screen_size: (f32, f32), padding: f32)` — center and zoom to fit a world rect with padding cells on each side

`colors.rs` — `EntityCategory` enum + classification:
- Variants: `Belt, UndergroundBelt, Splitter, Inserter, Assembler, Furnace, ChemicalPlant, Refinery, Pipe, ElectricPole, Beacon, Combinator, Lamp, Unknown`
- `from_prototype_name(name: &str) -> Self` — same substring matching order as `char_for_prototype` in `crates/grid/src/render.rs`
- `color(&self) -> egui::Color32` — distinct color per category on dark background
- `label_char(&self) -> char` — matches ASCII renderer chars (B, U, S, I, A, F, C, R, P, E, K, X, L, ?)

Color palette (dark-background-friendly):
| Category | Char | Hex |
|----------|------|-----|
| Belt | B | `#D4A017` |
| UndergroundBelt | U | `#B8860B` |
| Splitter | S | `#E88800` |
| Inserter | I | `#55AAFF` |
| Assembler | A | `#4682B4` |
| Furnace | F | `#CD7F32` |
| ChemicalPlant | C | `#008080` |
| Refinery | R | `#8B008B` |
| Pipe | P | `#708090` |
| ElectricPole | E | `#32CD32` |
| Beacon | K | `#9370DB` |
| Combinator | X | `#FA8072` |
| Lamp | L | `#FFFACD` |
| Unknown | ? | `#696969` |

**Test Cases:**

```rust
// viewport.rs tests
#[test]
fn test_world_to_screen_origin() {
    // At center=(0,0), zoom=32, world (0,0) maps to screen center
    let vp = ViewportTransform::new();
    let screen = vp.world_to_screen((0.0, 0.0), (800.0, 600.0));
    assert_eq!(screen, (400.0, 300.0));
}

#[test]
fn test_roundtrip_world_screen() {
    let vp = ViewportTransform { center: (5.0, 3.0), zoom: 48.0 };
    let world = (7.5, -2.0);
    let screen = vp.world_to_screen(world, (1280.0, 800.0));
    let back = vp.screen_to_world(screen, (1280.0, 800.0));
    assert!((back.0 - world.0).abs() < 1e-4);
    assert!((back.1 - world.1).abs() < 1e-4);
}

#[test]
fn test_visible_rect_shrinks_with_zoom() {
    let mut vp = ViewportTransform::new();
    let rect1 = vp.visible_world_rect((800.0, 600.0));
    vp.zoom *= 2.0;
    let rect2 = vp.visible_world_rect((800.0, 600.0));
    let width1 = rect1.2 - rect1.0;
    let width2 = rect2.2 - rect2.0;
    assert!((width2 - width1 / 2.0).abs() < 1e-4);
}

#[test]
fn test_zoom_at_preserves_world_point() {
    let mut vp = ViewportTransform { center: (10.0, 10.0), zoom: 32.0 };
    let screen_size = (800.0, 600.0);
    let mouse = (200.0, 150.0);
    let world_before = vp.screen_to_world(mouse, screen_size);
    vp.zoom_at(mouse, screen_size, 1.5);
    let world_after = vp.screen_to_world(mouse, screen_size);
    assert!((world_after.0 - world_before.0).abs() < 1e-3);
    assert!((world_after.1 - world_before.1).abs() < 1e-3);
}

#[test]
fn test_pan_shifts_center() {
    let mut vp = ViewportTransform { center: (0.0, 0.0), zoom: 32.0 };
    vp.pan((32.0, 0.0)); // drag right by 1 cell worth of pixels
    // Dragging right moves the world left → center.x decreases
    assert!((vp.center.0 - (-1.0)).abs() < 1e-4);
}

#[test]
fn test_fit_to_bounds_contains_rect() {
    let mut vp = ViewportTransform::new();
    vp.fit_to_bounds((0.0, 0.0), (20.0, 15.0), (800.0, 600.0), 2.0);
    let visible = vp.visible_world_rect((800.0, 600.0));
    assert!(visible.0 <= 0.0 && visible.1 <= 0.0);
    assert!(visible.2 >= 20.0 && visible.3 >= 15.0);
}

// colors.rs tests
#[test]
fn test_category_classification() {
    assert_eq!(EntityCategory::from_prototype_name("transport-belt"), EntityCategory::Belt);
    assert_eq!(EntityCategory::from_prototype_name("underground-belt"), EntityCategory::UndergroundBelt);
    assert_eq!(EntityCategory::from_prototype_name("fast-splitter"), EntityCategory::Splitter);
    assert_eq!(EntityCategory::from_prototype_name("fast-inserter"), EntityCategory::Inserter);
    assert_eq!(EntityCategory::from_prototype_name("assembling-machine-3"), EntityCategory::Assembler);
    assert_eq!(EntityCategory::from_prototype_name("electric-furnace"), EntityCategory::Furnace);
    assert_eq!(EntityCategory::from_prototype_name("chemical-plant"), EntityCategory::ChemicalPlant);
    assert_eq!(EntityCategory::from_prototype_name("oil-refinery"), EntityCategory::Refinery);
    assert_eq!(EntityCategory::from_prototype_name("pipe-to-ground"), EntityCategory::Pipe);
    assert_eq!(EntityCategory::from_prototype_name("substation"), EntityCategory::ElectricPole);
    assert_eq!(EntityCategory::from_prototype_name("beacon"), EntityCategory::Beacon);
    assert_eq!(EntityCategory::from_prototype_name("decider-combinator"), EntityCategory::Combinator);
    assert_eq!(EntityCategory::from_prototype_name("small-lamp"), EntityCategory::Lamp);
    assert_eq!(EntityCategory::from_prototype_name("something-modded"), EntityCategory::Unknown);
}

#[test]
fn test_distinct_colors() {
    // All categories should have unique colors
    let categories = [Belt, UndergroundBelt, Splitter, Inserter, Assembler, Furnace,
                      ChemicalPlant, Refinery, Pipe, ElectricPole, Beacon, Combinator, Lamp, Unknown];
    let colors: Vec<_> = categories.iter().map(|c| c.color()).collect();
    for i in 0..colors.len() {
        for j in (i+1)..colors.len() {
            assert_ne!(colors[i], colors[j], "{:?} and {:?} share a color", categories[i], categories[j]);
        }
    }
}

#[test]
fn test_label_chars_match_ascii_renderer() {
    // Must stay in sync with char_for_prototype in grid/src/render.rs
    assert_eq!(EntityCategory::Belt.label_char(), 'B');
    assert_eq!(EntityCategory::UndergroundBelt.label_char(), 'U');
    assert_eq!(EntityCategory::Splitter.label_char(), 'S');
    assert_eq!(EntityCategory::Inserter.label_char(), 'I');
    assert_eq!(EntityCategory::Assembler.label_char(), 'A');
    assert_eq!(EntityCategory::Furnace.label_char(), 'F');
    assert_eq!(EntityCategory::Unknown.label_char(), '?');
}
```

**Constraints:**
- `ViewportTransform` must NOT depend on any egui types — pure `f32` math only
- `EntityCategory::from_prototype_name` must match the exact classification order from `crates/grid/src/render.rs:8-38`

**Verification:**
```bash
cargo test -p factorio-ui
```
Expected: All tests pass

**Commit after passing.**

---

### Task 2: App shell with eframe, blueprint input, and grid loading

**Files:**
- Create: `crates/ui/src/main.rs`
- Create: `crates/ui/src/app.rs`
- Modify: `crates/ui/Cargo.toml` — add eframe, egui, factorio-grid, factorio-blueprint deps; declare binary target
- Delete: `crates/ui/src/lib.rs` — crate is now a pure binary

[Mode: Delegated]

**Contracts:**

`Cargo.toml` additions:
- `eframe` and `egui` at latest stable
- `factorio-grid = { path = "../grid" }`
- `factorio-blueprint = { path = "../blueprint" }`
- Keep `factorio-solver = { path = "../solver" }` for future phases
- Add `[[bin]]` section: `name = "factorio-ui"`, `path = "src/main.rs"`

`main.rs`:
- Module declarations: `mod app; mod colors; mod viewport;`
- `fn main() -> eframe::Result<()>` — launches native window at 1280x800, title "Factorio Layout Solver"

`app.rs` — `FactorioApp` struct:
- `blueprint_input: String` — text field content
- `state: AppState` — enum: `Empty`, `Loaded { grid: Grid, label: Option<String>, skipped: Vec<SkippedEntity> }`, `Error(String)`
- `viewport: ViewportTransform`
- `show_grid_lines: bool`
- `new() -> Self` — default state
- `load_blueprint(&mut self)` — decode input → build grid → update state; auto-fit viewport on success
- `eframe::App` impl with `update()`:
  - Top panel: text input, Load button (also loads on Enter), grid lines checkbox, entity count + label when loaded
  - Bottom panel: status messages ("Paste a blueprint..." / error / "N entities skipped")
  - Central panel: calls `self.render_viewport(ui)` (placeholder for Task 3)
- `render_viewport(&mut self, ui: &mut egui::Ui)` — stub that just paints dark gray background; Tasks 3+4 fill this in

**Constraints:**
- Blueprint books → show error "Blueprint books not yet supported"
- Decode errors → show in bottom panel in red
- Auto-fit viewport to grid bounding box on successful load (with padding)
- `lib.rs` is deleted — no downstream crates depend on `factorio-ui`

**Verification:**
```bash
cargo build -p factorio-ui && cargo run -p factorio-ui
```
Expected: Window opens. Text input and Load button visible. Pasting invalid text shows error. Pasting valid blueprint shows entity count.

**Commit after building.**

---

### Task 3: Grid viewport rendering with pan, zoom, and culling

**Files:**
- Modify: `crates/ui/src/app.rs` — implement full `render_viewport`

[Mode: Delegated]

**Contracts:**

`render_viewport(&mut self, ui: &mut egui::Ui)`:
1. Allocate painter with `ui.allocate_painter(available_size, Sense::click_and_drag())`
2. **Pan**: on drag (primary or middle button), call `viewport.pan(drag_delta)`
3. **Zoom**: on scroll, get mouse position, call `viewport.zoom_at(mouse, screen_size, factor)` with factor 1.1 per scroll tick. Clamp zoom to 2.0..200.0
4. **Background**: fill with dark gray (`Color32::from_gray(40)`)
5. **Grid lines** (when enabled): draw vertical/horizontal lines for visible cell range with subtle color (`from_gray(60)`, 0.5px stroke)
6. **Entity rendering**: iterate `grid.entities()`, for each:
   - Compute screen rect from `entity.position` (top-left) and `entity.size`
   - **Cull**: skip if screen rect doesn't intersect visible area
   - Draw filled rect with `EntityCategory::from_prototype_name(entity.prototype_name).color()`
   - Draw 1px dark border
   - If `viewport.zoom > 20.0`: draw `label_char()` centered in entity rect, white, font size proportional to zoom

**Constraints:**
- Must use the `ViewportTransform` from Task 1 for all coordinate math
- Must use `EntityCategory` from Task 1 for colors and labels
- Culling is viewport-based — skip entities entirely outside visible world rect
- No spatial index needed (O(n) entity scan is fine for this phase)

**Verification:**
```bash
cargo run -p factorio-ui
```
Paste a real blueprint → entities appear as colored rectangles. Scroll zooms. Drag pans. Grid lines toggle. Entity letters appear at high zoom.

**Commit after working.**

---

### Task 4: Hover tooltips and final polish

**Files:**
- Modify: `crates/ui/src/app.rs` — add tooltip logic to `render_viewport`

[Mode: Direct]

**Contracts:**

Tooltip logic (at end of `render_viewport`, after rendering):
- If pointer hovers in viewport, convert screen position → world coords via `screen_to_world`
- Floor to integer cell → call `grid.get_at(cell_x, cell_y)`
- If entity found, show tooltip with: name, position (grid coords), size, direction (human-readable), recipe (if any), entity type (if any)
- `direction_name(dir: Direction) -> &'static str` helper — maps all 8 variants to names

Additional polish:
- Home key: re-fit viewport to grid bounds
- Dark viewport background (`Color32::from_gray(40)`)
- Ensure `cargo test --workspace` still passes (no regressions)

**Test Cases:**

```rust
#[test]
fn test_direction_names() {
    assert_eq!(direction_name(Direction::North), "North");
    assert_eq!(direction_name(Direction::East), "East");
    assert_eq!(direction_name(Direction::South), "South");
    assert_eq!(direction_name(Direction::West), "West");
}
```

**Verification:**
```bash
cargo test --workspace && cargo run -p factorio-ui
```
Load blueprint → hover over entities → tooltip shows correct name, position, direction, recipe.

**Commit after passing.**

---

## Execution
**Skill:** superpowers:subagent-driven-development
- Mode A tasks: Opus implements directly (Task 4)
- Mode B tasks: Dispatched to subagents (Tasks 1, 2, 3)
