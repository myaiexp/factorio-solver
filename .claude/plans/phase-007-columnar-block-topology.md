# Phase 7 — Columnar Block Topology

**Status:** Complete
**Design:** `.claude/plans/2026-08-11-columnar-block-topology-design.md`
**Plan:** `.claude/plans/2026-08-11-columnar-block-topology-plan.md`
**Fixes:** idea #3364. Subsumes #3359.
**Supersedes:** the row topology from Phase 6. The chain calculator is untouched.

## Why

Phase 6 shipped a generator whose blocks pasted and ran, but not at the rate
on the tin. Confirmed in-game: **an inserter always places on the far lane of
a belt. There is no near-lane fallback.**

The old lane model had no concept of which lane anything landed on —
`pack_lanes` was pure capacity arithmetic, and it gave a lone item both lanes
of a belt. Sound for an input belt the player fills from the bus; wrong for an
output belt our own inserters fill. Every output belt in every generated block
was therefore provisioned at twice what it could carry, and the internal
intermediates were starved the same way, so the consuming machines ran at half
rate too.

Not a tuning problem: one machine row can only ever fill one lane, so no
arrangement of rows fixes it. The unit of layout had to change.

## What was built

The unit is now a **cell**: two machine columns sharing a spine of belts, with
an edge belt on each side. Cells tile horizontally and share their edge belts,
so every edge belt has a machine column on both sides — which is exactly where
double-siding is needed.

| Module | What it owns |
| --- | --- |
| `layout/lane.rs` | `drop_lane` (the far-lane rule as geometry over two cells), `LaneSide`, `lane_throughput` |
| `layout/cell.rs` | `CellTopology`, `CellPlan`, `size_step` — pure arithmetic, no `Grid` |
| `layout/place.rs` | `place_cell` — one cell's machines, inserters, belts and reserved pole rows |
| `layout/tile.rs` | `place_step` — machines across cells, cells across bands |
| `layout/validate.rs` | the three Phase 6 hard checks plus `check_delivered_rate` |

`layout/lanes.rs` and `layout/rows.rs` were deleted, along with
`belt_capacity_warnings` — keeping either would have left two answers to one
question.

## What it produces

Green-circuit plan (45/s electronic circuits from plates on
`assembling-machine-2`), express belts + medium poles + fast inserters:

- 53 × 57 tiles, 843 entities, 75 machines, 46 poles
- `electronic-circuit`: 4 claimed product lanes → 90/s against a 45/s goal
- `copper-cable`: 8 claimed product lanes → 180/s against a 135/s goal

Both over-provisioned because the ingredient side bound the cell sizing, which
is the right direction to err — and is now asserted rather than assumed.

## Decisions worth keeping

Condensed into `CLAUDE.md`'s "Decisions from previous phases". In brief:

- The far-lane rule lives in exactly one place, stated as geometry over two
  cells rather than derived from an inserter's `Direction`.
- Sizing starts at the belt, not at `machines_needed`.
- Ingredient lanes are per cell, product lanes per column.
- Lane allocation is exhaustive search, not proportional-with-rounding.
- One inserter per (stream, *belt*), not per stream.
- `CellTopology` is user configuration, because which arrangement wins depends
  on whether a step is input- or output-bound.
- A column reserves a pole row periodically; a vertical pole column is
  geometrically impossible.
- Delivered capacity is counted per belt run, never per belt tile.

## Deliberate regressions

- **A step with two or more products is now refused** (`MultipleProducts`).
  `uranium-processing` (1 ingredient, 2 item results, no fluids) laid out under
  the row topology and no longer does. What it built was wrong anyway — both
  output inserters dropped on the same far lane.

## Found during implementation, not in the design

Wiring `generate` end to end exposed a defect no unit test could see: **the
block had nowhere to put a power pole.** The green-circuit step needs 3
ingredient inserters per machine and `assembling-machine-2` is 3 tall, so both
ingredient gutters were solid for the column's full 24 rows. A medium pole in
the product gutter reaches x ∈ [-2, 5] and the inserter it must cover sits at
exactly x = 5 — Factorio powers on overlap, so it missed by one tile.

A vertical pole column cannot be inserted anywhere, because every column in a
cell is load-bearing. The fix is a reserved horizontal row, derived from the
configured pole's own supply distance. See the `place.rs` comment; it is the
thing a future reader will otherwise try to optimise away.

## Not done

- **In-game paste at rate.** The one test no unit test replaces, and the check
  that found the original bug. Left for the player.
- Belt routing between steps (#3362), direct insertion (#3354), fluids (#3355),
  modules/beacons/quality (#3351), multi-product steps, inserter throughput
  (#3360).
