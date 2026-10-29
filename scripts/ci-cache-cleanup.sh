#!/usr/bin/env bash
# Usage: scripts/ci-cache-cleanup.sh
# Run by ci.yml on main after the jobs that save cargo caches (ADR 0082):
# deletes every older generation of each Swatinem/rust-cache key on main and
# keeps the newest. Its keys are `v0-rust-<job>-<os>-<arch>-<env hash>-<lock
# hash>` and a job restores only by the part before the lock hash, so an entry
# is an older generation when a newer one shares that part. An entry with
# another env hash (another runner image or toolchain) is left to GitHub's
# 7-day eviction. Needs GH_TOKEN with actions: write and GH_REPO.
source "$(dirname "$0")/lib.sh"

entries=$(gh cache list --ref refs/heads/main --key v0-rust- --sort created_at \
  --order desc --limit 100 --json id,key --jq '.[] | "\(.id) \(.key)"')
# Newest first, so every entry after the first of its restore key is older.
older=$(awk '{
  restore = $2
  sub(/-[^-]*$/, "", restore)
  if (restore in newest) print $1, $2, newest[restore]
  else newest[restore] = $2
}' <<<"$entries")

deleted=0
while read -r id key newer; do
  [ -n "$id" ] || continue
  echo "deleting $key (newer: $newer)"
  gh cache delete "$id"
  deleted=$((deleted + 1))
done <<<"$older"
echo "deleted $deleted older cache generation(s)"
