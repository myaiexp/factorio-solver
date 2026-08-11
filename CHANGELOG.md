# Changelog

<!-- Everything below the Unreleased section is malformed — duplicated inside a
     code fence, with commentary interleaved. Tracked as idea #2780; not fixed
     here so the cleanup stays its own reviewable change. -->

## [Unreleased]

### Fixed

- **A generated block now delivers its target rate instead of half of it.** A Factorio inserter always drops on the far lane of a belt, so a belt filled by the block's own inserters gets one lane per neighbouring machine column, not two. The generator provisioned every output belt at two lanes and therefore ran at half the rate on the tin — 45/s of green circuits came out at 22.5/s, and the internal copper cable was starved the same way, so the circuit machines were fed at half rate too

### Added

- **Configurable cell topology for the block generator.** Blocks are built from cells — two machine columns sharing a spine of belts, with a shared edge belt on each side — and the arrangement is now a control rather than a constant: belts per side (1 or 2), which side carries ingredients, and an optional target width that wraps a long block into bands. Sharing the spine helps an input-bound step, sharing the edge an output-bound one; on the worked copper-cable case, moving the product to the wider side more than doubles the machines per cell
- **The generated-block panel reports which stream capped each step's sizing**, so it names the knob to turn
- **Recipes with three or four item ingredients now lay out**, fed by long-handed inserters reaching the outer belt of a pair. The long inserter is a picker of its own, filtered by actual reach rather than by name
- **A delivered-rate check**, measured from the placed grid rather than restated from the sizing arithmetic. A block that would under-deliver is refused outright instead of being handed over as a blueprint string that merely looks plausible

### Removed

- **Steps with two or more products are refused rather than laid out.** `uranium-processing` (one ingredient, two item results) built under the old topology and no longer does. This is deliberate: a machine column owns its product lanes outright, and what the old layout built dropped both outputs on the same far lane, so it was never right
- The belt-capacity warning, which flagged a belt segment over its rating — sizing from belt throughput makes that condition structurally impossible

```markdown
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
```

`★ Insight ─────────────────────────────────────`
- The `to_blueprint` addition is architecturally significant — it completes a **round-trip data pipeline**, which is a common milestone in format-handling tools. It's worth calling out explicitly in changelogs because it unlocks workflows that weren't previously possible.
- The `OnceLock`-based `HashMap` optimization is an implementation detail, but its *user-facing effect* (faster blueprint import/placement) is changelog-worthy — the entry frames it that way rather than mentioning `OnceLock` or `std::sync`.
- Expanding `EntityCategory` from 13 to ~15 groups is framed as a **Fixed** entry (grey `?` was effectively a rendering bug) rather than **Changed**, since the prior behavior was unintentional rather than a deliberate design choice.
`─────────────────────────────────────────────────`

```markdown
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
```

`★ Insight ─────────────────────────────────────`
- The `to_blueprint` addition is architecturally significant — it completes a **round-trip data pipeline**, which is a common milestone in format-handling tools. It's worth calling out explicitly in changelogs because it unlocks workflows that weren't previously possible.
- The `OnceLock`-based `HashMap` optimization is an implementation detail, but its *user-facing effect* (faster blueprint import/placement) is changelog-worthy — the entry frames it that way rather than mentioning `OnceLock` or `std::sync`.
- Expanding `EntityCategory` from 13 to ~15 groups is framed as a **Fixed** entry (grey `?` was effectively a rendering bug) rather than **Changed**, since the prior behavior was unintentional rather than a deliberate design choice.
`─────────────────────────────────────────────────`