# Columnar Block Topology — Design

> Replace the block generator's belt-fed rows with columnar cells, so that a
> generated block actually delivers its target rate.

**Date:** 2026-08-11
**Status:** Approved (design), pending implementation plan
**Supersedes:** the row topology in `.claude/plans/2026-08-11-chain-calculator-and-block-generator-design.md` (Phase 6). The chain calculator half of that document is unaffected.
**Fixes:** idea #3364. Subsumes #3359.

---

## Why

Phase 6 shipped a generator whose blocks paste and run, but not at the rate on
the tin. Confirmed in-game: **an inserter always places on the far lane of a
belt. There is no near-lane fallback.**

The lane model has no concept of which lane anything lands on — `pack_lanes` is
pure capacity arithmetic. It gives a lone item both lanes of a belt, which is
sound for an input belt the player fills from the bus and wrong for an output
belt our own inserters fill. Every output belt in every generated block is
therefore provisioned at twice what it can carry.

Worked: 45/s electronic circuits on yellow belts is 6 output lanes, emitted as
3 belts. Each of those belts can only ever carry one lane, 7.5/s, so the block
delivers 22.5/s against a 45/s goal. The internal copper cable is starved the
same way, so the circuit machines are fed at half rate too.

This is not a tuning problem. The row topology has one machine row per belt,
and one machine row can only ever fill one lane, so no arrangement of rows
fixes it. The unit has to change.

---

## The rule everything follows from

A belt has two lanes. An inserter **picks** from either lane, and **drops** on
the far one — the lane furthest from itself. Two consequences carry the whole
design:

- A belt filled by inserters needs a machine column on **both** sides to use
  both of its lanes.
- A belt filled from the bus does not, because the player fills both lanes.

So the constraint binds only on belts carrying our own products, never on
belts carrying ingredients.

---

## The cell

The unit of layout stops being a row and becomes a **cell**: two machine
columns sharing a spine of belts, with an edge belt on each side.

```
 edge belt │ inserters │ column A │ inserters │ spine │ inserters │ column B │ inserters │ edge belt
     ▲                                            ▲                                          ▲
     │                              1–2 belts, shared by A and B                             │
     └──────────── shared with the neighbouring cell, so both lanes fill ───────────────────┘
```

Cells tile horizontally and share their edge belts, so **every edge belt has a
machine column on both sides** — which is exactly where double-siding is
needed. The spine is shared by the cell's own two columns.

Reach follows from adjacency: the belt next to a column is served by an
ordinary inserter, and a second belt one further out needs a
`long-handed-inserter` (`pickup_position` -2.0, `insert_position` 2.2, already
in the registry). That is what makes 3- and 4-ingredient recipes fit, and it is
why idea #3359 stops being a separate feature.

### The topology is configuration, not a constant

Middle-feed and outer-feed have opposite strengths — sharing the spine helps an
input-bound step, sharing the edge helps an output-bound one — and which wins
depends on the recipe. So the arrangement is a parameter the user can change
and re-solve against, not a decision baked into the generator.

```rust
pub struct CellTopology {
    /// Belts on the spine between a cell's two machine columns. 1 or 2.
    pub spine_belts: u8,
    /// Belts on the cell edge, shared with the neighbouring cell. 1 or 2.
    pub edge_belts: u8,
    /// Which side carries ingredients. The other carries products.
    pub ingredients_on: Side,
    /// Tile cells left to right until adding one would exceed this, then wrap
    /// to a new band below. `None` lays every cell in a single band.
    pub target_width: Option<u32>,
}

pub enum Side { Spine, Edge }
```

`spine_belts == 2` or `edge_belts == 2` implies long-handed inserters for the
outer of the pair; the generator picks the inserter per belt from its distance,
rather than the user choosing.

---

## The arithmetic, inverted

Today's generator starts from `machines_needed` and asks how many belts that
needs. That is the wrong direction: **a belt's throughput caps how many
machines a column can feed**, so the calculation starts from the belt.

### Ingredient lanes are per cell; product lanes are per column

