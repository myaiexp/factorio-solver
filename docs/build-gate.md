# The Build Gate

Which git hook verifies which path onto `master`, and why the gate exists.

> The sections below were moved verbatim out of `CLAUDE.md` on 2026-09-18 (idea #4770), which now keeps only a pointer to each. This doc is the record: add new detail here, not to `CLAUDE.md`.

## Which hook covers which land path

Which hook covers which of `deploy`'s land paths was **measured, not assumed**:
a fast-forward fires nothing (the tree is the branch tip's, already gated), a
`--no-ff` merge fires `pre-merge-commit`, and a **cherry-pick fires no hook at
all** — so a single commit replayed onto a moved base is the one ungated path,
and `pre-push` warns when it sees a tree nothing verified. `pre-push` does no
building on purpose: `deploy` allows a push 15 seconds, an earlier version ran
the full gate there, and it killed a real deploy — the branch landed on master
and the push never happened.

## `--locked` and the tracked `Cargo.lock`

Both cargo invocations pass `--locked`, and **`Cargo.lock` is tracked** (the
scaffold `.gitignore` ignored it, which is the library default; this workspace
ships a binary). The two go together: cargo rewrites the lockfile during
*resolution*, before it compiles and long after pre-commit's unstaged check has
passed — so staging a manifest change alone used to land the old lockfile
beside a build done against the new one, a commit describing a dependency graph
nothing verified. `--locked` makes cargo refuse instead, and the hook prints the
remedy (`cargo metadata … && git add Cargo.lock`) since cargo's own advice is to
drop the flag.

## Why the gate exists: the 2026-07 audit

> **Note (2026-07):** A code audit found the committed HEAD referenced ~10 phantom module/data files (`spatial`, `astar`, `lod`, `recipe`, `calculator`, `control_behavior`, `wire_extraction`, `prototypes.json`, `to_blueprint`) that were documented as complete but had never been committed to any branch — the workspace did not compile. The engine pieces the tests actually exercise (spatial index, A*, LOD, prototypes registry, grid→blueprint export, `EntityCategory` declaration) were reconstructed; the unconsumed recipe/calculator/wire modules were stripped and backlogged.
