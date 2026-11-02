#!/usr/bin/env bash
# Usage: scripts/test-tiers.sh
# scripts/test.sh's tier dispatch (ADR 0095) against a fake cargo: each tier
# runs only its own commands, with the quarantined tests left out, and exits
# with their status. Never runs --tier scripts or the whole run (this suite is
# one of their suites). Bash 3.2 compatible.
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/bin"
cat >"$tmp/bin/cargo" <<'FAKE'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CARGO_LOG"
case "$*" in
  'test --doc'*) exit "${FAKE_DOCTEST_RC:-0}" ;;
  *) exit "${FAKE_CARGO_RC:-0}" ;;
esac
FAKE
chmod +x "$tmp/bin/cargo"
export PATH="$tmp/bin:$PATH" CARGO_LOG="$tmp/cargo.log"
# An expired entry: a tier that fell through to the quarantine check would
# fail on it.
printf 'test(=flaky)  #1  2020-01-01\n' >"$tmp/quarantine"
export QUARANTINE_FILE="$tmp/quarantine"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

tier() { # rc tier
  : >"$CARGO_LOG"
  local rc=0
  bash scripts/test.sh --tier "$2" >"$tmp/out" 2>&1 || rc=$?
  [ "$rc" = "$1" ] || fail "--tier $2 exited $rc, not $1: $(cat "$tmp/out")"
}

tier 0 app
[ "$(wc -l <"$CARGO_LOG" | tr -d ' ')" = 1 ] || fail "app ran: $(cat "$CARGO_LOG")"
grep -qF -- '-E (package(mdoc) - binary_id(mdoc::headless_renders)) - ((test(=flaky)))' "$CARGO_LOG" ||
  fail "app's filter: $(cat "$CARGO_LOG")"

FAKE_CARGO_RC=7 tier 7 renders
grep -qF 'binary_id(mdoc::headless_renders)' "$CARGO_LOG" || fail "renders' filter: $(cat "$CARGO_LOG")"

tier 0 recognition
grep -qF 'binary_id(mdoc-detect::recognition)' "$CARGO_LOG" || fail "recognition's filter: $(cat "$CARGO_LOG")"

# unit: nextest, then the doctests; a nextest failure skips them.
tier 0 unit
[ "$(wc -l <"$CARGO_LOG" | tr -d ' ')" = 2 ] || fail "unit ran: $(cat "$CARGO_LOG")"
grep -qF '!package(mdoc) - binary_id(mdoc-detect::recognition)' "$CARGO_LOG" || fail "unit's filter: $(cat "$CARGO_LOG")"
tail -n1 "$CARGO_LOG" | grep -q '^test --doc' || fail "unit's doctests: $(cat "$CARGO_LOG")"
FAKE_CARGO_RC=4 tier 4 unit
[ "$(wc -l <"$CARGO_LOG" | tr -d ' ')" = 1 ] || fail "unit ran doctests after a failure: $(cat "$CARGO_LOG")"
FAKE_DOCTEST_RC=5 tier 5 unit

# quarantined: only the listed tests, and never a failure.
FAKE_CARGO_RC=3 tier 0 quarantined
grep -qF -- '-E (test(=flaky))' "$CARGO_LOG" || fail "quarantined's filter: $(cat "$CARGO_LOG")"
: >"$tmp/quarantine"
tier 0 quarantined
[ ! -s "$CARGO_LOG" ] || fail "an empty list ran cargo: $(cat "$CARGO_LOG")"

# An unknown tier is an error before anything runs.
tier 1 everything
[ ! -s "$CARGO_LOG" ] || fail "an unknown tier ran cargo"

echo "test tiers: all checks passed"
