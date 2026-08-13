# Move factorio-solver sessions to the desktop

**Date**: 2026-08-13
**Status**: approved, implemented

## Why

Two independent reasons, either of which would be sufficient.

**The app cannot run where it is being written.** `factorio-ui` is an egui window.
It reads entity icons live out of a Factorio install (deliberately — that keeps
Wube's art out of a 0BSD repo and matches the player's own version and DLC), and
`crates/save` is verified against real save files. The VPS has no display, no
Factorio, and no saves. Every UI change made there was unverifiable *in principle*,
not merely inconvenient: the only honest verification available was `cargo test`.
The desktop has the game, the saves, 16 cores and 31 GB.

**Disk.** A full Rust target dir is 4.6 GB, each session worktree carried its own,
and nothing reclaimed it until the session was archived. This took the VPS to a
critical-disk health alert on three separate days (2026-08-11 twice, 2026-08-13),
each time needing a manual `cargo clean` across idle worktrees to recover 4–8 GB.
The VPS has 11 GB free of 80; the desktop has 62 GB free of 930. Idea #3381.

## What changed

Helm already had the mechanism — `projects.execution = 'remote-native'` SSH-spawns
`claude -p` on a remote machine inside a per-session worktree there, and has run
`skyrim-headless-mods` on this same desktop since June. Nothing was built for this;
it was configuration plus the tooling gaps that configuration exposes.

```sql
UPDATE projects SET machine='desktop',
                    remote_path='/home/mse/Projects/factorio-solver',
                    execution='remote-native'
WHERE name='factorio-solver';
```

There is no API for this. `POST /api/projects/remote` is the only route that writes
those three columns and it refuses a name that already exists; `PATCH
/api/projects/:id` edits `tier` alone. A direct `UPDATE` is the only path, which is
also how the switch gets *reversed* — see the fallback below. Filed as a Helm idea.

Session context generation is already remote-aware (`src/context/remote.ts` runs the
generator over SSH against `remote_path`), so nothing else needed changing.

## Build-cache sharing, and why debug only

`.helmcontext` declares `worktreeBridge: [target/debug]`. Helm symlinks those paths
from the main checkout into each fresh worktree, so N sessions share one build cache
instead of N × 4.6 GB. Without it this change would simply move the disk alert from
the VPS to the desktop.

Bridging is **debug only**, deliberately. `scripts/launch.sh` runs
`target/release/factorio-ui` — the binary behind the desktop launcher icon — and
decides staleness from that binary plus a `.launch-stamp` holding the sha it was
built from. Share `release` and a session's `cargo build --release` overwrites that
binary in place; the launcher then either rebuilds needlessly or, when master's HEAD
happens to match the stale stamp, silently launches a session's half-finished code
as though it were master. Nothing downstream could detect that. Debug is where the
sharing is worth having anyway: every command a session actually runs (`cargo test`,
`cargo clippy`, `cargo run`) is debug, worth 2.1 GB, against a `--release` build
that is rare and reclaimed when the session is archived.

Helm's bridge is dir-aware — a directory target is materialized as a real directory
whose children are symlinked, never a symlink standing in for the directory, because
a trailing-slash gitignore pattern matches a directory but not a symlink replacing
it and the worktree would read as dirty. The consequence to know: only the children
that exist *at worktree-creation time* are shared. `target/debug`'s layout (`deps`,
`build`, `incremental`, `.fingerprint`, `examples`) is created by the first build
and stable after, so in practice this is total; a brand-new main checkout should be
built once before the first session.

## Git: plain commits and push, no `deploy`

`deploy` is a Helm script (`~/Projects/helm/scripts/deploy`) and does not exist on
the desktop. This project follows the precedent `skyrim-headless-mods` already sets
for remote-native repos: **commit on the session branch, merge to master in the main
checkout, `git push origin`** — plain git, no landing-shape logic, no service
restart (there is no service), no mase.fi update logging.

