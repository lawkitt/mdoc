#!/usr/bin/env bash
# Usage: scripts/hub-gate.sh <PR number>
#
# The recognition hub's gate before it enqueues a PR (ADR 0108). In a
# worktree of its own it checks out origin/main, merges the head of every PR
# ahead of the candidate in main's merge queue (all of them when the
# candidate is not queued yet), then the candidate's head, and runs
# `cargo nextest run --locked -p mdoc-detect -p mdoc-pii --no-fail-fast`.
# When MDOC_HOLDOUT names the private holdout's folder it also scores it
# (`just recognize --scoreboard`, the real model) and fails when overall
# recall or precision falls below MDOC_HOLDOUT_RECALL / _PRECISION (97.6 and
# 94.0 by default, the holdout on main 49b525ce). The last line is a summary
# for the run log: aggregates only, never a document.
#
# MDOC_HUB_GATE_DIR is the worktree (default: a sibling of this checkout
# named <checkout>-hub-gate); it keeps its own target/ between runs.
source "$(dirname "$0")/lib.sh"

[ $# -eq 1 ] && [[ $1 =~ ^[0-9]+$ ]] || die "usage: scripts/hub-gate.sh <PR number>"
pr=$1
repo=lawkitt/mdoc
dir=${MDOC_HUB_GATE_DIR:-$(dirname "$PWD")/$(basename "$PWD")-hub-gate}
min_recall=${MDOC_HOLDOUT_RECALL:-97.6}
min_precision=${MDOC_HOLDOUT_PRECISION:-94.0}

command -v gh >/dev/null || die "gh is needed to read the merge queue"

# The PRs ahead of the candidate in main's merge queue, in queue order.
# shellcheck disable=SC2016 # GraphQL variables, not shell ones
queue=$(gh api graphql -F owner="${repo%/*}" -F name="${repo#*/}" -f query='
  query($owner: String!, $name: String!) {
    repository(owner: $owner, name: $name) {
      mergeQueue(branch: "main") {
        entries(first: 50) { nodes { position pullRequest { number } } }
      }
    }
  }' --jq '.data.repository.mergeQueue.entries.nodes[] | "\(.position) \(.pullRequest.number)"') ||
  die "cannot read main's merge queue"
mine=$(awk -v pr="$pr" '$2 == pr { print $1 }' <<<"$queue")
ahead=$(sort -n <<<"$queue" | awk -v pr="$pr" -v mine="${mine:-0}" \
  'NF == 2 && $2 != pr && (mine == 0 || $1 < mine) { print $2 }' | tr '\n' ' ')

refs=(+refs/heads/main:refs/remotes/origin/main)
for n in $ahead $pr; do refs+=("+refs/pull/$n/head:refs/hub-gate/pr-$n"); done
git fetch -q origin "${refs[@]}" || die "cannot fetch main and the PRs' heads"

if [ ! -e "$dir/.git" ]; then
  git worktree add -q --detach "$dir" origin/main || die "cannot add the gate worktree at $dir"
fi
lock="$dir/.hub-gate.lock"
mkdir "$lock" 2>/dev/null || die "another hub-gate run holds $lock"
trap 'rmdir "$lock"' EXIT

g() { git -C "$dir" -c user.name=hub-gate -c user.email=hub-gate@invalid "$@"; }
g merge --abort 2>/dev/null || true
g checkout -q --detach --force origin/main
g reset -q --hard origin/main
base=$(g rev-parse --short=8 HEAD)
stack="main $base"
for n in $ahead $pr; do
  g merge -q --no-ff --no-edit -m "hub-gate: #$n" "refs/hub-gate/pr-$n" >/dev/null ||
    die "#$n does not merge onto $stack (conflict)"
  stack+=" + #$n"
done
stack+=" ($(git rev-parse --short=8 "refs/hub-gate/pr-$pr"))"
echo "hub-gate: $stack"

log=$(mktemp)
trap 'rmdir "$lock"; rm -f "$log"' EXIT

set +e
(cd "$dir" && cargo nextest run --locked -p mdoc-detect -p mdoc-pii --no-fail-fast) 2>&1 | tee "$log"
tests=${PIPESTATUS[0]}
set -e
summary=$(grep -E '^ *Summary ' "$log" | tail -1 | sed -E 's/^ *Summary +\[[^]]*\] +//')
[ "$tests" -eq 0 ] || {
  echo "hub-gate #$pr: FAIL tests ($summary) on $stack"
  exit 1
}

holdout="holdout not run (MDOC_HOLDOUT unset)"
if [ -n "${MDOC_HOLDOUT:-}" ]; then
  [ -d "$MDOC_HOLDOUT" ] || die "MDOC_HOLDOUT is not a folder: $MDOC_HOLDOUT"
  (cd "$dir" && just recognize --scoreboard "$MDOC_HOLDOUT") >"$log" 2>&1 ||
    die "the holdout run failed: $(tail -5 "$log")"
  # The scoreboard's last "all all all" row: gold found recall proposed right precision.
  read -r recall precision < <(awk '$1 == "all" && $2 == "all" && $3 == "all" {
      r = $6; p = $9; sub(/%/, "", r); sub(/%/, "", p) } END { print r, p }' "$log")
  [ -n "$recall" ] && [ -n "$precision" ] || die "no overall row in the holdout's scoreboard"
  passing=$(grep -E '^[0-9]+ fixtures, [0-9]+ passing' "$log" | sed -E 's/^([0-9]+) fixtures, ([0-9]+) passing.*/\2 of \1 documents pass/')
  holdout="holdout recall $recall% precision $precision%${passing:+, $passing}"
  if awk -v r="$recall" -v p="$precision" -v mr="$min_recall" -v mp="$min_precision" \
    'BEGIN { exit !(r < mr || p < mp) }'; then
    echo "hub-gate #$pr: FAIL $holdout (floor $min_recall% / $min_precision%) on $stack"
    exit 1
  fi
fi

echo "hub-gate #$pr: PASS tests ($summary); $holdout; on $stack"
