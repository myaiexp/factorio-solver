# No Active Phase

Phases 1-8 complete. No phase 9 planned yet.

## Completed phases

- **Phase 1**: Blueprint Foundation — decode/encode/round-trip (`001-blueprint-foundation.md`)
- **Phase 2**: Grid Engine — spatial placement, collision, import (`002-grid-engine.md`)
- **Phase 3**: Basic UI — egui viewport, pan/zoom, tooltips, entity colors (`003-basic-ui.md`)
- **Phase 4**: Game Data Foundation — dump-derived entities + recipes, live icons (`004-game-data-foundation.md`)
- **Phase 5**: Production Chain Calculator — `ChainGoal` → `ProductionPlan`, UI panel (`005-chain-calculator.md`)
- **Phase 6**: Block Generator — `ProductionPlan` → `Grid` → blueprint string, Generate + copy in the UI (`006-block-generator.md`)
- **Phase 7**: Columnar Block Topology — cells instead of rows, so a block delivers its target rate instead of half (`007-columnar-block-topology.md`)
- **Phase 8**: Save-Derived Machine Availability — read the player's unlocked recipes out of a save, so the solver only proposes what they can build (`008-save-derived-availability.md`)

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
- **Availability-aware recipe picker** (#3385) — Phase 8 gates the *solver* but
  not the UI's own pickers, so a locked recipe is still offered and only
  dead-ends at Solve.
- **Blueprint book support** — currently shows an error message, not parsed.

## Verified, in-game, still outstanding

Two things no test replaces, both needing a machine with Factorio on it:

- **Pasting a generated block into Factorio and confirming it runs at rate.**
  Phase 7's blocks pass every unit and integration check, including a
  delivered-rate check measured from the placed grid — but the check that found
  the original half-rate bug was a human watching a belt, and it has not been
  done for the columnar version.
- **Decoding a real save.** Phase 8 has never run against one; the build
  machine has no Factorio install. Run
  `FACTORIO_SAVE_FIXTURE=~/.factorio/saves/2026.zip cargo test -p factorio-save`
  on the desktop. Until then the `level-init.dat` header layout (version field,
  mod list) is an inferred guess validated only by fixtures this repo also
  wrote, and the calibration search has only ever run against synthetic data.
