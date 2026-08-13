# Save-Derived Machine Availability — Design

**Date**: 2026-08-13
**Status**: Approved, not yet implemented
**Closes**: idea #3356 (tech-level gating). Supersedes idea #3350.
**Evidence**: idea #3382 (byte-level decode), #3384 (version-matching limit)

## Problem

The solver proposes recipes and machines the player cannot build. Measured
against Mase's current save (`2026.zip`, 2.0.77, 369/659 recipes unlocked):
`assembling-machine-3`, `electric-furnace`, `chemical-plant`, `oil-refinery`,
`beacon` and `advanced-circuit` are all locked, yet the generator will happily
emit a green-circuit block built from assembling-machine-3.

Idea #3350 recorded that live game state was unreachable mod-free and closed the
question. That premise was measured on 2026-08-13 and refuted: the recipe-unlock
state is readable directly from the save file.

## What was measured

Full byte-level findings are in idea #3382. The three facts this design rests on:

1. **The container is self-verifying.** The save zip holds `level.datN` chunks,
   each independently zlib-compressed and stored uncompressed in the zip.
   Concatenated and inflated they total exactly the `u64` in `level.datmetadata`
   — 79 chunks and 81,832,332 bytes on the achievement save, an exact match.

2. **`level-init.dat` is uncompressed and portable.** It holds the version, the
   mod list, and the prototype ID tables as `[u8 len][name][u16 id]` runs per
   category. It parsed on every save tested from 1.0.0 through 2.0.77, including
   a Pyanodon save with 10,897 recipes.

3. **Recipe unlocks are a flag array on the player force.** Searching for the
   literal bytes `01 06 "player"` locates the force; a fixed-stride record array
   follows whose first byte per record is the unlocked flag, indexed by the
   `level-init` recipe id, with length `recipe count + 1` (index 0 is the
   `recipe-unknown` sentinel).

The stride and offset are **not stable** and must never be hardcoded: stride is
7 bytes at 2.0.8/2.0.28/2.0.32 and 6 at 2.0.60/2.0.77, and the offset varies per
save even within one version (measured `base+67`, `base+181`, `base+134`).

## The calibration invariant

The array is located by search, not by constant. For each candidate
`(stride, offset)`, decode the array and test:

> every recipe with `enabled: true` in `recipes.json` is enabled in the save

On every 2.0 save tested this yields **exactly one** candidate. This is what
makes the approach survive patches: a layout shift produces zero candidates and
fails loudly rather than silently misreading.

**This invariant is load-bearing and must not be weakened.** A weaker check —
"the common starting recipes are present" — is satisfied by an alignment that is
off by exactly one record. The first decode during investigation was wrong in
exactly this way and rationalised its 38-recipe residual as correct gating. Only
the zero-violation form discriminates.

## Architecture

### New crate: `crates/save/` (`factorio-save`)

Pure Factorio save decoding. Depends on `zip` and `flate2` only — **no other
workspace crate**. This is deliberate: the calibration invariant needs the
default-enabled recipe set, which lives in `solver`, so if the crate read that
set itself the dependency would invert. Instead the set is a parameter.

Responsibilities:

- Open the save zip; read `level-init.dat` (version, mod list, prototype ID
  tables) and `level.datmetadata`.
- Inflate the `level.datN` chunks in numeric order, **lazily**, stopping as soon
  as the player force's recipe array has been fully read.
- Locate and decode that array by invariant search.

Public surface, in essence: open a save from a path, and ask it for the unlocked
recipe set given a default-enabled set.

**Lazy inflation is the difference between a fast load and a slow one.** The
player force sits about 130 KB into the stream, and the recipe array ends by
~196 KB even on a full-endgame save. Measured across five saves spanning 2.0.32
to 2.0.77, the array always completes inside the **first chunk** — 1 of 62, 1 of
77, 1 of 79, 1 of 132. Inflating the whole stream to read the first 200 KB would
do 60–130× more work than the job needs.

