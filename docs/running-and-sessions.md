# Running It, and Where Sessions Run

How the app is launched, and why and how sessions run on the desktop instead of the VPS.

> The sections below were moved verbatim out of `CLAUDE.md` on 2026-09-18 (idea #4770), which now keeps only a pointer to each. This doc is the record: add new detail here, not to `CLAUDE.md`.

## Running it

`cargo run -p factorio-ui` from the workspace root, or `scripts/launch.sh` — the
same thing, but it fast-forwards a clean `master` to `origin/master` and rebuilds
in release only when the binary is stale, so a desktop entry always starts the
newest code. `scripts/install-desktop-entry.sh` writes that entry (paths derived
from the checkout, so it is re-runnable and machine-independent). The window's
`app_id`, the entry's basename, and its `StartupWMClass` are all
`factorio-solver` and must stay equal — that triple is what pairs the window
with the launcher icon on Wayland.

## Where sessions run

**On the desktop, not the VPS** (`projects.execution='remote-native'`, `machine=desktop`,
`remote_path=/home/mse/Projects/factorio-solver`). Helm SSH-spawns `claude -p` there in a
worktree under `.worktrees/<id>`. The reason is not disk: the deliverable is an egui window
that reads icons live from a Factorio install and is verified against real saves, so on the
VPS every UI change was unverifiable in principle. Full rationale, including the tooling the
desktop needed and what it still lacks (`rtk`): `.claude/plans/2026-08-13-remote-native-desktop-design.md`.

## Fallback when the desktop is down

**Fallback when the desktop is down** (it reaches the VPS over a reverse SSH tunnel — no
tunnel, no sessions): the VPS clone at `~/Projects/solvers/factorio-solver` is kept for this.
`UPDATE projects SET execution='local' WHERE name='factorio-solver';` switches to it —
`path` and `remote_path` are separate columns — and `'remote-native'` switches back. Fetch
first; neither checkout pulls automatically. A VPS session can do all crate logic and the
full suite (`real_save.rs` early-returns without `FACTORIO_SAVE_FIXTURE`), but cannot run the
UI, read icons, or test a save — and it builds its own 4.6 GB target, since `worktreeBridge`
is remote-only.