That asymmetry is the whole design in one line, and it is the same rule as
before seen from the other end: **picking shares a lane, dropping owns one.**

- Both columns of a cell pick from the same ingredient belts, so the *cell* has
  `2 × B_ing` ingredient lanes to divide up, where `B_ing` is the belt count on
  whichever side `ingredients_on` names.
- A column drops on the far lane of each product-side belt it can reach, and no
  other column can use that lane. So each *column* owns `B_prod` product lanes.

This holds for every `CellTopology`, not just the default, because it is stated
in terms of which side each stream is on rather than in terms of spine and
edge. A spine belt is shared between a cell's two columns; an edge belt is
shared between two neighbouring cells' columns. Either way each belt has a
column on both sides, which is what makes both its lanes fill.

```
cell_cap   = min over ingredients i of (lanes_i × lane_throughput) ÷ rate_i
column_cap = min over products    p of (B_prod  × lane_throughput) ÷ rate_p

machines_per_cell = min( floor(cell_cap), 2 × floor(column_cap) )
```

`floor` on each, because half a machine cannot be built. A cell's two columns
then split `machines_per_cell` as evenly as possible, the extra machine going
to the first — the same idiom `rows.rs` already uses for sub-rows. The longer
column can never exceed `column_cap`, since `machines_per_cell` is capped at
twice it. Cells are tiled until every machine of the step is placed, so the
last cell may be partly filled; a partly-filled cell is under its caps by
construction and needs no special handling.

### Allocating ingredient lanes

`lanes_i` is not proportional-with-rounding. The search space is tiny — at most
4 lanes among at most 4 ingredients — so the allocation is chosen by
**exhaustive search for the one that maximises `cell_cap`**, with every
ingredient getting at least one lane. Ties go to the earlier ingredient in
recipe order, for determinism.

Proportional allocation would need a rounding rule, and the rounding rule is
where the arithmetic would quietly stop being optimal: the worked example below
divides evenly (1.5 : 4.5 is exactly 1 : 3 over 4 lanes), but the 3- and
4-ingredient recipes this design exists to unlock generally do not. Searching
sidesteps the question rather than answering it badly.

A recipe with more distinct ingredients than there are lanes is an error.

**Output is usually not the binding stream, but sometimes is.** An assembler
normally consumes more items than it emits, so the ingredients bind. 18
belt-only recipes emit *more* items than they consume — `copper-cable` and
`iron-stick` at 2×, asteroid crushing at 5–20× — and for those the product
binds. This needs no special case: it falls out of putting the product in the
same `min`.

### Worked example — the two binding cases, one each

45/s electronic circuits from plates on `assembling-machine-2` (1.5 crafts/s
per machine), default topology: `spine_belts: 2`, `edge_belts: 1`,
`ingredients_on: Spine`. So 4 ingredient lanes per cell and 1 product lane per
column.

| tier | step | machines/cell | columns | cells | bound by |
| --- | --- | --- | --- | --- | --- |
| express (22.5/lane) | electronic-circuit | 15 | 8 + 7 | 2 | ingredient (copper-cable) |
| | copper-cable | 14 | 7 + 7 | 4 | **product** |
| yellow (7.5/lane) | electronic-circuit | 5 | 3 + 2 | 6 | ingredient (copper-cable) |
| | copper-cable | 4 | 2 + 2 | 12 | **product** |

The circuit step is input-bound on cable; the cable step is output-bound on its
own 2× multiplier — one clean instance of each. Note the cable step is *not*
`floor(cell_cap)`: its copper-plate input would allow 60 machines per cell on
express, and its own product caps it at 7 per column.

### Worked example — why the topology is configurable

The same cable step on express with the topology flipped — `edge_belts: 1`,
`ingredients_on: Edge`, so products go on the 2-belt spine and each column owns
2 product lanes instead of 1:

| step | machines/cell | cells | bound by |
| --- | --- | --- | --- |
| copper-cable | 30 | 2 | ingredient (copper-plate) |

