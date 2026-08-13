# Recipe Availability Gate — Design

> The solver proposes recipes and machines the player cannot build. This gates
> both on a set of unlocked recipes, sourced however we can get it, persisted
> across runs.

Status: approved 2026-08-13. Closes the consuming half of idea #3356; folds in
idea #3380 (panel state persistence). Deliberately source-agnostic so the
save-file import in #3382 becomes a second source rather than a rewrite.

## Problem

`chain::select` considers every recipe in the registry. On a Space Age dump a
mid-game player asking for chemical science hits four `AmbiguousRecipe` errors
in a row — `iron-plate`, `copper-cable`, `iron-gear-wheel`, `pipe` — each
because a `casting-*` recipe exists beside the assembler one. All four are
unlocked by a single technology, `foundry` (behind `calcite-processing` and
`tungsten-carbide`, i.e. deep Vulcanus). The player is being asked to arbitrate
between a recipe they use daily and one they cannot build.

Machine selection has the same hole and it matters more: `MachinePolicy::
fastest()` picks the fastest prototype covering a crafting category with no
regard for buildability, so a chemical-science plan currently assigns three of
its seven steps to **electromagnetic plants** (Fulgora). The blueprint is
unpasteable for most of a playthrough.

## Why the core is keyed on recipes

The obvious model is a set of researched *technologies*, deriving recipes
through unlock edges. Idea #3382 rules that out as the primitive: a save file's
per-recipe unlocked flags are readable (2.0.32+, self-calibrating, failing
loudly on layout drift), while researched technologies are a firm negative
across three rejected hypotheses. Recipes are what any source can actually
produce.

So the core takes a **recipe set**, and every way of obtaining one — a manual
edit, a technology projection, a save import — is a source feeding the same
model. That is the single decision this design turns on, and it is what keeps
the import in #3382 from being a rewrite of the solver.

## Architecture

```
                        ┌── manual editing (this phase)
unlocked recipe set ←───┼── technology projection (later, optional)
        │               └── save-file import (#3382, other session)
        ↓
solver::availability ──→ chain::select / chain::solve ──→ errors
        ↓
eframe persistence
```

The technology graph is still ingested, but for a different job than input: it
holds the unlock edges that turn "you cannot build this" into "this needs
`foundry`". Explanation, not source.

Nothing here reaches `layout` — the gate constrains which plan gets built,
never how a plan is laid out.

## Components

### 1. `solver::availability` — the model

New module `crates/solver/src/availability.rs`.

```rust
pub enum Availability {
    Everything,                     // default: today's behaviour
    Unlocked(BTreeSet<String>),     // recipe names
}
```

`BTreeSet` rather than `HashSet` so any error listing recipes is deterministic,
the same reason `candidates_for` sorts.

| Recipe                                | Available under `Everything` | Available under `Unlocked(set)` |
| ------------------------------------- | ------------------------------ | --------------------------------- |
| `enabled: true`                       | yes                          | yes — always                    |
| `enabled: false`, some unlocking tech | yes                          | iff in `set`                    |
| `enabled: false`, no unlocking tech   | **no**                       | iff in `set`                    |

Two rows need justifying.

**`enabled: true` is available under `Unlocked` without being in the set.** A
recipe available at game start is never taken away, so no source should have to
enumerate 323 of them. A save import's set is a superset of them anyway (that
superset relation is exactly the invariant #3382 self-calibrates on), so
OR-ing changes nothing for that source while making a hand-built set possible.

**The never-unlockable exclusion applies only to `Everything`.** Eight recipes
are `enabled: false` with no technology unlocking them — `loader`,
`fast-loader`, `express-loader`, `turbo-loader`, `infinity-chest`,
`infinity-pipe`, `heat-interface`, `pistol`. No playthrough can reach them, so
with no source to consult they are excluded; that is a live bug fix independent
of this feature, since they are candidates today. But under `Unlocked` the
source is authoritative — if a save says a recipe is unlocked, the app does not
overrule it with a static table.

**Machine availability is derived, never a second list**: a machine prototype
named `N` is available when no recipe produces item `N` at all, or when at
least one recipe producing `N` is available. The first clause keeps editor-only
prototypes with no recipe from being wrongly excluded — missing data is not
evidence of a lock. The scan walks the **raw registry**, not `candidates_for`'s
filtered view: the hidden/recycling/main-product filters answer "which recipe
should make this item inside a chain", a different question from "can this
machine be obtained at all".

(The same conclusion was reached independently by the #3382 investigation —
"machine availability *is* recipe availability" — which is some evidence it is
the right primitive.)

### 2. `dump-ingest` — technology ingestion

