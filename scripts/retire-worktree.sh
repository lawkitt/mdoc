#!/usr/bin/env bash
# Usage: scripts/retire-worktree.sh <path>
# Removes a retired git worktree together with its Cargo target/ directories
# (ADR 0050). The path is absolute or relative to the repository root. Refuses
# the main worktree and any worktree with uncommitted, untracked or ignored
# files other than target/ and .DS_Store, since `git worktree remove` would
# delete ignored files such as dist/ or .qualification/; never forces. The
# branch is kept.
source "$(dirname "$0")/lib.sh"

[ $# -eq 1 ] || die "usage: just retire-worktree <path>"
[ -d "$1" ] || die "$1 is not a directory"
path=$(cd "$1" && pwd -P)

main=$(git worktree list --porcelain | awk '/^worktree / { print substr($0, 10); exit }')
[ "$path" != "$(cd "$main" && pwd -P)" ] || die "refusing to retire the main worktree"
git worktree list --porcelain | awk '/^worktree / { print substr($0, 10) }' |
  while read -r tree; do [ "$(cd "$tree" 2>/dev/null && pwd -P)" = "$path" ] && echo yes; done |
  grep -q yes || die "$path is not a worktree of this repository"

targets=() kept=()
while IFS= read -r -d '' entry; do
  case $entry in
    '!! target/' | '!! '*/target/) targets+=("${entry#!! }") ;;
    '!! .DS_Store' | '!! '*/.DS_Store) ;;
    *) kept+=("$entry") ;;
  esac
done < <(git -C "$path" status --porcelain -z --ignored)
[ ${#kept[@]} -eq 0 ] || die "$path has files that removal would lose:
$(printf '  %s\n' "${kept[@]}")"

for dir in ${targets[@]+"${targets[@]}"}; do
  echo "removing $(du -sh "$path/$dir" | cut -f1) in $path/$dir"
  rm -rf "${path:?}/$dir"
done
git worktree remove "$path"
echo "retired $path (branch kept)"