This is why the `level.datmetadata` total is **not** verified on every load:
deliberately stopping early makes the full byte count unknowable. The size check
moves to a separate, explicit verification used by tests and available for
diagnostics, rather than being a precondition of reading a save. The integrity
guarantee that remains on the hot path is stronger anyway — a unique calibration
hit against the zero-violation invariant is a far more specific claim about the
data than a byte count.

### Solver: an `Availability` constraint

A new type in `chain`, carried on `ChainGoal` beside `available` and `machines`:

- **Unrestricted** — the default. Byte-for-byte today's behaviour.
- **Unlocked(set of recipe names)** — only these may be used.

Two filters follow.

**Recipe selection filters inside `select_recipe`, never inside
`candidates_for`.** This distinction is load-bearing and the single easiest
thing to get wrong here.

`chain::solve` calls `candidates_for(&item).is_empty()` as its *raw-resource
test* (`solve.rs:91`): an item with no recipe anywhere in the registry is iron
ore, so it is pulled from the bus rather than erroring. If availability filtering
happened inside `candidates_for`, then an intermediate whose only recipes are
*locked* would also come back empty, and the solver would silently add it to
`plan.inputs` as though the player could mine it — the exact opposite of this
feature's purpose, and a silent wrong answer rather than a loud one. Note the
asymmetry that hides it: the goal item is protected by its own check at
`solve.rs:48`, so this would only ever corrupt intermediates.

So `candidates_for` stays purely structural, and `select_recipe` applies
availability to the candidate list it gets back:

| Structural candidates | Unlocked among them | Result |
| --- | --- | --- |
| none | — | unchanged: raw resource (or `UnreachableBoundary` for the goal) |
| some | none | **`RecipeLocked`** — new error |
| some | exactly one | that recipe |
| some | more than one | `AmbiguousRecipe`, listing only the unlocked ones |

The third row is a real gain: filtering before the count means a locked
alternative no longer makes an item look ambiguous, so availability will
sometimes *resolve* an `AmbiguousRecipe` that today needs a manual override.

**An explicit recipe override wins even when the recipe is locked.**
`select_recipe`'s override path bypasses candidate selection entirely, and that
stays true — an override is a deliberate user statement, and the normal UI flow
only ever offers unlocked candidates anyway.

**Machine selection** (`select_machine`) drops machines the player cannot craft.
Under the fastest-available fallback this silently picks assembling-machine-2
over 3, which is the point. Under a *named* or *preferred* machine a locked
machine is an **error**, not a downgrade — consistent with how a named machine
that cannot craft a category already errors rather than falling back.

A machine counts as craftable when some unlocked recipe produces an item whose
name equals the machine's name. Keyed on the produced item rather than the
recipe name so a machine whose recipe is named differently still resolves. This
assumes entity-prototype name and item name coincide, which holds for ordinary
buildings throughout this data set; it is an assumption, not a guarantee, and a
machine that violated it would read as locked rather than as wrongly available.

### Dependency graph

```
ui → solver → save
       └────→ templates → grid → blueprint
```

`solver` owns `recipes.json`, so it supplies the invariant set and hands the UI
a finished `Availability`. UI stays the thinnest layer, per the project's
existing value.

### UI

A save selector in the chain panel: scan `~/.factorio/saves/` for `*.zip`,
listed in a dropdown **sorted by modification time, newest first**, plus a
manual path field for saves kept elsewhere. No new dependency — deliberately not
a native file dialog, which would pull in `rfd` and a portal/GTK dependency to
point at one well-known directory.

Selecting a save decodes it and switches the goal to restricted availability.
A control returns to unrestricted. When the saves directory does not exist the
dropdown is simply absent and the manual field remains — the app must keep
building and running with no Factorio installed, which is an existing project
property.

**The product/recipe picker is deliberately left alone in this design.** It runs
its own hidden/recycling filter (`chain_panel/logic.rs::filtered_recipes`) and
does not go through `chain::select` at all, so making it availability-aware is a
separate change with its own reasoning about which filters compose — captured as
idea #3385. Until then a locked pick is not silently accepted: it dead-ends at
Solve with `RecipeLocked`, naming the item and the remedy. That is an acceptable
interim because the error is explicit, not because the picker is right.