A new `technologies.rs` beside `recipes.rs`, following its conventions exactly:
strict and loud (a missing required field, or one present with the wrong type,
aborts naming the technology), `defaulted()` for optional fields, `load_locale`
for display names — `technology-locale.json` exists in the dump and carries the
full name table under its `names` key.

A new `--out-technologies` arg defaulting to
`crates/solver/data/technologies.json`; `run()` returns a third count.

Ingest **all 275** technologies, not just the 163 that unlock recipes:
prerequisite chains run through bonus-only techs, so dropping them would break
any later projection walk. Per technology: `name`, `display_name`,
`prerequisites: Vec<String>`, `unlocks: Vec<String>` (from `effects[]` entries
whose `type` is `unlock-recipe`; every other effect type ignored).

A prerequisite naming an absent technology is a hard error — a broken graph
would under-report silently. An `unlock-recipe` effect naming a recipe absent
from the *filtered* recipe set is dropped rather than erroring: the recipe
filter already discards `parameter: true` placeholders by design, so those
edges are legitimately dangling.

**The data file must be regenerated from a real dump**, which lives only on the
desktop (`~/.factorio/script-output/data-raw-dump.json`, 27 MB). That is an
orchestrator step, not something an implementing agent can do on the VPS.

### 3. Where the gate applies

**"No recipe" and "no recipe you can build" must stay distinguishable.** This is
the load-bearing constraint, and gating `candidates_for` in place would
silently violate it. `solve.rs:89-94` treats an empty candidate set for an
*intermediate* as "raw resource, fold into the bus inputs" — correct today,
because an item nothing produces really is ore or water. Gated, a locked
intermediate two levels down would report as a bus requirement instead of an
error: the plan would tell the user to put 45/s of a Vulcanus casting product
on their bus rather than that they need `foundry`. That is the same failure
mode #3382 documents as its own worst trap — an alignment that passes a weak
check and rationalises its residual — and it gets worse with a real import,
which produces far more locked intermediates than a hand-built set ever would.

So selection grows a second function rather than a parameter on the old one:

- **`candidates_for(item)`** — unchanged, ungated. "Does anything produce this?"
- **`available_candidates_for(item, &availability)`** — the gated subset,
  applied after the existing hidden/recycling/main-product filters.

The gap between them is the error case, and each call site is explicit about
which question it asks:

1. **`solve`'s raw-resource test** (`solve.rs:91`) keeps calling the ungated
   one, so a genuinely raw item still folds into the bus exactly as today.
   Ungated non-empty but gated empty is `ChainError::NotUnlocked` — craftable,
   just not by this player.
2. **`solve`'s goal check** (`solve.rs:48`) takes the same two steps:
   ungated-empty stays `UnreachableBoundary`, ungated-non-empty-but-locked
   becomes `NotUnlocked`.
3. **`select_recipe`** resolves against the gated set — this is what makes the
   four ambiguity errors disappear.
4. **`select_machine`** skips unavailable prototypes in the fastest-available
   search. If that search finds nothing available where the ungated search
   would have found something, the error names the lock rather than falling
   through to `NoMachineForCategory`, whose message talks about the machine
   policy and would send the user to the wrong control. A *named* machine that
   is locked is likewise an error, not a silent substitution.

`ChainGoal` gains `availability: Availability`, defaulting to `Everything`,
with a `with_availability()` builder matching `with_machines()`.

### 4. UI — subtractive editing

The set is edited, not built. Switching the mode radio from **Everything** to
**Only what I can build** seeds the set with every recipe currently available,
so the switch changes nothing on its own and the first edit is a removal. That
matters because the user's actual complaint is subtractive — "stop offering me
casting recipes" — and building a 300-entry set from nothing to express it
would be absurd.

A collapsing "Available recipes" section below the machine selector:

- the mode radio;
- a search box, matching the product picker's behaviour (case-insensitive,
  display name or internal name);
- **Tick all / untick all matching the search** — so "casting" → untick is one
  action, which is the whole reported problem solved in two clicks;
- a scrolling checkbox list of the filtered recipes;
- a count — "412 of 649 available" — so the state is legible without scrolling.

A later save import replaces the set wholesale; manual edits then become
corrections on top of it, which is the same UI doing less work.

### 5. Persistence

eframe's `persistence` feature is **not** in its defaults, so
`crates/ui/Cargo.toml` gains `features = ["persistence"]` (pulling in `ron` and
`serde`). `FactorioApp` implements `eframe::App::save` and restores in
`FactorioApp::new` from the `CreationContext`'s storage.

Persisted: the availability mode and set; the chain panel's inputs — product,
rate value/unit, belt tier, bus list, machine choice, recipe overrides,
`show_hidden_recycling`; the layout config — belt tier, pole, inserter, long
inserter, topology.

