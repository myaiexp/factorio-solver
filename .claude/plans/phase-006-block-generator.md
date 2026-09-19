# Phase 6 — Block Generator

**Status:** complete
**Design:** `.claude/plans/2026-08-11-chain-calculator-and-block-generator-design.md`
**Plan:** `.claude/plans/2026-08-11-phase6-block-generator-plan.md`

Turns a `ProductionPlan` into a `Grid` of real entities and out to a blueprint
string you can paste into the game. `solver::layout` is the whole of it:
`ProductionPlan` → `Grid` → the existing `factorio_grid::to_blueprint`.

## What shipped

| Module | Does |
| --- | --- |
| `layout/mod.rs` | `LayoutConfig`, `generate`, `generate_with_report`, cyclic rejection |
| `layout/lanes.rs` | Per-lane throughput, lane counts, ingredient→belt packing |
| `layout/rows.rs` | Machines, inserters and belts for one step |
| `layout/power.rs` | Pole placement and coverage checking |
| `layout/validate.rs` | Pre-emit hard checks and soft warnings |
| `layout/error.rs` | Every named refusal |

The headline case, 45/s green circuits from plates on assembling-machine-2,
generates a 46×73 block: 75 assemblers, 450 belts, 180 inserters, 33 medium
poles, no power coverage gaps, and it survives the
`to_blueprint → encode → decode → from_blueprint` round-trip with nothing
dropped.

## Geometry

Each step is one or more bands, stacked downward in the plan's topological
order (producers above consumers) with one blank row between:

```
input belt          ← 1 row, all facing East
input inserters     ← one per distinct ingredient, per machine
machines            ← machine_height rows, side by side
output inserters    ← one per distinct product, per machine
output belt         ← 1 row
```

A step splits into parallel sub-rows exactly when its rates need more belts
than one row's inserters can reach — an inserter reaches exactly one belt, so
belts and sub-rows are the same number. Machines divide as evenly as possible,
the remainder going to the first sub-rows.

## Decisions worth keeping

- **Belts and sub-rows are the same thing.** Because a row's inserters reach
  only the belt beside them, every belt of a step must carry that step's whole
  ingredient set. So a shared pair of items costs `max(lanes)` belts, not
  `ceil(total_lanes / 2)` — 45/s iron plate beside 135/s copper cable is 6
  belts, matching the design's own arithmetic. An item alone on a belt takes
  both lanes, which is where "cable alone needs only 3" comes from.
- **Inserter orientation is derived, never assumed.** The four cardinals are
  searched until the prototype's own rotated `pickup_position`/`insert_position`
  land on the wanted cells. Factorio's unrotated inserter picks from the north
  and drops to the south, so an inserter's `direction` points at what it takes
  *from* — the opposite of most people's intuition, and the reason this is
  derived rather than written down.
- **Pole coverage uses the game's rule: overlap, not containment.** Requiring a
  machine's every cell inside the supply area would make a 3-wide machine
  impossible to cover with a small pole (reach 2.5 clears only 2 cells past the
  pole's column) and retire small poles for no safety gain.
- **Each pole is scored by its own prototype's reach**, not the config's, so a
  grid carrying poles this module did not place is judged honestly.
- **Fluids are refused by the layout, not just by the calculator.**
  `chain::solve` only rejects a fluid *ingredient* that is off the bus; a fluid
  the user declared available, or any fluid a recipe *produces*, arrives at the
  layout looking like an ordinary item rate.
- **Cyclic steps are refused by name.** Checked on the recipe's own
  ingredient/result overlap rather than on the step's derived rates.
- **Two ingredients maximum per step**, because one belt has two lanes. Three
  needs a second belt reached by long-handed inserters — idea #3359.

## Known gaps (all backlogged)

- Steps are not belted to each other; the player wires the block into the bus
  and between steps by hand (#3362).
- Three-plus-ingredient recipes are refused (#3359).
- Inserter throughput is not modelled (#3360).
- Burner vs electric machines are indistinguishable in the registry (#3361).
- No snap-to-grid on the emitted blueprint (#3363).
