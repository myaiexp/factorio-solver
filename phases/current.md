# No Active Phase

Phases 1-9 complete. No phase 10 planned yet.

## Completed phases

- **Phase 1**: Blueprint Foundation — decode/encode/round-trip (`001-blueprint-foundation.md`)
- **Phase 2**: Grid Engine — spatial placement, collision, import (`002-grid-engine.md`)
- **Phase 3**: Basic UI — egui viewport, pan/zoom, tooltips, entity colors (`003-basic-ui.md`)
- **Phase 4**: Game Data Foundation — dump-derived entities + recipes, live icons (`004-game-data-foundation.md`)
- **Phase 5**: Production Chain Calculator — `ChainGoal` → `ProductionPlan`, UI panel (`005-chain-calculator.md`)
- **Phase 6**: Block Generator — `ProductionPlan` → `Grid` → blueprint string, Generate + copy in the UI (`006-block-generator.md`)
- **Phase 7**: Columnar Block Topology — cells instead of rows, so a block delivers its target rate instead of half (`007-columnar-block-topology.md`)
- **Phase 8**: Save-Derived Machine Availability — read the player's unlocked recipes out of a save, so the solver only proposes what they can build (`008-save-derived-availability.md`)
- **Phase 9**: Recipe Availability Gate — edit that same set by hand, explain a refusal from the technology graph, and persist the panel's inputs (`009-recipe-availability-gate.md`)

## Next up

Nothing planned. The strongest candidates, in rough order of how much they
unlock, all live in the backlog (`helm idea list factorio-solver`):

- **Belt routing between steps** (#3362) — the generator stacks a producer
  directly above its consumer but does not connect them. `crates/grid/src/astar.rs`
  already has `find_path`. This is the piece that turns a block you finish by
  hand into one you paste and walk away from.
- **Direct insertion between a producer and its consumer** (#3354) — deferred
  out of Phase 7 deliberately, so it is measured against a block that works
  rather than against a half-rate one. Attractive precisely because it
  sidesteps belting copper cable, whose 2× yield is what makes it
  output-bound.
- **Inserter throughput** (#3360) — Phase 7 sizes belts correctly but still
  assumes an inserter can move whatever its machine needs. A green-circuit
  machine wants 6/s through one arm; a fast inserter carries about 6.4/s.
- **Blueprint book support** — currently shows an error message, not parsed.

## Verified, in-game, still outstanding

Two things no test replaces, both needing a machine with Factorio on it:

- **Pasting a generated block into Factorio and confirming it runs at rate.**
  Phase 7's blocks pass every unit and integration check, including a
  delivered-rate check measured from the placed grid — but the check that found
  the original half-rate bug was a human watching a belt, and it has not been
  done for the columnar version.
- ~~**Decoding a real save.**~~ **Done 2026-08-13** (idea #3399). The reader
  opens the player's current save and decodes 659 recipes, 275 technologies and
  369 unlocked recipes, matching `real_save.rs`'s ground truth exactly — so the
  inferred `level-init.dat` header layout and the calibration search are both
  confirmed against real data. Remaining: `mods()` still parses empty
  (idea #3400), which affects only the "is this vanilla" report.
- **Driving the availability UI by hand.** Phase 9's headless egui harness
  exercises the real render path and the reported case end to end, but nobody
  has clicked the tick list in a running window, and panel persistence is
  tested at the serde layer rather than across a real restart.
