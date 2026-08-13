#!/usr/bin/env bash
# Remembers which git *trees* the gate has already passed.
#
# Keyed on the tree, not the commit, and that is the whole point: `deploy`
# lands a session branch onto master, and when master has not moved the merge
# commit is a new sha over the *same* tree the pre-commit hook just built and
# tested. A commit-keyed memo would rebuild all of it from a cold `target/` in
# the main checkout; a tree-keyed one recognises it instantly.
#
# The file lives in the shared git dir rather than under `target/`, because
# those two checkouts do not share a `target/` — and a `cargo clean` must not
# be able to make the gate forget what it verified.

_stamp_file() {
  printf '%s/gate-verified-trees\n' "$(git rev-parse --git-common-dir)"
}

tree_is_verified() {
  local file
  file=$(_stamp_file)
  [ -f "$file" ] && grep -qxF "$1" "$file"
}

record_verified_tree() {
  local file
  file=$(_stamp_file)
  printf '%s\n' "$1" >>"$file"

  # Bounded, because nothing ever prunes this otherwise. 200 trees is far more
  # history than any land needs to reach back through. Rewritten via a temp
  # file and rename so a concurrent reader never sees a half-written list.
  local count
  count=$(wc -l <"$file" 2>/dev/null || echo 0)
  if [ "$count" -gt 200 ]; then
    tail -n 100 "$file" >"$file.tmp" && mv "$file.tmp" "$file"
  fi
}
