#!/usr/bin/env bash
# Usage: scripts/test-nightly.sh
# scripts/nightly-issue.sh and scripts/deny-ci.sh against a fake gh and just
# and a scratch repository: no network. Bash 3.2 compatible.
# Checks given to eval expand when they run.
# shellcheck disable=SC2016
source "$(dirname "$0")/lib.sh"
root=$(pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
failures=0

check() { # description command...
  local what=$1
  shift
  if "$@"; then
    echo "ok: $what"
  else
    echo "FAIL: $what" >&2
    failures=$((failures + 1))
  fi
}

mkdir -p "$tmp/bin"
# gh: `issue list` prints the open issue's number from $tmp/open (empty: none);
# every call is logged.
cat >"$tmp/bin/gh" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >>"$tmp/gh.log"
case "\$1 \$2" in
  "issue list") [ ! -s "$tmp/open" ] || cat "$tmp/open" ;;
  "issue close") : >"$tmp/open" ;;
  "issue create") echo 42 >"$tmp/open" ;;
esac
EOF
cat >"$tmp/bin/just" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$1" >>"$tmp/just.log"
EOF
chmod +x "$tmp/bin/gh" "$tmp/bin/just"
export PATH="$tmp/bin:$PATH" RUN_URL=https://run

report() {
  : >"$tmp/gh.log"
  bash scripts/nightly-issue.sh "$@" >/dev/null
}
calls() { grep -c "^issue $1" "$tmp/gh.log"; }

: >"$tmp/open"
report macos=success ios=success
check "a green night with no issue writes nothing" [ "$(wc -l <"$tmp/gh.log" | tr -d ' ')" = 1 ]
report macos=success ios=failure detection=skipped
check "a red night opens the issue" [ "$(calls create)" = 1 ]
check "naming each failed job and its result" grep -q 'Failed: ios (failure) detection (skipped)\. Passed: macos\. Run: https://run' "$tmp/gh.log"
report macos=failure
check "the next red night comments on it instead" eval '[ "$(calls comment)" = 1 ] && [ "$(calls create)" = 0 ] && grep -q "^issue comment 42 " "$tmp/gh.log"'
report macos=success ios=success
check "a green night closes it" grep -q '^issue close 42 --comment Green again: https://run (macos ios)\.' "$tmp/gh.log"
check "no arguments is an error" eval '! bash scripts/nightly-issue.sh >/dev/null 2>&1'

# deny-ci.sh in a repository whose last commit changes either the lock or not.
repo=$tmp/repo
mkdir -p "$repo/scripts"
cp scripts/deny-ci.sh scripts/lib.sh "$repo/scripts/"
(
  cd "$repo" || exit 1
  g() { git -c user.name=t -c user.email=t@t -c commit.gpgsign=false "$@" >/dev/null; }
  g init -q
  echo a >Cargo.lock && echo a >src.rs && g add -A && g commit -qm base
  base=$(git rev-parse HEAD)
  echo b >src.rs && g commit -qam code
  code=$(git rev-parse HEAD)
  echo b >Cargo.lock && g commit -qam lock
  lock=$(git rev-parse HEAD)
  run() {
    : >"$tmp/just.log"
    EVENT=$1 BASE=$2 RELEASE_CALL=${3:-false} bash scripts/deny-ci.sh >/dev/null
    tr '\n' ' ' <"$tmp/just.log"
  }
  check "a PR without a lock change checks the policy only" [ "$(git checkout -q "$code" && run pull_request "$base")" = "deny-policy " ]
  check "a lock change checks the advisories too" [ "$(git checkout -q "$lock" && run pull_request "$code")" = "deny-policy deny-advisories " ]
  check "so does a queue commit's" [ "$(run merge_group "$code")" = "deny-policy deny-advisories " ]
  # On a commit with no lock change, so only the flag can add the advisories.
  check "a release call checks both" [ "$(git checkout -q "$code" && run push "$base" true)" = "deny-policy deny-advisories " ]
  check "a manual run checks both" [ "$(run workflow_dispatch "")" = "deny-policy deny-advisories " ]
  check "an unknown base checks both" [ "$(run push 0000000000000000000000000000000000000000)" = "deny-policy deny-advisories " ]
  exit "$failures"
) || failures=$((failures + $?))

cd "$root" || exit 1
check "ci.yml's lint runs it" grep -q 'run: scripts/deny-ci.sh' .github/workflows/ci.yml
check "and no longer the full deny" eval '! grep -q "run: just deny$" .github/workflows/ci.yml'

if [ "$failures" -gt 0 ]; then
  echo "error: $failures nightly check(s) failed" >&2
  exit 1
fi
echo "nightly: all checks passed"
