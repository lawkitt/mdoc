#!/usr/bin/env bash
# Usage: scripts/test.sh (`just test`)
#
# The workspace tests through cargo-nextest (.config/nextest.toml), then
# doctests, which nextest does not run (ADR 0082). The script suites (release,
# CI cache cleanup, local CI runner and its hosted fallback) need no build and mostly wait on fakes and
# timers, so they run beside the Rust tests instead of after them: about a
# minute off a Windows run (#240). Each suite's output is printed after the
# Rust tests, in order; any failure fails the run.
# shellcheck source=lib.sh
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
suites=(test-release.sh test-ci-cache-cleanup.sh test-local-ci.sh test-local-ci-fallback.sh)
pids=()
# shellcheck disable=SC2317,SC2329 # runs from the trap
cleanup() {
  [ "${#pids[@]}" -eq 0 ] || kill "${pids[@]}" 2>/dev/null || true
  rm -rf "$tmp"
}
trap cleanup EXIT
for suite in "${suites[@]}"; do
  bash "scripts/$suite" </dev/null >"$tmp/$suite.log" 2>&1 &
  pids+=($!)
done

rc=0
cargo nextest run --workspace --features headless-renders || rc=$?
[ "$rc" -ne 0 ] || cargo test --doc --workspace --features headless-renders || rc=$?

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