Not persisted: `result`, `generated`, `pending_grid`. They are derived from a
solve, and restoring them would show a plan that no longer matches the inputs
beside it. The panel opens with its inputs filled and no result, one click from
where it left off.

`CellTopology` and `MachineChoice` need `Serialize`/`Deserialize`. Restore is
**lenient**: a field that fails to deserialise falls back to its default rather
than discarding the whole blob, so adding a field later cannot wipe a saved
setup. A recipe name in the restored set that no longer exists in the registry
is dropped on load — a game update can retire recipes.

## Data flow

```
untick "casting-iron" (or import a save, later)
  → ChainPanel.availability updated
  → build_goal() → ChainGoal { availability: Unlocked(set), … }
  → chain::solve
      → candidates_for(item)                     ungated: is it raw?
      → available_candidates_for(item, &avail)   gated: can he build it?
          both non-empty  → select → 1 candidate → no ambiguity
          ungated non-empty, gated empty → NotUnlocked (never a bus input)
          both empty      → raw resource → bus input, as today
  → select_machine → electromagnetic-plant skipped when locked
  → ProductionPlan
```

## Error handling

- **Ingest** — strict and loud, per the existing rule: a missing field, an
  unparseable prerequisite, or a dangling prerequisite aborts naming the
  technology. A silent partial write from a rarely-run tool poisons every
  downstream phase.
- **`ChainError::NotUnlocked { item, unlocked_by: Vec<String> }`** — names the
  item and, from the technology graph, every technology that would unlock a
  recipe for it, sorted. This is what the tech ingestion is for. Message points
  at both remedies: research it, or tick it in the panel.
- **`ChainError::MachineNotUnlocked { machine, unlocked_by }`** — a named
  machine that is locked, and equally a fastest-available search that found
  only locked machines, listing them.
- **Empty set under `Unlocked`** — not special-cased. Every `enabled: false`
  recipe is unavailable and the goal fails with `NotUnlocked` naming what to
  tick. Legible without a bespoke branch.

## Testing

**dump-ingest** — a fixture dump covering: an `unlock-recipe` effect, a
non-unlock effect ignored, a bonus-only technology kept, a dangling
prerequisite rejected, an `unlock-recipe` naming a filtered-out recipe dropped.

**availability** — one test per row of the table, against the real registry:
`iron-plate` available under an empty `Unlocked` set (`enabled: true`);
`casting-iron` unavailable without it and available with it; `loader`
unavailable under `Everything` but available under an `Unlocked` set naming it,
proving the source outranks the static table.

**Locked is not raw** — the case the two-function split exists for and the one
most likely to regress silently. Demand a locked item as an *intermediate*, not
as the goal, and assert the solve fails with `NotUnlocked` naming it. A plan
that instead lists it under `inputs` is the bug, and it would otherwise look
like a perfectly ordinary successful plan.

**select** — the headline case: with a set excluding the casting recipes,
`available_candidates_for("iron-plate")` returns exactly one recipe and solving
the chemical-science goal raises no `AmbiguousRecipe` at all. That plan
currently needs three hand-entered overrides (`crates/ui/src/chain_panel/
scroll_tests.rs`), and this is what removes them. Machine gating: with
`electromagnetic-plant`'s recipe locked, the same plan assigns no
electromagnetic plants.

**Existing behaviour** — `research_locked_recipes_are_still_candidates` in
`select.rs` encodes that `enabled` is not a selection filter, and its assertion
must still hold under `Everything`: the gate is opt-in, not a redefinition of
the default. Its *source* does change — it and the other `candidates_for`
callers in `select.rs`'s and `solve.rs`'s test modules take the new function or
an explicit `&Availability::Everything`. "Unchanged" means the assertion, not
the call site.

**UI** — through the existing headless harness: the section paints, the mode
switch seeds the set (so switching alone changes no solve result), untick-all-
matching removes exactly the searched recipes, and the count line tracks the
set. Persistence is tested at the serde layer — round-trip the state struct,
and restore a truncated or renamed-field blob to defaults — rather than through
eframe's storage, which needs a real app lifecycle.

## Out of scope

- **The save-file import itself** (#3382) — another session's work. The contract
  between them is exactly one thing: a set of unlocked recipe names. Note that
  it makes `recipes.json` matching the player's game version a *hard*
  dependency where today it is soft.
- Projecting a technology set onto the recipe set — the graph is ingested for
  it, but no UI builds one until manual editing proves too tedious in practice.
- Quality tiers, modules, beacons (#3351).
- Per-planet or per-surface gating; one flat set covers the stated need.
- Gating on inventory or built entities: this is about what can be built, not
  what has been.