14 machines per cell becomes 30, and 4 cells become 2, purely by moving the
output to the side with more belts. That is the whole argument for exposing
`CellTopology` rather than picking one: an output-bound step wants its product
on the wide side, an input-bound step wants its ingredients there, and which a
step is depends on the recipe.

**Yellow belts** make the block wide rather than tall — 12 cells of 2-machine
columns for the cable step. `target_width` is what keeps that from becoming a
horizontal version of the ribbon it replaces.

---

## Error handling

Named and actionable, consistent with the existing `LayoutError`:

| condition | behaviour |
| --- | --- |
| More distinct ingredients than spine lanes | Error naming the recipe, its ingredients, and the lane count |
| A step with two or more products | Error. Each column owns its product lanes outright, so a two-product split across one lane has no representation. Same treatment as the cyclic step. **This is a regression**: the row topology lays `uranium-processing` out today (1 ingredient, 2 item results, no fluids) and it will start erroring. Deliberate — what it builds today is wrong anyway, since both output inserters drop on the same far lane. Must reach the changelog, not just this table |
| A stream whose per-machine rate exceeds one lane | Error naming the stream and the tier that would fix it — a single machine that cannot be fed has no layout at any column length |
| Cyclic step, fluid on a belt, unknown tier | Unchanged from Phase 6 |
| Partial trailing cell (odd column count) | Not an error. Its outer edge belt has one column, so it is accounted at one lane |

---

## Testing

- **The rule itself** — for every placed inserter, the cell its rotated
  `insert_position` lands on is a belt, and the lane it lands on is the far one
  relative to the inserter. This is the assertion Phase 6 lacked.
- **Both lanes of every edge belt are claimed**, by columns on opposite sides.
- **The two worked cases above**, asserted exactly: circuit input-bound at 15
  machines/cell on express, cable output-bound at 15.
- **The output-binding class** — `copper-cable` sizes on its product, not its
  ingredient, and the same block on a faster tier scales as the lane ratio says
  it should.
- **Delivered rate matches the goal** — sum the lane capacity actually reachable
  by the placed inserters for the final product and assert it meets the goal
  rate. This is the test that would have caught #3364.
- **Every `CellTopology` combination generates**, not just the default — the
  arithmetic is stated per side precisely so the other seven are not untested
  guesses. The flipped-topology cable numbers above are the pinned case.
- **Lane allocation is optimal and deterministic** — the searched allocation
  beats or matches proportional-with-rounding on a 3-ingredient recipe, and the
  same input always yields the same allocation.
- **Round-trip** — unchanged: every generated blueprint survives
  `to_blueprint → encode → decode → from_blueprint`.
- **In-game paste** — the one test no unit test replaces.

`validate.rs`'s `belt_capacity_warnings` goes away rather than being ported.
It exists to warn that a step's belts are over their rating, which is exactly
what sizing from the belt makes structurally impossible — and its whole
calculation is written in terms of the `sub_rows` concept that no longer
exists. The delivered-rate check above replaces its purpose.

---

## Out of scope

- **Direct insertion** between a producer and its consumer. It would sidestep
  belting copper cable entirely, which is attractive precisely because of the
  2× that makes cable output-bound. Deliberately deferred until the columnar
  version exists, so it is measured against something that works rather than
  against a half-rate block.
- **Belt routing between steps** (#3362). Steps remain separate blocks the
  player wires together.
- Modules, beacons, quality (#3351). Multi-product steps. Fluids.

---

## Alternatives rejected

- **Patch `pack_lanes` to give output belts one lane.** Corrects the arithmetic
  without correcting the topology: it doubles the row count of every
  output-bound step and leaves each belt half-used forever. It fixes the number
  and not the waste.
- **Output belt in the middle of a machine pair, inputs outside.** The first
  form considered, and sound — but it is one point in the space `CellTopology`
  now covers, so it becomes a setting rather than the design.
- **Keep rows, add wrapping for the aspect ratio.** Addresses only the shape,
  and the shape was the lesser of the two problems.
