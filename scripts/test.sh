#!/usr/bin/env bash
# Usage: scripts/test.sh [--tier unit|app|renders|recognition|scripts|quarantined]
# (`just test`, `just test-tier <tier>`)
#
# The workspace tests through cargo-nextest (.config/nextest.toml), then
# doctests, which nextest does not run (ADR 0082). The script suites (release,
# CI cache cleanup, local CI runner and its hosted fallback, the quarantine
# list, the merge gate, the merge-queue runner) need no build and mostly wait on fakes and timers, so they run beside
# the Rust tests instead of after them: about a minute off a Windows run
# (#240). Each suite's output is printed after the Rust tests, in order; any
# failure fails the run.
#
# With --tier, one test tier (ADR 0095), which CI runs as its own job:
#   unit         the crates outside the app package, without the recognition
#                fixtures, and the doctests
#   app          the app package's tests without the render test
#   renders      the render test
#   recognition  the recognition fixtures on their snapshots
#   scripts      the script suites and the quarantine list's expiry check
#   quarantined  only the quarantined tests (.config/quarantine.txt); reports
#                and never fails
# Without it, every tier but quarantined. Quarantined tests are left out of
# every other tier. Bash 3.2 compatible (macOS /bin/bash).
# shellcheck source=lib.sh
source "$(dirname "$0")/lib.sh"

tier=all
case "${1:-}" in
  '') ;;
  --tier)
    tier=${2:-}
    case "$tier" in
      unit | app | renders | recognition | scripts | quarantined) ;;
      *) die "unknown tier '${tier}': unit, app, renders, recognition, scripts or quarantined" ;;
    esac
    ;;
  *) die "usage: scripts/test.sh [--tier unit|app|renders|recognition|scripts|quarantined]" ;;
esac

# Each tier's nextest filterset; `all` is their union.
renders_filter='binary_id(mdoc::headless_renders)'
recognition_filter='binary_id(mdoc-detect::recognition)'
app_filter="package(mdoc) - $renders_filter"
unit_filter="!package(mdoc) - $recognition_filter"

quarantined=$(bash scripts/quarantine.sh filter) || die "scripts/quarantine.sh: .config/quarantine.txt does not parse"

# Runs nextest over $1, without the quarantined tests.
nextest() {
  local filter=$1
  [ -z "$quarantined" ] || filter="($filter) - ($quarantined)"
  cargo nextest run --workspace --features headless-renders --no-tests=pass -E "$filter"
}

doctests() { cargo test --doc --workspace --features headless-renders; }

suites=(test-release.sh test-ci-cache-cleanup.sh test-local-ci.sh test-local-ci-fallback.sh test-quarantine.sh test-tiers.sh test-merge-gate.sh test-queue-runner.sh test-ios-build-info.sh)

# Runs one tier's commands and exits with their status; nothing else runs.
only() {
  local rc=0
  "$@" || rc=$?
  exit "$rc"
}

# shellcheck disable=SC2317,SC2329 # run through only
unit() {
  nextest "$unit_filter" && doctests
}

case "$tier" in
  unit) only unit ;;
  app) only nextest "$app_filter" ;;
  renders) only nextest "$renders_filter" ;;
  recognition) only nextest "$recognition_filter" ;;
  quarantined)
    if [ -z "$quarantined" ]; then
      echo "quarantine: no tests listed"
      exit 0
    fi
    cargo nextest run --workspace --features headless-renders --no-tests=pass --no-fail-fast -E "$quarantined" ||
      echo "quarantine: some quarantined tests still fail (not a failure; see .config/quarantine.txt)"
    exit 0
    ;;
esac

# The script suites: alone for --tier scripts, beside the Rust tests for all.
tmp=$(mktemp -d)
pids=()
# shellcheck disable=SC2317,SC2329 # runs from the trap
cleanup() {
  [ "${#pids[@]}" -eq 0 ] || kill "${pids[@]}" 2>/dev/null || true
  rm -rf "$tmp"
}
trap cleanup EXIT
# Suites added on their own line (snapshots fetch) merge without touching the list above.
suites+=(test-snapshots-fetch.sh)
suites+=(test-nightly.sh)
suites+=(test-ci-stats.sh)
for suite in "${suites[@]}"; do
  bash "scripts/$suite" </dev/null >"$tmp/$suite.log" 2>&1 &
  pids+=($!)
done

rc=0
bash scripts/quarantine.sh check || rc=1
if [ "$tier" = all ]; then
  nextest "all()" || rc=1
  [ "$rc" -ne 0 ] || doctests || rc=$?
fi

for i in "${!suites[@]}"; do
  echo "scripts/${suites[$i]}"
  suite_rc=0
  wait "${pids[$i]}" || suite_rc=$?
  cat "$tmp/${suites[$i]}.log"
  if [ "$suite_rc" -ne 0 ]; then
    echo "error: scripts/${suites[$i]} failed (exit $suite_rc)" >&2
    rc=1
  fi
done
pids=()
exit "$rc"
