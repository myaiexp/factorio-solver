#!/usr/bin/env bash
# Desktop launcher: fast-forward to origin/master, rebuild if stale, run the UI.

# The body is wrapped in a brace group so bash parses the whole file in one
# read. This script git-pulls the repo it lives in, and bash otherwise reads a
# script lazily by byte offset — it would resume a rewritten file mid-line.
{
  set -uo pipefail

  self=$(readlink -f "${BASH_SOURCE[0]}")
  repo=$(dirname "$(dirname "$self")")
  bin="$repo/target/release/factorio-ui"
  stamp="$repo/target/release/.launch-stamp"
  cd "$repo" || exit 1

  note() { command -v notify-send >/dev/null 2>&1 && notify-send -a "Factorio Solver" "$@"; }

  # --- sync -----------------------------------------------------------------
  # Only ever fast-forward a clean master. This checkout can hold live work,
  # and a launcher that discards it would be a very expensive convenience.
  held=""
  if [[ -n "$(git status --porcelain 2>/dev/null)" ]]; then
    held="Working tree is dirty — launching the checked-out version."
  elif [[ "$(git symbolic-ref --quiet --short HEAD 2>/dev/null)" != "master" ]]; then
    held="Not on master — launching the checked-out version."
  elif ! timeout 20 git fetch --quiet origin master 2>/dev/null; then
    held="Could not reach origin — launching the local version."
  elif ! git merge --ff-only --quiet FETCH_HEAD 2>/dev/null; then
    held="Local master has diverged from origin — launching the local version."
  fi
  [[ -n "$held" ]] && note "$held"

  # --- is the binary current? -----------------------------------------------
  head=$(git rev-parse HEAD 2>/dev/null || echo unknown)
  need_build=1
  if [[ -x "$bin" && "$(cat "$stamp" 2>/dev/null)" == "$head" ]]; then
    # The commit matches the binary, so only uncommitted edits can stale it.
    if [[ -z "$(find crates Cargo.toml Cargo.lock -newer "$bin" -print -quit 2>/dev/null)" ]]; then
      need_build=0
    fi
  fi

  # A cold release build takes minutes. Launched from a desktop entry there is
  # nowhere for that to show, so re-enter inside a terminal and build in view.
  if ((need_build)) && [[ ! -t 1 ]] && command -v xdg-terminal-exec >/dev/null 2>&1; then
    exec xdg-terminal-exec -- "$self"
  fi

  # --- build ----------------------------------------------------------------
  if ((need_build)); then
    echo "Building factorio-ui (release) — the first build takes a few minutes."
    if cargo build --release -p factorio-ui; then
      printf '%s' "$head" >"$stamp"
    else
      note -u critical "Build failed" "Launching the previous build, if there is one."
      [[ -t 1 ]] && read -rsp $'\nBuild failed. Press enter to close.\n'
      [[ -x "$bin" ]] || exit 1
    fi
  fi

  # --- run ------------------------------------------------------------------
  if [[ -t 1 ]]; then
    # Detach, so the terminal we opened to show the build can close behind us.
    setsid -f "$bin" >/dev/null 2>&1 </dev/null
  else
    exec "$bin"
  fi
}
