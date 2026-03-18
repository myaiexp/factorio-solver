# Phase 3: Basic UI [COMPLETE]

> Implement the `factorio-ui` crate — visualize Factorio blueprints in a native egui window with pan, zoom, and blueprint import.

## Goals

- Create a native desktop window using egui that renders grid layouts
- Render entities as colored rectangles with type indicators
- Implement pan and zoom navigation
- Load blueprint strings and display them visually

## Requirements

_All requirements met:_

- egui window launches at 1280x800 with top/bottom/central panel layout
- Blueprint string → decode → grid build → viewport rendering
- Entities as colored rectangles (14 categories, distinct colors on dark background)
- Entity label characters (B, U, S, I, A, F, C, R, P, E, K, X, L, ?) at zoom > 20
- Drag to pan, scroll to zoom (anchored at cursor), zoom clamped 2..200
- Grid lines toggle (subtle 0.5px lines, only for visible range)
- Hover tooltips: name, position, size, direction, recipe, entity type
- Text input with Load button (also Enter key)
- Viewport culling — entities outside visible rect skipped
- Home key re-fits viewport to grid bounds

## Architecture

```
crates/ui/
├── Cargo.toml       # binary crate, deps: eframe, egui, factorio-grid, factorio-blueprint
├── src/
│   ├── main.rs      # eframe launch, module declarations
│   ├── app.rs       # FactorioApp, AppState, update(), render_viewport(), tooltips
│   ├── viewport.rs  # ViewportTransform — pure f32 math, no egui dependency
│   └── colors.rs    # EntityCategory enum, color palette, label chars
```

- `ViewportTransform` handles all coordinate math (world ↔ screen), zoom, pan, fit-to-bounds
- `EntityCategory` classifies prototypes by substring matching (same order as grid ASCII renderer)
- `FactorioApp` orchestrates: blueprint decode → grid build → viewport render → tooltips
- Tooltip via `egui::Area` at pointer with popup frame styling

## Notes

- Blueprint books show a clear error message ("not yet supported")
- Auto-fit viewport on blueprint load (with 2-cell padding)
- Entity border: 1px dark stroke (from_gray(20)) for visual separation
- 10 unit tests covering viewport math, color classification, label chars, direction names
