# Phases 5–6: Production Chain Calculator + Block Generator — Design

> Turn "45/s green circuits, I have plates on the bus" into a blueprint string you can
> paste into the game.

**Date:** 2026-08-11
**Status:** Approved (design), pending implementation plan
**Depends on:** Phase 4 (game data foundation) — needs `recipes.json` and the expanded prototype registry
**Related:** `.claude/plans/2026-08-11-game-data-foundation-design.md`

---

## Context

The app is a native egui desktop app running on a second monitor alongside the game
(2.0.77, full Space Age, no mods — achievement run). Phase 4 supplies **649** real recipes
(659 in the dump minus 10 `parameter` placeholders) and 169 real entity prototypes. This
design covers the two phases that turn that data into something pasteable.

**The app does not model the player's base.** It generates a standalone block that the
player wires into their bus by hand.

---

## The central idea: the goal declares a boundary, not a recipe

There is no distinction between "single-recipe block" and "full chain". The user declares
what they already have, and the chain is resolved from the target back to that boundary.

```rust
pub struct ChainGoal {
    pub product: String,                          // "electronic-circuit"
    pub rate: Rate,                               // ItemsPerSec | ItemsPerMin | Belts(n, tier)
    pub available: HashSet<String>,               // bus contents — the chain stops here
    pub machines: MachinePolicy,                  // preferred machine per crafting category
    pub recipe_overrides: HashMap<String, String>,// item -> chosen recipe (43 ambiguous items)
}
```

| `available` contains | Result |
| --- | --- |
| `iron-plate`, `copper-plate` | builds cable assemblers **and** circuit assemblers |
| `iron-plate`, `copper-cable` | builds circuit assemblers only |
| `electronic-circuit` | nothing to build — it is already a bus item |

One code path serves all three. This is what makes "block" and "chain" the same feature.

### Fluids fall out of the boundary, rather than needing a rule

`plastic-bar` requires 20 petroleum-gas — a fluid, which cannot be belted. But
`advanced-circuit` with `plastic-bar` declared available needs no fluid at all.

**Rule:** the generator builds **belt-fed item chains**. A recipe whose ingredients include
a fluid must instead have its own product declared available. The error names the item and
says exactly that.

Honest cost: no oil refinery, chemical, or smelting-with-fluid blocks in these phases.
Upgrade path is pipes + fluid routing in a later phase, and nothing here precludes it.

---

## Phase 5 — `solver::chain`

### Output

```rust
pub struct ProductionPlan {
    pub steps: Vec<ProductionStep>,   // topologically ordered, producers before consumers
    pub inputs: Vec<ItemRate>,        // what the bus must supply
    pub byproducts: Vec<ItemRate>,    // surplus that must be dealt with
}

pub struct ProductionStep {
    pub recipe: &'static Recipe,
    pub machine: &'static EntityPrototype,
    pub exact_count: f64,             // fractional — the true ratio
    pub machines_needed: u32,         // ceil(exact_count)
    pub crafts_per_sec: f64,
    pub inputs: Vec<ItemRate>,        // per-step, for the layout phase
    pub outputs: Vec<ItemRate>,
}
```

`ProductionPlan` is the **entire interface** between Phase 5 and Phase 6. Phase 5 knows
nothing about geometry; Phase 6 knows nothing about recipes.

### Requirement: multi-output and cyclic recipes must be correct

A tree recursion cannot express these, and they are **not** all excluded by the no-fluids
boundary — this was checked specifically against the dump:

| Recipe | Why a tree fails | Fluid? |
| --- | --- | --- |
| `uranium-processing` | 1 ore → 0.993 U-238 **+** 0.007 U-235 | no — in scope |
| `kovarex-enrichment-process` | consumes and produces U-235 — a **cycle** | no — in scope |
| Asteroid crushing (metallic/carbonic/oxide) | multi-output, Space Age | no — in scope |
| `yumako-processing`, `jellynut-processing` | multi-output, Space Age | no — in scope |
| `advanced-oil-processing`, `coal-liquefaction` | multi-output | yes — out of scope |

32 recipes are multi-output overall. **The numerical method is deliberately left to the
implementer** — the requirement is correctness on the cases above, pinned by tests. A tree
recursion is explicitly insufficient and must not be used.

### Recipe selection

43 items have more than one producing recipe counting all sources (`copper-cable` 3,
`carbon` 4, `concrete` 3, `advanced-circuit` 2) — though those totals include recycling
recipes, which are filtered out before selection. `copper-cable`, for instance, has 3
producers in total but only **2** surviving candidates.

**Multi-output yields are probability-weighted.** 103 recipes declare a `probability` on
their results, and `uranium-processing` gives both outputs `amount: 1` with the whole split
in that field. Every rate calculation uses `amount × probability`.

