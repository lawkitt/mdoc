#!/usr/bin/env bash
# Usage: scripts/disk.sh
# Reports the size of each worktree's target/ (ADR 0050). Reports only;
# cleanup is `just retire-worktree <path>` or `cargo clean --profile dev`.
source "$(dirname "$0")/lib.sh"

total=0
while read -r tree; do
  if [ -d "$tree/target" ]; then
    kib=$(du -sk "$tree/target" | cut -f1)
    total=$((total + kib))
    printf '%8s  %s\n' "$((kib / 1024)) MiB" "$tree/target"
  else
    printf '%8s  %s\n' "-" "$tree (no target/)"
  fi
done < <(git worktree list --porcelain | awk '/^worktree / { print substr($0, 10) }')
printf '%8s  total\n' "$((total / 1024)) MiB"
