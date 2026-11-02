#!/usr/bin/env bash
# Usage: scripts/snapshots-fetch.sh <pr|run> (`just snapshots-fetch`)
#
# Writes the detection gate's regenerated snapshots into the tree for an
# agent to review and commit (ADR 0095): a failing `ci/detection (<k>)`
# shard uploads the fresh snapshot of each stale fixture as the artifact
# `detection-snapshots-<k>`; no workflow writes to PR branches. <pr> is a PR
# number or URL: the newest of its runs (on its head branch or commit, or
# its merge-queue commit) that has such an artifact. <run> is a run URL or
# id (a number of more than 6 digits). Only `*.model.json` files beside an
# existing fixture are written; anything else in an artifact fails the
# fetch before a file changes. Needs gh, signed in. Bash 3.2 compatible.
source "$(dirname "$0")/lib.sh"

[ $# -eq 1 ] || die "usage: just snapshots-fetch <pr|run>"
fixtures=tests/fixtures/recognition
pr=""
run=""
case "$1" in
  */actions/runs/*)
    run=${1#*/actions/runs/}
    run=${run%%[!0-9]*}
    ;;
  */pull/*)
    pr=${1#*/pull/}
    pr=${pr%%[!0-9]*}
    ;;
  *[!0-9]* | "") ;;
  ???????*) run=$1 ;;
  *) pr=$1 ;;
esac
[ -n "$pr$run" ] || die "not a PR or a run: $1"

# Names of the run's unexpired detection-snapshots artifacts, one a line.
snapshot_artifacts() {
  gh api "repos/{owner}/{repo}/actions/runs/$1/artifacts?per_page=100" \
    --jq '.artifacts[] | select(.expired | not) | .name | select(startswith("detection-snapshots-"))'
}

if [ -n "$pr" ]; then
  head=$(gh pr view "$pr" --json headRefName,headRefOid --jq '.headRefName + " " + .headRefOid') ||
    die "cannot read PR #$pr"
  branch=${head% *}
  sha=${head#* }
  # Newest first, as gh lists them: the PR's own runs and its queue commits'
  # (gh-readonly-queue/<base>/pr-<n>-<sha>).
  candidates=$(gh run list --limit 200 --json databaseId,headBranch,headSha \
    --jq '.[] | [.databaseId, .headBranch, .headSha] | @tsv' |
    awk -F '\t' -v pr="$pr" -v branch="$branch" -v sha="$sha" '
      $2 == branch || $3 == sha || index($2, "gh-readonly-queue/") == 1 && $2 ~ ("/pr-" pr "-") { print $1 }
    ') || die "cannot list the runs"
  # A failed query stops the fetch: skipping that run would apply an older
  # run's snapshots.
  for id in $candidates; do
    names=$(snapshot_artifacts "$id") || die "cannot list run $id's artifacts"
    if [ -n "$names" ]; then
      run=$id
      break
    fi
  done
  [ -n "$run" ] || die "no run of PR #$pr has regenerated snapshots (a passing gate uploads none)"
fi

names=$(snapshot_artifacts "$run") || die "cannot list run $run's artifacts"
[ -n "$names" ] || die "run $run has no detection-snapshots artifact (passed, expired or not a detection run)"
# Which run, so a reviewer can tell it is the one wanted (a newer run may
# have passed).
gh run view "$run" --json workflowName,headBranch,headSha,createdAt \
  --jq '"run '"$run"': " + .workflowName + " on " + .headBranch + " at " + .headSha[0:8] + ", " + .createdAt' ||
  die "cannot read run $run"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
gh run download "$run" --pattern 'detection-snapshots-*' --dir "$tmp" || die "cannot download run $run's artifacts"

# Every file first, so a stray path changes nothing. An artifact holds the
# repository paths under its own folder.
list="$tmp/list"
: >"$list"
while IFS= read -r file; do
  rel=${file#"$tmp"/}
  case "$rel" in
    detection-snapshots-*/*) rel=${rel#*/} ;;
  esac
  case "$rel" in
    "$fixtures"/*.model.json) ;;
    *) die "unexpected file in run $run's artifacts: $rel" ;;
  esac
  [ -f "${rel%.model.json}.md" ] || die "no fixture for $rel (run $run tested other fixtures)"
  printf '%s\t%s\n' "$file" "$rel" >>"$list"
done < <(find "$tmp" -type f ! -path "$list")

[ -s "$list" ] || die "run $run's artifacts hold no snapshots"
count=0
while IFS="$(printf '\t')" read -r file rel; do
  cp "$file" "$rel"
  count=$((count + 1))
done <"$list"
echo "wrote $count snapshot(s) from run $run; review them with git diff -- $fixtures, then commit"
