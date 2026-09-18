# Save Reading and Recipe Availability

What the save reader has been verified against, and the lasting decisions behind the availability gate.

> The sections below were moved verbatim out of `CLAUDE.md` on 2026-09-18 (idea #4770), which now keeps only a pointer to each. This doc is the record: add new detail here, not to `CLAUDE.md`.

## Verified against a real save

> **Verified against a real save 2026-08-13.** The reader opens the player's
> current save (2.0.77) and decodes 659 recipes, 275 technologies and 369
> unlocked recipes — matching the ground truth in
> `crates/save/tests/real_save.rs` exactly. Getting there took three fixes
> (idea #3399): entries are nested under a folder named after the save, the
> `zip` crate needs its non-default `deflate` feature because Factorio
> compresses `level-init.dat`, and every `ZipError` was being reported as
> `MissingEntry`, which hid the second behind the first. Run it yourself with
> `FACTORIO_SAVE_FIXTURE=<a save> cargo test -p factorio-save`.
>
> One piece is still wrong: `SaveFile::mods()` parses empty, because a
> variable-length scenario section sits between the version header and the mod
> list (idea #3400, which carries the decoded layout). Contained by design —
> `init.rs` locates the id tables by search, so a mod-list mismatch corrupts
> only `mods()`, and that is used solely to answer "is this vanilla".

## Decisions

- **The unlock array's stride and offset are never hardcoded, and the calibration invariant must not be weakened.** Stride was measured at 7 on 2.0.8/2.0.28/2.0.32 and 6 on 2.0.60/2.0.77; the offset varies per save *within* one version (+67, +181, +134). The layout is found by searching for the `(stride, offset)` where **every** default-enabled recipe decodes as enabled — which yields exactly one hit on every 2.0 save measured. A weaker check ("the common starting recipes are present") is satisfied by an alignment off by exactly one record, and the investigation's first decode failed in precisely that way and rationalised its 38-recipe residual as correct gating. Zero candidates is `CalibrationFailed`, more than one is `CalibrationAmbiguous`; there is no best-guess fallback
- **`default_enabled` is a parameter of `save`, never something it reads**: the set lives in `solver`'s registry, so reading it inside `save` would invert the crate graph. This is the whole reason `save` has no workspace dependency
- **Availability filters inside `select_recipe`, never inside `candidates_for`.** `solve` uses `candidates_for(&item).is_empty()` as its *raw-resource* test, so filtering there would make an intermediate whose only recipes are locked look like iron ore and get silently billed to `plan.inputs` — a silent wrong answer instead of a loud one. The asymmetry that hides it: the goal item has its own emptiness check, so only intermediates would be corrupted. Filtering after the count is also a gain — a locked alternative no longer makes an item look ambiguous, so availability sometimes *resolves* an `AmbiguousRecipe`
- **A locked *named* machine is an error (`MachineLocked`), a locked *fallback* candidate is skipped**: the fastest-available search silently picking assembling-machine-2 over a locked 3 is the point of the feature, while a machine the user named is a claim worth surfacing — the same rule as a named machine that cannot craft the category. `MachineLocked` is deliberately distinct from `NoMachineForCategory`, whose remedy ("pick a machine for that category") is wrong advice when the category is fine
- **An explicit recipe override wins even over a locked recipe** — an override is a deliberate user statement, and the override path bypasses candidate selection entirely
- **`level-init.dat`'s category-table format is measured; its header is not.** `[u8 len][category][u16 count]` then `count` × `[u8 len][name][u16 id]` parsed on ~50 real saves from 1.0.0 to 2.0.77. The version field and mod-list encoding in front of it were never confirmed, which is why the recipe/technology tables are located by **searching** for their length-prefixed category name rather than by an offset the header parse computed — a wrong header assumption can then only corrupt `mods()`, never the id tables
- **Chunks are ordered by numeric suffix, never lexicographically** (`level.dat10` sorts before `level.dat2` as a string), and inflated lazily — the force's array completes inside the first chunk on every save measured (1 of 62 through 1 of 132), so inflating the whole stream would do 60–130× the needed work. That deliberately makes the total byte count unknowable, which is why the `level.datmetadata` size check is an explicit `verify_total_size` rather than a load precondition
- **Availability is a set of recipe *names*, never of technologies**: a save exposes per-recipe unlocked flags while researched technologies are not readable, so a hand edit, a save import and a future projection all produce the same thing. That single decision is what let two independently-built gates (phases 8 and 9) merge instead of one replacing the other
- **"No recipe" and "no recipe you can build" must stay distinguishable**: `solve`'s raw-resource test keeps calling the *ungated* `candidates_for`. Gated in place, a locked intermediate two levels down reports as a bus requirement — a plan telling the player to belt 45/s of a Vulcanus casting product, looking entirely successful. `available_candidates_for` is a second function for exactly this reason, and each call site is explicit about which question it asks
- **`Availability::allows` ORs with `Recipe::enabled`**: without it, unticking one recipe silently locks the 323 the game grants at the start. Free for the save source, whose own calibration invariant asserts a save marks every default-enabled recipe on — pinned by `from_save`'s `an_imported_set_contains_every_default_enabled_recipe`
- **The technology graph explains, it never gates**: `tech::unlockers_for` turns "you cannot build this" into "this needs `foundry`". Nothing selects on it, which is why 112 bonus-only technologies are ingested anyway (prerequisite chains run through them) and why a dangling unlock edge is dropped while a dangling *prerequisite* is a hard error
- **`Everything` excludes what no playthrough can reach**, derived from the graph rather than a list of the eight names — but this changes no chain result, since all eight are also `hidden: true` and `candidates_for` had always dropped them. What it protects is the UI's seeded tick-list. (The design doc claims otherwise; it is wrong)
- **An override outranks availability, in `solve` as well as in `select_recipe`**: an override already bypasses the hidden/recycling/main-product filters, so naming a locked recipe means the user meant it. `solve` defers through `locked_without_an_override` rather than refusing first — the two disagreed once, and `solve` won the race
- **Editing the set is subtractive**: entering "only what I can build" seeds it from what is already available, so the switch alone changes no result and the first edit is a removal. A save import replaces the set wholesale and hand edits then correct it — one set, three sources, never two constraints that can disagree
