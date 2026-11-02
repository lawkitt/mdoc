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
  newer=0
  for id in $candidates; do
    if [ -n "$(snapshot_artifacts "$id")" ]; then
      run=$id
      break
    fi
    newer=$((newer + 1))
  done
  [ -n "$run" ] || die "no run of PR #$pr has regenerated snapshots (a passing gate uploads none)"
  [ "$newer" -eq 0 ] || echo "note: $newer newer run(s) of PR #$pr have no regenerated snapshots; check they are still wanted" >&2
fi

[ -n "$(snapshot_artifacts "$run")" ] || die "run $run has no detection-snapshots artifact (passed, expired or not a detection run)"
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