## Error handling

Every failure is explicit and loud. Consistent with the project's existing rule
that ambiguity is always an error and never a guess:

### Decoding the save

| Condition | Behaviour |
| --- | --- |
| Zip unreadable / entry missing | Error naming the file |
| Chunk inflate fails | Error naming the chunk |
| Chunks exhausted before the force is found | Error — a save whose layout defeats the early stop |
| Player force not found | Error |
| **Zero** calibration candidates | Error stating the likely cause: the committed dump does not match the save's version (see #3384) |
| **More than one** candidate | Error. Never pick one |

The zero-candidate case is expected in normal use — it is what a save from a
different game version looks like. It must read as an explicable condition with
a stated remedy (regenerate the dump), not as a crash.

### New solver errors

`chain::error` documents that every variant names the offending item or recipe
**and** the remedy. Two variants are added to hold to that:

- **`RecipeLocked { item, recipes }`** — the item has producers, but none are
  unlocked in the loaded save. Names the locked candidates; the remedy is to
  research one, add the item to `available` so the chain stops there, or clear
  the save selection.
- **`MachineLocked { machine, recipe }`** — a named or preferred machine that
  covers the category but is not craftable. Distinct from the existing
  `NoMachineForCategory`, whose remedy ("pick a machine for that category") is
  the wrong advice here: the category is fine, the machine is simply not
  researched yet.

## Testing

Real saves cannot be committed: they are ~16MB and contain Wube's content.

- **Synthetic fixtures.** Tests construct minimal save zips in-process — a few
  fake prototypes, a known force array at a deliberately awkward offset and
  stride. These cover the container, the metadata size check, the table parse,
  the calibration search, and the failure modes above.
- **An ambiguity fixture that must be rejected.** A save crafted so two
  alignments satisfy the invariant, asserting the decoder refuses rather than
  choosing. This is the regression guard for the off-by-one trap.
- **Opt-in ground truth.** A test gated behind an environment variable naming a
  real save, skipped by default, asserting known-unlocked and known-locked
  recipes. Run against `2026.zip` (369/659, Nauvis), `2.0 fast` (518/659, in
  orbit) and `2.0 Sandbox` (730/787, full endgame) as the progression fixtures.
- **Lazy-inflation test.** Assert a load reads only the chunks it needs, so the
  optimisation cannot silently regress into a full inflate.
- **Solver tests.** A locked assembling-machine-3 falls back to 2 under the
  fastest policy and raises `MachineLocked` under a named one. A locked recipe
  raises `RecipeLocked`. An item with several producers of which one is unlocked
  resolves instead of raising `AmbiguousRecipe`. An explicit override to a locked
  recipe is still honoured. Unrestricted availability leaves every existing
  result unchanged.
- **The raw-resource regression guard.** The single most important test in this
  work: an intermediate whose only recipes are locked must raise `RecipeLocked`
  and must **not** appear in `plan.inputs`. This is what catches the failure
  described under recipe selection, where the solver would otherwise treat an
  unbuildable intermediate as a raw material and quietly bill it to the bus.

`candidates_for` gains no parameter under this design, so existing call sites and
its own test module are untouched; the ripple is confined to `select_recipe`,
`select_machine`, and their callers in `solve.rs`.

## Non-goals

- **Technologies are not read.** Three structural hypotheses were tested and
  rejected (#3382); the array needs the real C++ read order. It is not needed —
  machine availability derives from recipe unlocks.
- **No production statistics, built entities, or force bonuses.** Those live
  deeper in the format. The mod-on-a-copy route (#3383) is the escape hatch if
  they are ever wanted.
- **No file watching or auto-refresh.** Saves are loaded explicitly.
- **No support for saves whose version does not match the committed dump.**
  That fails loudly by design (#3384).
