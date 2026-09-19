# Phase 5 — Production Chain Calculator

**Status:** Complete
**Design:** `.claude/plans/2026-08-11-chain-calculator-and-block-generator-design.md`
**Plan:** `.claude/plans/2026-08-11-phase5-chain-calculator-plan.md`

## Goal

Turn a `ChainGoal` ("45/s electronic circuits, I have plates on the bus") into a
`ProductionPlan` listing which recipes to run, on which machines, and how many.

## What shipped

- `solver::chain` — `ChainGoal`/`ProductionPlan`/`ChainError` (`mod.rs`, `error.rs`),
  recipe + machine selection (`select.rs`), the rate solver (`solve.rs`).
- `ui::chain_panel` — side panel driving it, with an override escape hatch for
  ambiguous recipes.
- `ItemAmount.probability` + `Recipe.main_product` added to the recipe model and
  the dump ingest; `recipes.json` regenerated from the same 2.0.77 dump.

## Prerequisite the plan assumed but did not have

The plan lists `probability` and `main_product` as Phase 4 deliverables. Neither
was ingested: `item_amount.rs` documented `probability` as an ignored extra key,
and `main_product` was never read. Both are load-bearing here —
`uranium-processing` declares both its results as `amount: 1` and carries the
entire 0.007/0.993 split in `probability`, so the whole multi-output requirement
was unimplementable until the field existed. Fixed first, in its own commit.

## Corrections to the plan's test cases

Six of the plan's tests omitted recipe overrides they need. `copper-cable`,
`uranium-235`, `uranium-238` and `plastic-bar` are all ambiguous, so those goals
error on `AmbiguousRecipe` before reaching the maths under test — including the
headline `green_circuits_from_plates_is_30_and_45`. The design *does* say
copper-cable errors and demands an override ("correct behaviour, not a
failure"); the plan's tests just did not carry it through.

Two fixture facts were also wrong:

- `iron-ore` was the example of a goal with no recipe. Space Age asteroid
  crushing produces it, so it is ambiguous, not unreachable. `wood` is used.
- `molten-iron` was the fixture for `main_product` demotion, but it has a single
  result and cannot exercise the rule. `copper-bacteria` (also yields
  `spoilage`) does.

## Numerical method

Demand propagation with surplus netting, not a linear solve. Each recipe's own
item is netted against itself before dividing, which resolves a self-consuming
recipe exactly in one step rather than iterating. Byproducts fall out as
leftover surplus.

Consequence worth knowing: for a coupled cycle this is greedy rather than
optimal. The kovarex chain lands on 1.0 kovarex crafts/s plus a small U-235
byproduct, where an exact system solve would give ~0.979 and no surplus. Both
are physically valid; the greedy answer overproduces slightly and reports the
surplus honestly.

## Deliberately not done

- Spatial layout — Phase 6. `ProductionPlan` is the entire interface.
- Fluid routing. A fluid ingredient not on the bus is a named error.
- Modules, beacons, quality tiers.
- Per-category machine pinning in the UI (`MachinePolicy::with_preference`
  exists and is tested; the panel only exposes fastest/named).
