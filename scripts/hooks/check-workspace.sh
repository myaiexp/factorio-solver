#!/usr/bin/env bash
# The build gate: clippy (deny warnings) then the full test suite.
#
# Shared by the pre-commit and pre-push hooks, and runnable by hand. Clippy
# rather than `cargo check` because clippy type-checks everything check does
# and also holds the clean-warning baseline (`--all-targets` reaches tests and
# benches, which a bare `cargo check` never compiles). Warm, the pair costs
# ~5s; that is the whole budget this gate spends.

set -uo pipefail

cd "$(git rev-parse --show-toplevel)" || exit 1

# No toolchain means the tree cannot be verified. Refusing is the point of the
# gate — a silent skip is exactly how an unbuildable HEAD shipped before it
# existed. (No Rust was even installed on the VPS when the audit found that.)
if ! command -v cargo >/dev/null 2>&1; then
  echo "gate: cargo is not on PATH — cannot verify this tree." >&2
  echo "      Install the toolchain, or bypass with --no-verify." >&2
  exit 1
fi

echo "gate: cargo clippy --workspace --all-targets -D warnings"
if ! cargo clippy --workspace --all-targets --quiet -- -D warnings; then
  echo "gate: clippy failed." >&2
  exit 1
fi

echo "gate: cargo test --workspace"
if ! cargo test --workspace --quiet; then
  echo "gate: tests failed." >&2
  exit 1
fi

echo "gate: ok"
