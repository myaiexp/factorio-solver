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
  # A desktop entry starts with no terminal and often no ssh-agent, so the
  # fetch must fail fast rather than block the launch on a prompt it cannot
  # show. The timeout is the backstop; the ssh options are what make it rare.
  elif ! GIT_TERMINAL_PROMPT=0 \
    GIT_SSH_COMMAND='ssh -o BatchMode=yes -o ConnectTimeout=8' \
    timeout 20 git fetch --quiet origin master 2>/dev/null; then
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

    # Pick the first cargo on PATH that actually RUNS, not the first that
    # exists. A graphical session's PATH is not a login shell's: omarchy puts
    # rustup's shim directory ahead of /usr/bin, and that rustup has no default
    # toolchain, so `cargo` exited 1 before compiling anything. It read as "the
    # build is broken" because the launcher only invokes cargo when HEAD has
    # moved — every launch in between had nothing to build and looked healthy.
    # A rustup pinned by a rust-toolchain file still wins here: it installs on
    # demand and answers --version. Only an unusable shim gets skipped.
    cargo_bin=""
    while read -r c; do
      if [[ -x "$c" ]] && "$c" --version >/dev/null 2>&1; then
        cargo_bin="$c"
        break
      fi
    done < <(type -aP cargo 2>/dev/null; echo /usr/bin/cargo)

    built=0
    if [[ -z "$cargo_bin" ]]; then
      echo "No working cargo found. Candidates tried:"
      type -aP cargo 2>/dev/null || echo "  (none on PATH)"
    elif "$cargo_bin" build --release -p factorio-ui; then
      printf '%s' "$head" >"$stamp"
      built=1
    fi

    if ((!built)); then
      # Name what the fallback actually is. The old message said "the previous
      # build, if there is one", so a failed build opened an app that looked
      # healthy while running a binary 24 commits old — the one failure mode
      # here worth being loud about, since nothing downstream can detect it.
      was=$(cat "$stamp" 2>/dev/null)
      stale="no previous build"
      if [[ -x "$bin" ]]; then
        stale="the previous build"
        if [[ -n "$was" ]]; then
          stale="the build from ${was:0:7}"
          n=$(git rev-list --count "$was..$head" 2>/dev/null)
          [[ -n "$n" && "$n" != 0 ]] && stale+=", $n commits behind"
        fi
      fi
      note -u critical "Build failed" "Launching $stale."
      [[ -t 1 ]] && read -rsp "
Build failed — launching $stale.
Press enter to close.
"
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
