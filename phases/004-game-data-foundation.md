# Phase 4: Game Data Foundation

**Status:** Complete
**Design:** `.claude/plans/2026-08-11-game-data-foundation-design.md`
**Plan:** `.claude/plans/2026-08-11-game-data-foundation-plan.md`

## Goal

Replace the hand-written entity registry with data ingested from the player's own
Factorio install, add a recipe registry, and render real game icons in the viewport.

Hard constraint: **no mods**. Mods disable Steam achievements and this is an
achievement run, so every mechanism here is mod-free (`factorio --dump-data` runs
in a separate process, touches no saves, and carries no achievement risk).

## What shipped

| Piece | Result |
| --- | --- |
| `crates/dump-ingest` | New dev-only binary. Reads `data-raw-dump.json` + locale files, emits both data files. Manually run on game updates; deliberately not a build step. |
| `crates/grid/data/prototypes.json` | 85 hand-written → **169 derived** entities |
| `crates/solver/src/recipe.rs` + `data/recipes.json` | New `Recipe`/`ItemAmount`/`ItemKind` model and registry, **649 recipes** |
| `crates/ui/src/icons.rs` | Icons read live from the install, decoded, cropped and cached as egui textures |
| `crates/ui/src/entity_draw.rs` | Per-entity draw extracted from `render_viewport`; icons at `LodLevel::Full` |

Reference data: Factorio **2.0.77**, linux64, Steam, full Space Age
(`base` + `elevated-rails` + `quality` + `space-age`).

## Bugs this fixed in the existing data

- **Five wrong footprints**, all Space Age buildings — `big-mining-drill`,
  `foundry` and `biolab` were 3×3/4×4/4×4 and are really 5×5; `rocket-turret`
  2×2 → 3×3; `recycler` 2×2 → 2×4. Not cosmetic: a wrong footprint means wrong
  collision detection and misplaced entities when importing any blueprint
  containing them.
- **`storage-tank` fluid connections** were 1; the game has 4.
- **`power_kw` conflated consumption and production** — steam-engine,
  steam-turbine and solar-panel stored *output* in the same field a solver would
  sum as *load*. Now consumption-only; generator output is backlogged
  (helm idea #3358).

## Traps worth remembering

- **`enabled` absent means `true`.** 242 of 659 recipes omit it, and
  `#[serde(default)]` on a `bool` yields the opposite — it would have silently
  marked iron-plate, copper-plate, transport-belt and stone-furnace
  research-locked. `jq`'s `//` operator hides this too, since it treats an
  explicit `false` as null-ish; use `has()`. The regression suite pins the
  aggregate (326 locked) so a regression reports 558 and fails loudly.
- **Factorio serializes an empty Lua table as `{}`**, so `ingredients`/`results`
  are not reliably arrays. `biter-egg` is a *real* ingredient-free recipe that
  hits this, not a placeholder — handled as a general rule rather than by name.
- **Tile size needs per-axis fallback.** Some prototypes declare only one of
  `tile_width`/`tile_height` (`railgun-turret`, `half-diagonal-rail`).
- **`.speed` is not a belt marker** — robots have it too. Belt throughput keys on
  the prototype-type.
- Neither `round(selection_box)` nor `ceil(collision_box)` alone works as a
  size rule; both break `train-stop`, which declares 2×2 despite a 1×1 collision
  box because trains pass through it.

## Verification

- Full workspace suite green (304 tests), including new regression suites in
  `crates/grid/tests/prototype_regression.rs` and
  `crates/solver/tests/recipe_regression.rs`.
- Live-install sweep on the gaming desktop: **169 icons resolved, decoded and
  cropped, 0 failures** (`cargo test -p factorio-ui -- --ignored`).
- **Not done:** the pixel screenshot of the rendered viewport. It needs a
  headless compositor (`cage`/`sway`) that isn't installed on the desktop, and
  the alternative — launching against the live Wayland session — would put a
  window on the user's monitor.

## Regenerating after a game update

```
factorio --dump-data --dump-prototype-locale     # writes to ~/.factorio/script-output/
cargo run -p factorio-dump-ingest -- \
  --dump <dir>/data-raw-dump.json --locale-dir <dir> \
  --out-prototypes crates/grid/data/prototypes.json \
  --out-recipes crates/solver/data/recipes.json
cargo test --workspace
```

The count assertions in the regression suites are pinned to 2.0.77 (169 entities,
649 recipes) and are expected to need updating on a content patch — review the
diff rather than adjusting until green.

## Next

The production-chain calculator: walk `Recipe` graphs and match them to machines
via `crafting_categories`.
