#!/usr/bin/env bash
# Usage: scripts/ci-cache-cleanup.sh
# Run by ci.yml on main after the jobs that save cargo caches (ADR 0082):
# keeps the newest cache of each job and deletes its older generations.
# Swatinem/rust-cache keys are `v0-rust-<job>-<os>-<arch>-<env hash>-<lock
# hash>`; a generation differs in its lock hash (Cargo files) or env hash
# (toolchain, CARGO* variables, the runner image's installed Rust versions).
# While a new runner image rolls out, two env hashes alternate and a run on
# the other image restores nothing; main runs have no time target, and the
# alternative is a second full cache set for every toolchain change, over
# the 10 GB repository limit. Needs GH_TOKEN with actions: write and GH_REPO.
source "$(dirname "$0")/lib.sh"

entries=$(gh cache list --ref refs/heads/main --key v0-rust- --sort created_at \
  --order desc --limit 100 --json id,key --jq '.[] | "\(.id) \(.key)"')
# Newest first, so every entry after the first of its job's key is older.
older=$(awk '{
  job = $2
  sub(/-[^-]*-[^-]*$/, "", job)
  if (job in newest) print $1, $2, newest[job]
  else newest[job] = $2
}' <<<"$entries")

deleted=0
while read -r id key newer; do
  [ -n "$id" ] || continue
  echo "deleting $key (newer: $newer)"
  gh cache delete "$id"
  deleted=$((deleted + 1))
done <<<"$older"
echo "deleted $deleted older cache generation(s)"