**`main_product` cannot carry this rule** — it is declared on only **13 of 659** recipes,
and on none of the ambiguous items above. The load-bearing fact is instead that **420
recipes have exactly one result**.

Candidate set for item X = recipes where X appears in `results`, minus `hidden`, minus
`category == "recycling"`, minus recipes whose only claim on X is as a secondary output
(i.e. multi-result recipes that do not declare `main_product == X`).

Selection order:

1. An explicit entry in `recipe_overrides`.
2. Otherwise, if exactly one candidate remains, use it. A single-result recipe trivially
   targets its result, which resolves the ~600 unambiguous items.
3. If more than one candidate remains → error listing them. Never guess silently.

Worked case: `copper-cable` has two surviving candidates (`copper-cable` and
`casting-copper-cable`, both single-result) once recycling is filtered, so it errors and
demands an override. That is correct behaviour, not a failure.

### Machine selection

```rust
pub struct MachinePolicy {
    /// Explicit choice per crafting category, e.g. "electronics" -> "assembling-machine-2".
    pub preferred: HashMap<String, String>,
    /// When a category has no explicit entry: pick the highest crafting_speed machine
    /// whose crafting_categories contains it.
    pub fallback: MachineFallback,   // FastestAvailable | Named(String)
}
```

`machines_needed = ceil(rate / (machine.crafting_speed / recipe.energy_required))`

The machine is the one `MachinePolicy` resolves for the recipe's category, which must have
that category in its `crafting_categories`. If no machine covers the category, error naming
both. Category → machine coverage is dense in practice: `crafting` has 3 machines,
`electronics` 4, `smelting` 3, `chemistry` 1.

### Worked example (pins the maths)

45/s `electronic-circuit` on `assembling-machine-2` (speed 0.75), from plates:

- circuit: 0.5 s/craft → 1.5 crafts/s/machine → **30 machines**
- cable demand: 45 × 3 = 135/s; cable machine yields 2 per 0.5 s at 0.75 speed = 3/s → **45 machines**

The classic 3:2 ratio, checkable by hand.

---

## Phase 6 — `solver::layout`

`ProductionPlan` → `Grid` → the existing `to_blueprint` → blueprint string.

### Topology: belt-fed rows, deterministic geometry

Every step is a row (or rows) of machines. Inputs arrive on belts, outputs leave on belts,
inserters bridge machine and belt, power poles are interleaved to cover every machine.

**No direct insertion.** Explicitly excluded. Direct insertion is a compactness trick that
only pays off in malls and the single copper-cable → green-circuit case; at production scale
it imposes rigid ratios, blocks tiling, and adds inserter-throughput edge cases. Belt-fed
throughout keeps the geometry uniform. This is a per-step layout decision, not an
architectural one, so it can be added later without rework.

Its measurable cost is surfaced, not hidden: 45/s green circuits from plates carries 135/s
of cable internally — 3 blue belts against a single blue belt of output. The user's two
escapes are already in the model: declare `copper-cable` available, or raise the belt tier.

**No A\*.** Geometry is computed, not searched. This is what lets Phase 6 ship before a belt
router exists, and it is why it cannot emit the subtly-broken blueprints a half-working
router would.

### Belt lanes are computed, not assumed

A step's demand routinely exceeds one belt, and this is the normal path, not a warning case.

**A Factorio belt carries two independent lanes**, each holding a different item, so one
belt can feed two ingredients — which is exactly how the real green-circuit build works
(iron plate on one lane, copper cable on the other). Two consequences:

- **Per-lane throughput is half the belt's rating.** Blue belt is 45/s, so 22.5/s per lane.
- **Lane count for an item = `ceil(rate / (belt_throughput / 2))`**, and belts needed for a
  step is derived from how its ingredients pack into lane pairs.

Reworking the headline example with this: 135/s of cable is `ceil(135 / 22.5)` = **6 lanes**.
Sharing each belt with iron plate (45/s → 2 lanes) gives 6 belts, and dedicating both lanes
of a belt to cable gives 3. The generator computes this; the point is that a single-belt
implementation fails the design's own centerpiece example.

**v1 packing rule:** at most two ingredients per belt, one per lane. Where an item needs
more lanes than one belt provides, the step is split into parallel machine sub-rows, each
with its own belt pair — that is how it is built in-game, and it keeps every inserter
adjacent to exactly one belt.

The throughput *warning* in validation is for the residual case where a single physical
segment still exceeds its rating after this splitting (e.g. a shared input trunk).

### Cyclic steps are rejected by the layout phase

