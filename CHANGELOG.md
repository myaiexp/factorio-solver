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