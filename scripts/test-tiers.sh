#!/usr/bin/env bash
# Usage: scripts/test-tiers.sh
# scripts/test.sh's tier dispatch (ADR 0095) against a fake cargo: each tier
# runs only its own commands, with the quarantined tests left out, and exits
# with their status. mdoc-detect's lib tests run from the binary nextest lists
# (a fake that logs its arguments), quarantined ones skipped by exact path,
# and their failures printed as nextest's (ADR 0113). Never runs --tier
# scripts or the whole run (this suite is one of their suites). Bash 3.2
# compatible.
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/bin"
cat >"$tmp/bin/cargo" <<'FAKE'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CARGO_LOG"
case "$*" in
  'test --doc'*) exit "${FAKE_DOCTEST_RC:-0}" ;;
  'nextest list'*'--list-type binaries-only'*)
    printf '{"rust-binaries":{"mdoc":{"binary-id":"mdoc","kind":"lib","binary-path":"/nowhere/mdoc"},'
    printf '"mdoc-detect":{"binary-id":"mdoc-detect","binary-name":"mdoc_detect","kind":"lib","binary-path":"%s"}}}\n' "$DETECT_BIN"
    ;;
  # Coloured names (CARGO_TERM_COLOR=always) unless asked for none.
  'nextest list'*'--color never'*) printf 'mdoc-detect rules::tests::flaky\nmdoc-detect lists::tests::also_flaky\n' ;;
  'nextest list'*) printf 'mdoc-detect \033[1mrules::tests::flaky\033[0m\nmdoc-detect \033[1mlists::tests::also_flaky\033[0m\n' ;;
  *) exit "${FAKE_CARGO_RC:-0}" ;;
esac
FAKE
# mdoc-detect's lib test binary: logs its arguments and folder.
cat >"$tmp/bin/detect-lib" <<'FAKE'
#!/usr/bin/env bash
printf 'detect-lib %s (in %s)\n' "$*" "${PWD##*/}" >>"$CARGO_LOG"
echo "test rules::tests::passes ... ok"
[ "${FAKE_DETECT_RC:-0}" = 0 ] || printf 'test rules::tests::broken ... FAILED\ntest lists::tests::odd ... FAILED\n'
exit "${FAKE_DETECT_RC:-0}"
FAKE
chmod +x "$tmp/bin/cargo" "$tmp/bin/detect-lib"
export PATH="$tmp/bin:$PATH" CARGO_LOG="$tmp/cargo.log" DETECT_BIN="$tmp/bin/detect-lib"
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

# What unit and rust run: nextest, then mdoc-detect's lib tests (the binary
# nextest lists, the quarantined tests among them, the binary), then the
# doctests.
ran() { # tier
  [ "$(wc -l <"$CARGO_LOG" | tr -d ' ')" = 5 ] || fail "$1 ran: $(cat "$CARGO_LOG")"
  sed -n 2p "$CARGO_LOG" | grep -q '^nextest list .*--list-type binaries-only' || fail "$1's binary list: $(cat "$CARGO_LOG")"
  sed -n 3p "$CARGO_LOG" | grep -qF -- '-E binary_id(mdoc-detect) & ((test(=flaky)))' ||
    fail "$1's quarantined mdoc-detect tests: $(cat "$CARGO_LOG")"
  # Each quarantined test listed is skipped by its exact path, in its
  # crate's folder (where nextest and cargo test run it).
  sed -n 4p "$CARGO_LOG" | grep -qxF 'detect-lib --exact --skip rules::tests::flaky --skip lists::tests::also_flaky (in mdoc-detect)' ||
    fail "$1's mdoc-detect lib tests: $(cat "$CARGO_LOG")"
  tail -n1 "$CARGO_LOG" | grep -q '^test --doc' || fail "$1's doctests: $(cat "$CARGO_LOG")"
}

# unit: nextest without mdoc-detect's lib tests, which follow in one
# process, then the doctests. A failure skips what follows it.
tier 0 unit
ran unit
head -n1 "$CARGO_LOG" | grep -qF -- '-E (!package(mdoc) - binary_id(mdoc-detect::recognition) - binary_id(mdoc-detect)) - ((test(=flaky)))' ||
  fail "unit's filter: $(cat "$CARGO_LOG")"
FAKE_CARGO_RC=4 tier 4 unit
[ "$(wc -l <"$CARGO_LOG" | tr -d ' ')" = 1 ] || fail "unit ran more after a failure: $(cat "$CARGO_LOG")"
FAKE_DOCTEST_RC=5 tier 5 unit
# A failing mdoc-detect lib test fails the tier before the doctests, and is
# named as nextest names one (the line the queue runner reads).
FAKE_DETECT_RC=101 tier 101 unit
[ "$(wc -l <"$CARGO_LOG" | tr -d ' ')" = 4 ] || fail "unit ran doctests after mdoc-detect failed: $(cat "$CARGO_LOG")"
for test in rules::tests::broken lists::tests::odd; do
  grep -qE "^ +FAIL \[[^]]*\] +mdoc-detect $test\$" "$tmp/out" || fail "mdoc-detect's failure $test unnamed: $(cat "$tmp/out")"
done
! grep -q 'FAIL.*passes' "$tmp/out" || fail "a passing mdoc-detect test named as failing: $(cat "$tmp/out")"
# Without a quarantine list: no list of quarantined tests, no --skip.
cp "$QUARANTINE_FILE" "$tmp/quarantine.kept"
: >"$QUARANTINE_FILE"
tier 0 unit
[ "$(wc -l <"$CARGO_LOG" | tr -d ' ')" = 4 ] || fail "unit without a list ran: $(cat "$CARGO_LOG")"
sed -n 3p "$CARGO_LOG" | grep -qxF 'detect-lib --exact (in mdoc-detect)' || fail "unit without a list: $(cat "$CARGO_LOG")"
cp "$tmp/quarantine.kept" "$QUARANTINE_FILE"

# rust: every Rust tier in one nextest run, then the doctests; no script
# suites and no quarantine check (the expired entry above would fail it).
tier 0 rust
ran rust
head -n1 "$CARGO_LOG" | grep -qF -- '-E (all() - binary_id(mdoc-detect)) - ((test(=flaky)))' || fail "rust's filter: $(cat "$CARGO_LOG")"
! grep -q 'scripts/\|quarantine' "$tmp/out" || fail "rust ran a script suite: $(cat "$tmp/out")"
FAKE_CARGO_RC=6 tier 6 rust
[ "$(wc -l <"$CARGO_LOG" | tr -d ' ')" = 1 ] || fail "rust ran more after a failure: $(cat "$CARGO_LOG")"
FAKE_DETECT_RC=101 tier 101 rust
FAKE_DOCTEST_RC=2 tier 2 rust

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
