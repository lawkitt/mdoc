#!/usr/bin/env bash
# Usage: scripts/test-snapshots-fetch.sh
# Runs scripts/snapshots-fetch.sh in a scratch copy of the scripts against a
# fake gh; the real tree and GitHub are never touched.
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
repo="$tmp/repo"
mkdir -p "$tmp/bin" "$repo/scripts" "$repo/tests/fixtures/recognition/en/contract"
cp scripts/lib.sh scripts/snapshots-fetch.sh "$repo/scripts/"
fixture="$repo/tests/fixtures/recognition/en/contract/lease"
echo fixture >"$fixture.md"
echo old >"$fixture.model.json"
# GH_RUNS: `gh run list` rows; GH_ARTIFACTS: "<run> <name>" lines; GH_FILES:
# "<artifact>/<path>" lines the download writes, each holding "new".
export GH_RUNS="$tmp/runs" GH_ARTIFACTS="$tmp/artifacts" GH_FILES="$tmp/files" GH_LOG="$tmp/gh.log"
cat >"$tmp/bin/gh" <<'FAKE'
#!/usr/bin/env bash
set -eu
echo "$*" >>"$GH_LOG"
case "$*" in
  'pr view 523 --json headRefName,headRefOid --jq '*) echo "mac/ci-detection abc123" ;;
  'pr view '*) echo "no such PR" >&2 && exit 1 ;;
  'run view '*' --json workflowName,headBranch,headSha,createdAt --jq '*) echo "run $3: Queue on q at ddd, 2026-11-02" ;;
  'run list --limit 200 --json databaseId,headBranch,headSha --jq '*) cat "$GH_RUNS" ;;
  'api repos/{owner}/{repo}/actions/runs/'*)
    id=${2#repos/\{owner\}/\{repo\}/actions/runs/}
    id=${id%%/*}
    [ "$id" != "${GH_API_FAILS:-}" ] || { echo "HTTP 503" >&2 && exit 1; }
    awk -v id="$id" '$1 == id { print $2 }' "$GH_ARTIFACTS" ;;
  "run download "*" --pattern detection-snapshots-* --dir "*)
    while read -r path; do
      mkdir -p "$7/$(dirname "$path")"
      echo new >"$7/$path"
    done <"$GH_FILES" ;;
  *) echo "unexpected gh $*" >&2 && exit 90 ;;
esac
FAKE
chmod +x "$tmp/bin/gh"
export PATH="$tmp/bin:$PATH"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
fetch() {
  : >"$GH_LOG"
  echo old >"$fixture.model.json"
  bash "$repo/scripts/snapshots-fetch.sh" "$@" >"$tmp/out" 2>&1
}
snapshot=detection-snapshots-1/tests/fixtures/recognition/en/contract/lease.model.json
tab=$(printf '\t')

# A PR: the newest run with snapshots, here its merge-queue commit's; an
# older PR's queue run and another branch's run are not its runs.
cat >"$GH_RUNS" <<ROWS
9000000005${tab}gh-readonly-queue/main/pr-52-fff${tab}fff
9000000004${tab}mac/ci-detection${tab}abc123
9000000003${tab}gh-readonly-queue/main/pr-523-ddd${tab}ddd
9000000002${tab}mac/other${tab}eee
ROWS
printf '%s\n' "9000000005 detection-snapshots-1" "9000000003 detection-snapshots-1" "9000000003 detection-snapshots-2" >"$GH_ARTIFACTS"
echo "$snapshot" >"$GH_FILES"
fetch 523 || fail "PR fetch exited non-zero: $(cat "$tmp/out")"
[ "$(cat "$fixture.model.json")" = new ] || fail "PR fetch did not write the snapshot"
grep -q '^run download 9000000003 ' "$GH_LOG" || fail "PR fetch downloaded: $(grep download "$GH_LOG")"
grep -q '^run 9000000003: ' "$tmp/out" || fail "does not name the run: $(cat "$tmp/out")"
grep -q 'wrote 1 snapshot(s) from run 9000000003' "$tmp/out" || fail "summary: $(cat "$tmp/out")"

# A newer run whose artifacts cannot be listed stops the fetch, rather than
# falling back to an older run's snapshots.
GH_API_FAILS=9000000004 fetch 523 && fail "a failed artifact query fell back to an older run"
grep -q "cannot list run 9000000004's artifacts" "$tmp/out" || fail "query-failure message: $(cat "$tmp/out")"
! grep -q '^run download' "$GH_LOG" || fail "a failed artifact query still downloaded"
[ "$(cat "$fixture.model.json")" = old ] || fail "a failed artifact query wrote a snapshot"
GH_API_FAILS=9000000005 fetch 9000000005 && fail "a failed artifact query of the run fetched"
grep -q "cannot list run 9000000005's artifacts" "$tmp/out" || fail "run query-failure message: $(cat "$tmp/out")"

# A PR URL and a run URL; a long number is a run.
fetch https://github.com/lawkitt/mdoc/pull/523 || fail "PR URL: $(cat "$tmp/out")"
grep -q '^run download 9000000003 ' "$GH_LOG" || fail "PR URL downloaded: $(grep download "$GH_LOG")"
fetch https://github.com/lawkitt/mdoc/actions/runs/9000000005/job/1 || fail "run URL: $(cat "$tmp/out")"
grep -q '^run download 9000000005 ' "$GH_LOG" || fail "run URL downloaded: $(grep download "$GH_LOG")"
fetch 9000000005 || fail "run id: $(cat "$tmp/out")"
[ "$(cat "$fixture.model.json")" = new ] || fail "run id did not write the snapshot"

# No snapshots: a passing run, a PR without any, not a number.
! fetch 9000000002 || fail "a run without snapshots fetched"
grep -q 'no detection-snapshots artifact' "$tmp/out" || fail "no-artifact message: $(cat "$tmp/out")"
: >"$GH_ARTIFACTS"
! fetch 523 || fail "a PR without snapshots fetched"
grep -q 'no run of PR #523 has regenerated snapshots' "$tmp/out" || fail "no-run message: $(cat "$tmp/out")"
! fetch main || fail "a branch name fetched"
grep -q 'not a PR or a run' "$tmp/out" || fail "bad-target message: $(cat "$tmp/out")"

# A stray path or a snapshot without its fixture changes nothing.
echo "9000000005 detection-snapshots-1" >"$GH_ARTIFACTS"
printf '%s\n' "$snapshot" "detection-snapshots-1/scripts/lib.sh" >"$GH_FILES"
! fetch 9000000005 || fail "a stray file fetched"
[ "$(cat "$fixture.model.json")" = old ] || fail "a failed fetch wrote a snapshot"
grep -q 'unexpected file in run 9000000005' "$tmp/out" || fail "stray message: $(cat "$tmp/out")"
printf '%s\n' "$snapshot" "detection-snapshots-1/tests/fixtures/recognition/en/contract/gone.model.json" >"$GH_FILES"
! fetch 9000000005 || fail "an orphan snapshot fetched"
[ "$(cat "$fixture.model.json")" = old ] || fail "an orphan snapshot wrote a snapshot"
grep -q 'no fixture for tests/fixtures/recognition/en/contract/gone.model.json' "$tmp/out" || fail "orphan message: $(cat "$tmp/out")"

echo "snapshots-fetch: all checks passed"
