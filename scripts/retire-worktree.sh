#!/usr/bin/env bash
# Usage: scripts/retire-worktree.sh <path>
# Removes a retired git worktree together with its target/ (ADR 0050). The path
# is absolute or relative to the repository root. Refuses the main worktree and
# any worktree with uncommitted or untracked changes; never forces. The branch
# is kept.
source "$(dirname "$0")/lib.sh"

[ $# -eq 1 ] || die "usage: just retire-worktree <path>"
[ -d "$1" ] || die "$1 is not a directory"
path=$(cd "$1" && pwd -P)

main=$(git worktree list --porcelain | awk '/^worktree / { print substr($0, 10); exit }')
[ "$path" != "$(cd "$main" && pwd -P)" ] || die "refusing to retire the main worktree"
git worktree list --porcelain | awk '/^worktree / { print substr($0, 10) }' |
  while read -r tree; do [ "$(cd "$tree" 2>/dev/null && pwd -P)" = "$path" ] && echo yes; done |
  grep -q yes || die "$path is not a worktree of this repository"

changes=$(git -C "$path" status --porcelain)
[ -z "$changes" ] || die "$path has uncommitted or untracked changes:
$changes"

if [ -d "$path/target" ]; then
  echo "removing $(du -sh "$path/target" | cut -f1) in $path/target"
  rm -rf "$path/target"
fi
git worktree remove "$path"
echo "retired $path (branch kept)"
