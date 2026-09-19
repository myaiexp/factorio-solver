# Phase 8 — Save-Derived Machine Availability

**Status**: Complete (code), unverified against a real save
**Design**: `.claude/plans/2026-08-13-save-derived-availability-design.md`
**Plan**: `.claude/plans/2026-08-13-save-derived-availability-plan.md`
**Closes**: idea #3356. Supersedes #3350. Evidence: #3382, #3384.

## The problem

The solver proposed recipes and machines the player cannot build. Measured
against Mase's current save (`2026.zip`, 2.0.77, 369/659 recipes unlocked):
`assembling-machine-3`, `electric-furnace`, `chemical-plant`, `oil-refinery`,
`beacon` and `advanced-circuit` are all locked, yet the generator would happily
emit a green-circuit block built from assembling-machine-3.

Idea #3350 had recorded that live game state was unreachable mod-free and
closed the question. That premise was measured on 2026-08-13 and refuted: the
recipe-unlock state is readable directly from the save file.

## What shipped

- **`crates/save` (`factorio-save`)** — opens a save zip, parses
  `level-init.dat` (version, mod names, prototype id tables), inflates the
  `level.dat<N>` chunks lazily in numeric order, and locates the player force's
  recipe-unlock array by calibration search.
- **`chain::Availability`** — `Unrestricted` (the default) or
  `Unlocked(HashSet<String>)`, carried on `ChainGoal` and applied in
  `select_recipe` / `select_machine`. New errors `RecipeLocked` and
  `MachineLocked`.
- **`solver::availability`** — `default_enabled()` supplies the calibration
  invariant from the committed registry; `from_save(path)` is the bridge.
- **UI save picker** — a dropdown of `~/.factorio/saves/*.zip` newest first,
  a manual path field, Rescan/Clear, and a status line.

553 workspace tests pass.

## What is not verified

**No Factorio install exists on the build machine**, so nothing here has ever
run against a real save. The synthetic fixtures validate the decoder against a
layout this repo also authors, which cannot catch a wrong guess about the
`level-init.dat` *header* (the version field and mod-list encoding) — that part
was never measured, only the category-table format was, and both parse sites
say so.

`crates/save/tests/real_save.rs` is the check that closes this, gated on an
env var so it skips where no save exists:

```
FACTORIO_SAVE_FIXTURE=~/.factorio/saves/2026.zip cargo test -p factorio-save
```

Expected against `2026.zip` (2.0.77, 369/659): `assembling-machine-2` unlocked,
`assembling-machine-3` and `oil-refinery` locked. Run it on the desktop before
trusting the feature.

## Decisions

Condensed into `CLAUDE.md`'s "Decisions from previous phases". The two that
carry the most weight:

- **The calibration invariant must not be weakened.** "Every default-enabled
  recipe is enabled in the save" is the only check that discriminates; a weaker
  "the common starting recipes are present" is satisfied by an alignment off by
  exactly one record, and the investigation's first decode failed in precisely
  that way and rationalised its 38-recipe residual as correct gating.
- **Availability filters in `select_recipe`, never in `candidates_for`.**
  `solve` uses `candidates_for(&item).is_empty()` as its raw-resource test, so
  filtering there would make a locked intermediate look like iron ore and get
  quietly billed to the bus.

## Deferred

- **Technologies are not read** (#3382): three structural hypotheses tested and
  rejected. Not needed — machine availability derives from recipe unlocks.
- **The product/recipe picker is not availability-aware** (#3385): it runs its
  own hidden/recycling filter and does not go through `chain::select`. A locked
  pick dead-ends at Solve with `RecipeLocked`, which names the item and the
  remedy — acceptable because the error is explicit, not because the picker is
  right.
- **No file watching, no auto-refresh, no support for a save whose version does
  not match the committed dump** (#3384) — that last one fails loudly by design.


---

## Verified against a real save — 2026-08-13

This phase shipped documented as "complete in code, never run against a real
save", because the machine that wrote it had no Factorio install. Run against
one for the first time while finishing phase 9, it did not work at all — three
separate faults stacked behind a single misleading error (idea #3399):

1. **Every entry is nested.** Factorio writes a save's contents under a folder
   named after the save, so the required entry is `<save>/level-init.dat`. The
   reader resolved names at the archive root. Chunk discovery had the same bug
   and failed *silently*: it matched `level.dat<N>` against the full path, found
   nothing, and an empty chunk list is not an error anywhere — it just reads as
   a save with no data.
2. **`zip` was built without `deflate`.** `default-features = false` drops it.
   Factorio stores the `level.dat<N>` chunks uncompressed (their contents are
   zlib'd separately, which is what `inflate_chunk` undoes) but *deflates*
   `level-init.dat`, so the one required entry could never be read.
3. **Every `ZipError` became `MissingEntry`.** `read_entry` and `inflate_chunk`
   both did `.map_err(|_| MissingEntry)`, so (2) reported as "save is missing
   required entry 'level-init.dat'" — a message that names the opposite of what
   was wrong, and which sent the first diagnosis to (1) and no further.

The fixtures were what let all three ship: `FixtureSave` wrote entries flat and
`Stored`, a shape no real save has, so 37 passing tests said nothing about the
format. They now nest and deflate exactly as Factorio does, and the reader
derives the save directory from the archive rather than assuming a layout.

**What the real save proves.** Against `2026.zip` (2.0.77, the same version as
the committed dump): version parses as 2.0.77, the recipe id table holds 659
entries — precisely the dump's raw count, 649 kept plus the 10 `parameter`
placeholders — the technology table holds 275, and the calibration search
decodes 369 unlocked recipes. Those are the exact numbers `real_save.rs` was
already asserting as ground truth, reached independently. The inferred header
layout and the calibration search are therefore both confirmed against real
data, which is what this phase could not claim before.

A run against an older save (2.0.28) fails calibration and says so, naming the
version mismatch and the remedy — correct designed behaviour, not a fault.

**Still wrong:** `mods()` parses empty. A variable-length scenario section sits
between the version header and the mod list, and `parse_mods` reads a count
immediately after the header. Idea #3400 carries the decoded byte layout. The
blast radius is exactly what `init.rs` predicted: the id tables are located by
search, so only `mods()` is affected, and that answers nothing but "is this
vanilla".
