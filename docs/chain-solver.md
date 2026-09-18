# Recipe and Chain Solver Decisions

Lasting decisions about recipe ingest, recipe selection, and the rate solver.

> The sections below were moved verbatim out of `CLAUDE.md` on 2026-09-18 (idea #4770), which now keeps only a pointer to each. This doc is the record: add new detail here, not to `CLAUDE.md`.

## Decisions

- **Recipe defaults come from Factorio, not Rust**: `enabled` absent → **`true`** (`#[serde(default)]` on a bool gives the opposite and would mark 242 recipes research-locked), `category` absent → `"crafting"`, `energy_required` absent → `0.5`. `ingredients`/`results` accept an array, an empty object `{}` (Factorio's empty-Lua-table serialization, hit by the real `biter-egg`) or null; a *populated* object is an error
- **Recipe filtering**: the 10 `parameter: true` placeholders are skipped, the 319 `hidden: true` ones are kept with the flag (Space Age recycling recipes are hidden but real)
- **Yields are `amount × probability`, never `amount`** (`ItemAmount::effective_amount`): 103 recipes weight a result, and `uranium-processing` gives both outputs `amount: 1` with the whole 0.007/0.993 split in `probability`. Absent → 1.0; a present-but-wrong-typed or out-of-range value is an ingest error, since defaulting past it turns a 0.7% chance into a certainty
- **`main_product` is a demotion signal, not a selector**: only 8 of 649 recipes declare a non-empty one (Factorio's `""` means "no single main product" and stores as `None`). It can prove a result is *secondary*; it can never identify a primary. Selection rests on the 420 single-result recipes instead — a filter keyed on `main_product == item` would exclude `uranium-processing` from its own outputs
- **The goal declares a boundary, not a recipe**: `ChainGoal.available` is the bus, and resolution walks back from the product until it reaches it. "One assembler" and "the whole chain" are the same code path — only `available` differs. Fluids fall out of this for free: a fluid ingredient not in `available` is an error telling the user to declare that recipe's product instead
- **Recycling is filtered by `category.starts_with("recycling")`, never `== "recycling"`**: 310 recipes are exactly `recycling`, but `scrap-recycling` is `recycling-or-hand-crafting` and outputs 10+ common items. Exact equality makes nearly every common item look ambiguous. Applies to both `chain::select` and the UI recipe picker
- **Ambiguity is always an error, never a guess** (`AmbiguousRecipe` listing candidates, resolved via `recipe_overrides`). 43 items have several producers — `copper-cable`, `iron-plate` and `copper-plate` among them — so the headline green-circuit case needs an override, which the UI offers as a button per candidate
- **The rate solver nets a recipe's own item against itself before dividing** (`chain::solve::net_yield`). That single subtraction is what makes a self-consuming recipe (kovarex: 40 U-235 in, 41 out) resolve in one division instead of iterating toward a limit. Cross-recipe cycles have no closed form and hit the iteration cap as `DidNotConverge` — never a silent partial plan
- **`enabled` is not a selection filter**: research-locked recipes (`uranium-processing`) are legitimate goals
- **Ingest is strict and loud**: a missing required field, an unparseable `energy_usage`, or a present-but-wrong-typed field aborts naming the entity/recipe. A silent partial write from a rarely-run tool would poison every downstream phase
