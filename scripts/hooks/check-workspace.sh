#!/usr/bin/env bash
# The build gate: clippy (deny warnings) then the full test suite, against the
# working tree. Records the tree it passed, so the same tree is never rebuilt.
#
# Shared by pre-commit and pre-merge-commit, and runnable by hand. Clippy rather
# than `cargo check` because clippy type-checks everything check does and also
# holds the clean-warning baseline (`--all-targets` reaches tests and benches,
# which a bare `cargo check` never compiles). Warm, the pair costs ~5s.
#
# Output is captured and shown only on failure: these hooks run inside `git
# commit` and `git merge`, where a passing suite's few hundred lines of "test
# result: ok" bury whatever git itself has to say.

set -uo pipefail

cd "$(git rev-parse --show-toplevel)" || exit 1
. "$(dirname "$0")/stamp.sh"

# No toolchain means the tree cannot be verified. Refusing is the point of the
# gate — a silent skip is exactly how an unbuildable HEAD shipped before it
# existed. (No Rust was even installed on the VPS when the audit found that.)
if ! command -v cargo >/dev/null 2>&1; then
  echo "gate: cargo is not on PATH — cannot verify this tree." >&2
  echo "      Install the toolchain, or bypass with --no-verify." >&2
  exit 1
fi

# The index is the tree about to be committed; both hooks that call this have
# already established that the working tree matches it (pre-commit refuses
# otherwise, and a clean merge writes both together).
tree=$(git write-tree) || exit 1

if tree_is_verified "$tree"; then
  echo "gate: ok (tree $tree already verified)"
  exit 0
fi

run() {
  local label="$1"
  shift
  echo "gate: $label"
  local output
  if ! output=$("$@" 2>&1); then
    printf '%s\n' "$output" >&2
    echo "gate: $label FAILED." >&2
    # Cargo's own advice for this one is "remove the --locked flag", which is
    # the opposite of what should happen here. Say what to do instead.
    if printf '%s' "$output" | grep -q -- '--locked was passed'; then
      echo >&2
      echo "gate: a manifest changed without its lockfile. Refresh and stage it:" >&2
      echo "        cargo metadata --format-version 1 >/dev/null && git add Cargo.lock" >&2
    fi
    exit 1
  fi
}

# `--locked` is not belt-and-braces, it closes a hole this gate would otherwise
# open. Cargo rewrites Cargo.lock during *resolution* — before it compiles
# anything, and long after the unstaged check above has passed. Measured: stage
# a Cargo.toml that adds a dependency, commit, and the tree that lands carries
# the OLD lockfile while everything clippy and the tests just proved was built
# against the new one. The commit then describes a dependency graph that was
# never verified, which is the exact failure this gate exists to prevent, dressed
# up as a pass. With --locked cargo refuses to rewrite and says so instead.
run "cargo clippy --workspace --all-targets -D warnings" \
  cargo clippy --locked --workspace --all-targets --quiet -- -D warnings
run "cargo test --workspace" cargo test --locked --workspace --quiet

record_verified_tree "$tree"
echo "gate: ok"
