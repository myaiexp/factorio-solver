# No Active Phase

Phases 1-6 complete. No phase 7 planned yet.

## Completed phases

- **Phase 1**: Blueprint Foundation — decode/encode/round-trip (`001-blueprint-foundation.md`)
- **Phase 2**: Grid Engine — spatial placement, collision, import (`002-grid-engine.md`)
- **Phase 3**: Basic UI — egui viewport, pan/zoom, tooltips, entity colors (`003-basic-ui.md`)
- **Phase 4**: Game Data Foundation — dump-derived entities + recipes, live icons (`004-game-data-foundation.md`)
- **Phase 5**: Production Chain Calculator — `ChainGoal` → `ProductionPlan`, UI panel (`005-chain-calculator.md`)
- **Phase 6**: Block Generator — `ProductionPlan` → `Grid` → blueprint string, Generate + copy in the UI (`006-block-generator.md`)

## Next up

Nothing planned. The strongest candidates, in rough order of how much they
unlock, all live in the backlog (`helm idea list factorio-solver`):

- **Belt routing between steps** (#3362) — the generator stacks a producer
  directly above its consumer but does not connect them. `crates/grid/src/astar.rs`
  already has `find_path`. This is the piece that turns a block you finish by
  hand into one you paste and walk away from.
- **Second input belt via long-handed inserters** (#3359) — unlocks the 124
  recipes with three or more item ingredients, `advanced-circuit` among them.
- **Blueprint book support** — currently shows an error message, not parsed.
