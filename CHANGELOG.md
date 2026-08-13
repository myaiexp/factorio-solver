# Changelog

## [Unreleased]

### Fixed

- **A generated block now delivers its target rate instead of half of it.** A Factorio inserter always drops on the far lane of a belt, so a belt filled by the block's own inserters gets one lane per neighbouring machine column, not two. The generator provisioned every output belt at two lanes and therefore ran at half the rate on the tin — 45/s of green circuits came out at 22.5/s, and the internal copper cable was starved the same way, so the circuit machines were fed at half rate too

### Added

- **Configurable cell topology for the block generator.** Blocks are built from cells — two machine columns sharing a spine of belts, with a shared edge belt on each side — and the arrangement is now a control rather than a constant: belts per side (1 or 2), which side carries ingredients, and an optional target width that wraps a long block into bands. Sharing the spine helps an input-bound step, sharing the edge an output-bound one; on the worked copper-cable case, moving the product to the wider side more than doubles the machines per cell
- **The generated-block panel reports which stream capped each step's sizing**, so it names the knob to turn
- **Recipes with three or four item ingredients now lay out**, fed by long-handed inserters reaching the outer belt of a pair. The long inserter is a picker of its own, filtered by actual reach rather than by name
- **A delivered-rate check**, measured from the placed grid rather than restated from the sizing arithmetic. A block that would under-deliver is refused outright instead of being handed over as a blueprint string that merely looks plausible
- **Steps with two products lay out**, each product claiming a whole belt rather than a lane — a machine column reaches only each belt's far lane, so a belt cannot be split between two items the way an ingredient belt can. Every output inserter carries a filter, without which both belts would take a random mix. Three or more products still refuse (`TooManyProductsForBelts`), since no topology offers a side more than two belts

### Removed

- The belt-capacity warning, which flagged a belt segment over its rating — sizing from belt throughput makes that condition structurally impossible

## [1.0.0] - 2026-03-20

### Added

- Centered empty-state guidance in the blueprint viewport — first-time users now see welcome text, load/paste instructions, and keyboard shortcut hints instead of a blank canvas
- Grid-to-Blueprint export (`to_blueprint`) for full round-trip capability: blueprints can now be imported, modified, and exported back to a blueprint string
- `Display` implementation for the `Direction` enum, plus utility methods: `opposite()`, `rotate_cw()`, `rotate_ccw()`, and cardinal direction checks

### Changed

- Expanded `EntityCategory` coverage to classify all 78 registered prototype groups — chests, turrets, walls, gates, mining drills, solar panels, accumulators, roboports, labs, and train stops no longer render as grey `?`
- Unified duplicated entity classification logic into the `grid` crate, eliminating the divergence between `grid/render.rs` and `ui/colors.rs`

### Fixed

- Prototype lookup optimized from O(n) linear scan to O(1) `HashMap` via `OnceLock`, reducing per-entity overhead during blueprint import and grid placement