The safety consequence has to be stated because it is not obvious: **Helm's
archive pre-flight fails open for remote projects.** The guard that refuses to
archive a session holding unlanded commits — the net the global git rules lean on
everywhere else — does not run here. Archiving a remote-native session with an
unmerged branch destroys that branch and its commits with no refusal. Landing is
therefore the session's own responsibility, every time, with nothing behind it.
Filed as a Helm idea.

## Tooling on the desktop

The desktop already had `build-lock` (a no-op passthrough there) and
`mase-fi-update`. Two gaps were filled, both in `~/.local/bin`:

- **`helm`** — a shim forwarding to the VPS over SSH. The base modules instruct
  every session to run `helm idea add`, `helm session needs-input`, `helm roadmap
  show`; none of them resolved on the desktop, and module composition has no
  remote-awareness that could have trimmed those instructions. Fixes
  `skyrim-headless-mods` at the same time.
- **`trash`** — `hooks/rm-guard.py` *is* shipped to remote hosts and wired as a
  PreToolUse Bash guard, so `rm` was blocked while the `trash` it redirects to did
  not exist. That is a deadlock, not an inconvenience.

`rtk` stays absent and is not shimmable — it is a VPS-local binary, and its rewrite
hook is deliberately excluded from the remote guard set. Remote sessions pay full
token cost for shell output. That is the real, recurring price of this move.

`scripts/install-hooks.sh` was run in the desktop checkout: its `core.hooksPath` was
still the global `~/.config/git/hooks`, so the clippy + test commit gate had never
fired there.

## Fallback: the VPS when the desktop is down

The desktop reaches the VPS through a reverse SSH tunnel (`Host desktop` →
`localhost:2222`), maintained from the desktop side. Desktop off or tunnel down
means no factorio-solver session can launch at all. The VPS clone at
`~/Projects/solvers/factorio-solver` is therefore **kept, as a deliberate fallback**
rather than removed — `projects.path` and `projects.remote_path` are separate
columns, so flipping `execution` back to `'local'` switches which one is used and
nothing else has to change.

```sql
UPDATE projects SET execution='local'  WHERE name='factorio-solver';  -- to VPS
UPDATE projects SET execution='remote-native' WHERE name='factorio-solver';  -- back
```

Before using it: `git -C ~/Projects/solvers/factorio-solver fetch origin && git
merge --ff-only origin/master`. Both checkouts push to the same GitHub `origin`,
but neither pulls automatically, and desktop work is only visible once landed and
pushed.

What a VPS session **can** do: everything in `blueprint`, `grid`, `templates`,
`save`, `solver` — including the full test suite. `crates/save/tests/real_save.rs`
early-returns without `FACTORIO_SAVE_FIXTURE`, and the workspace builds and runs
with no Factorio install present.

What it **cannot** do: run the UI, verify anything visual, read icons, or test
against a real save.

What it costs: `worktreeBridge` is remote-only — the sole call site is
`createRemoteWorktree` — so a VPS worktree builds its own 4.6 GB target with 11 GB
free. Survivable for one occasional session, which is the entire fallback role;
archiving the session reclaims it. Two concurrent VPS sessions is the disk alert
again.

## Rejected

**Fixing only the disk problem** (shared target dir, VPS sessions unchanged) — the
cheap option, and it addresses the alert while leaving the more serious problem
untouched: the app still could not be run or seen by whoever was writing it.

**Porting `deploy` to the desktop.** Considered specifically because archive
pre-flight fails open, which makes a dirty-tree-refusing land script more valuable
here than where it already exists. Rejected for now as disproportionate to a
one-project, no-service repo; the skyrim precedent is the house style for remote
repos and consistency across the two is worth more than the guard.

**A machine-global `CARGO_TARGET_DIR` on the desktop** instead of `worktreeBridge`
— total sharing with no per-worktree holes, but it is config living outside the
repo where no future session would find it, it breaks `launch.sh`'s binary path,
and it silently orphans the existing 4.6 GB target.