Phase 5 solves cycles correctly (`kovarex-enrichment-process` consumes 40 U-235 + 5 U-238
and produces 41 U-235 + 2 U-238 — it both consumes and produces U-235). Phase 6 has **no
spatial realization** for a step that consumes its own output: in-game that needs a
self-loop return belt carrying the recycled majority, which is a distinct topology from
every other step.

**Decision:** `solver::layout` **refuses** a `ProductionPlan` containing a self-consuming
step, with a named error identifying the step and item. The calculator still answers the
ratio question correctly — you just cannot get a blueprint for it yet. Deferring this is
deliberate: inventing a half-working self-loop topology is exactly the class of subtly
broken output this phase is designed to avoid.

### Validation before emitting

- Every machine has a reachable input and output path.
- No overlapping entities (the `Grid` already enforces placement collision).
- No single belt segment exceeds its tier's throughput after lane splitting — otherwise
  warn, naming the tier that would fix it.
- Every machine is within `supply_area_distance` of a power pole. This field is supplied by
  Phase 4's extended `EntityPrototype` (small pole 2.5, medium 3.5, substation 9) — it is a
  real prototype value, not a hardcoded spacing constant.

---

## Data flow

```
ChainGoal ──> solver::chain ──> ProductionPlan ──> solver::layout ──> Grid
                   │                                                    │
            recipes.json                                          to_blueprint
         prototype registry                                             │
                                                                 blueprint string
```

---

## Error handling

All of these are **named, actionable errors** — never silent fallbacks, because a silently
wrong blueprint costs the player real in-game time to discover:

| Condition | Behaviour |
| --- | --- |
| Recipe needs a fluid ingredient | Error naming the fluid and the recipe, telling the user to declare that recipe's product available |
| Ambiguous recipe | Error listing candidates; resolved via `recipe_overrides` |
| No machine for a crafting category | Error naming category and recipe |
| Unreachable boundary (no path from target to `available`) | Error naming the item that dead-ends |
| Belt throughput exceeded | Warning with the tier that would fix it; still emits |
| Byproducts produced | Warning listing them and their rates; still emits |
| Plan contains a self-consuming (cyclic) step | **Layout refuses**, naming the step and item. Phase 5 still returns correct ratios |

---

## Testing

- **Hand-checkable ratios** — the 3:2 cable:circuit example above, asserted exactly.
- **Multi-output** — `uranium-processing` produces both U-238 and U-235 at the right rates.
- **Cycle** — `kovarex-enrichment-process` terminates and balances in Phase 5 (it consumes
  40 U-235 + 5 U-238 and yields 41 U-235 + 2 U-238, so it nets +1 U-235 / −3 U-238 per
  batch), **and** Phase 6 rejects the resulting plan with a named error rather than emitting
  a broken layout.
- **Belt lanes** — 135/s of cable on blue belts resolves to 6 lanes (22.5/s per lane), not
  one belt plus a warning, and the layout reflects that.
- **Boundary semantics** — the same goal with `available = [plates]` vs `[copper-cable]`
  yields 2 steps vs 1, from one code path.
- **Fluid rejection** — a `plastic-bar` goal errors with the actionable message; the same
  goal with `plastic-bar` available succeeds.
- **Ambiguity** — a `copper-cable` goal without an override errors listing all 3 recipes.
- **Layout** — generated grids have zero overlapping entities; every machine has an
  inserter and pole in range.
- **Round-trip** — every generated blueprint survives `to_blueprint` → `encode` → `decode`
  → `from_blueprint` unchanged. This is the end-to-end guarantee that it will paste.
- **In-game verification** — paste a generated green-circuit block into a real save and
  confirm it runs. The one test no unit test replaces.

---

## Out of scope

- Fluids, pipes, and fluid routing (the boundary rule above covers the gap).
- Direct insertion (excluded above).
- **Spatial layout of cyclic steps** — Phase 5 computes them, Phase 6 refuses them. A
  self-loop return-belt topology is a later phase.
- A* belt routing between blocks; main-bus construction; city blocks.
- Modules, beacons, quality tiers — idea #3351.
- Trains, logistics bots, and any non-belt item movement.
- Modelling the player's existing base.

---

## Alternatives rejected

- **Tree recursion for the calculator.** Simpler, and fine until the first multi-output or
  cyclic recipe — which, as shown above, exist even with fluids excluded. It would be
  rewritten rather than extended, so it is not a stepping stone.
- **Direct insertion in v1.** See above — user's call, with sound reasoning.
- **A\* routing in v1.** The hard part, and unverifiable until deterministic layout works.
- **Solving layout and ratios together.** Keeping `ProductionPlan` as a pure data interface
  means the calculator is testable with no geometry and the layout is testable with a
  hand-written plan.
