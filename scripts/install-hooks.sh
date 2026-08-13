#!/usr/bin/env bash
# Point git at the tracked hooks in scripts/hooks/. Idempotent; run once per clone.
#
# `core.hooksPath` rather than copying files into .git/hooks: the hooks are then
# version-controlled, reviewable, and updated by a pull instead of by remembering
# to re-run an installer. Git resolves a *relative* hooksPath against each working
# tree's own root, and the config lives in the shared .git — so this one command
# covers the main checkout and every present and future worktree of it, each
# running the copy on its own branch. A branch that predates the hooks simply has
# no file there, and git treats a missing hook as a no-op.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

chmod +x scripts/hooks/pre-commit scripts/hooks/pre-push scripts/hooks/check-workspace.sh
git config core.hooksPath scripts/hooks

echo "hooks installed: core.hooksPath = $(git config core.hooksPath)"
echo "  pre-commit — untracked/unstaged check, then clippy + tests"
echo "  pre-push   — clippy + tests on the tree being published"
echo "Bypass either with --no-verify."
