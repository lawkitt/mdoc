#!/usr/bin/env bash
# Isolated Git histories and fake gh/just/cargo; never pushes or builds.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export REAL_GIT
REAL_GIT=$(type -P git)
mkdir -p "$tmp/bin" "$tmp/repo"
export GH_LOG="$tmp/gh.log" CHECK_LOG="$tmp/check.log" PUSH_LOG="$tmp/push.log"
export PROMPT_LOG="$tmp/prompt.log"
cat >"$tmp/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -eu
printf '%s\n' "$@" > "$GH_LOG"
args="$*"
[[ "$args" == 'run list --workflow '* ]] || exit 90
while [ "$1" != --commit ]; do shift; done
sha=$2
if [[ "$args" == 'run list --workflow release.yml --commit '* ]]; then
  # The tag's Release run for a release commit.
  [[ "$args" == *"--branch $GH_RELEASE_TAG --event push --limit 1 --json "* ]] || exit 94
  [ "$sha" = "$GH_RUN_SHA" ] || exit 0
  case "${GH_RELEASE_RESULT:-missing}" in
    missing) ;;
    green) printf 'completed|success|https://github.com/example/mdoc/actions/runs/43|%s|push|Release|%s\n' "$sha" "$GH_RELEASE_TAG" ;;
    failed) printf 'completed|failure|https://github.com/example/mdoc/actions/runs/43|%s|push|Release|%s\n' "$sha" "$GH_RELEASE_TAG" ;;
    pending) printf 'in_progress||https://github.com/example/mdoc/actions/runs/43|%s|push|Release|%s\n' "$sha" "$GH_RELEASE_TAG" ;;
    *) exit 95 ;;
  esac
  exit 0
fi
[[ "$args" == 'run list --workflow ci.yml --commit '* ]] || exit 90
[[ "$args" == *'--branch main --event push --limit 1 --json '* ]] || exit 91
[ "${GH_RESULT:-}" != error ] || exit 1
[ "$sha" = "$GH_RUN_SHA" ] || exit 0
case "$GH_RESULT" in
  skipped) printf 'completed|skipped|https://github.com/example/mdoc/actions/runs/42|%s|push|CI\n' "$sha" ;;
  missing) ;;
  green) printf 'completed|success|https://github.com/example/mdoc/actions/runs/42|%s|push|CI\n' "$sha" ;;
  failed) printf 'completed|failure|https://github.com/example/mdoc/actions/runs/42|%s|push|CI\n' "$sha" ;;
  pending) printf 'in_progress||https://github.com/example/mdoc/actions/runs/42|%s|push|CI\n' "$sha" ;;
  wrong-event) printf 'completed|success|https://github.com/example/mdoc/actions/runs/42|%s|pull_request|CI\n' "$sha" ;;
  *) exit 92 ;;
esac
EOF
cat >"$tmp/bin/git" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = push ]; then
  printf '%s\n' "$*" >> "$PUSH_LOG"
  exit 0
fi
exec "$REAL_GIT" "$@"
EOF
cat >"$tmp/bin/just" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$CHECK_LOG"
EOF
cat >"$tmp/bin/cargo" <<'EOF'
#!/usr/bin/env bash
# Minimal workspace fixture: model cargo update's local-package version bump.
[ "$*" = 'update --workspace --quiet' ] || exit 93
v=$(awk '/^version = / { gsub(/"/, "", $3); print $3; exit }' Cargo.toml)
printf '[[package]]\nname = "mdoc"\nversion = "%s"\n' "$v" > Cargo.lock
EOF
chmod +x "$tmp/bin/gh" "$tmp/bin/git" "$tmp/bin/just" "$tmp/bin/cargo"
export PATH="$tmp/bin:$PATH"
export GH_RESULT=green GH_RUN_SHA GH_RELEASE_RESULT=missing GH_RELEASE_TAG=none

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
contains() { grep -F -- "$2" "$1" >/dev/null || fail "missing '$2' in $1"; }
commit() {
  git add .
  git commit --quiet -m "$1"
  git update-ref refs/remotes/origin/main HEAD
}
green() {
  release_ci_green >"$tmp/result" || fail "expected green: $(cat "$tmp/result")"
  contains "$tmp/result" "$GH_RUN_SHA"
  contains "$tmp/result" 'https://github.com/example/mdoc/actions/runs/42'
}
local_checks() {
  if release_ci_green >"$tmp/result"; then fail 'unexpected green'; fi
  contains "$tmp/result" "$1"
}

cd "$tmp/repo"
git init --quiet -b main
git config user.name 'Release script test'
git config user.email 'release-test@example.invalid'
git config commit.gpgsign false
git config tag.gpgsign false
git config core.autocrlf false
mkdir -p scripts resources .github/workflows
cp "$root/scripts/"{release.sh,release-ci.sh,lib.sh,set-version.sh} scripts/
cp "$root/.github/workflows/ci.yml" .github/workflows/
printf '[package]\nname = "mdoc"\nversion = "0.1.0"\nedition = "2021"\n' >Cargo.toml
printf '[[package]]\nname = "mdoc"\nversion = "0.1.0"\n' >Cargo.lock
printf '<key>CFBundleVersion</key>\n<string>0.1.0</string>\n<key>CFBundleShortVersionString</key>\n<string>0.1.0</string>\n' >resources/Info.plist
printf '## Unreleased\n\n- Test release.\n' >CHANGELOG.md
printf 'base\n' >code.rs
commit base
source scripts/release-ci.sh
GH_RUN_SHA=$(git rev-parse HEAD)
green
echo 'ok: green CI'

GH_RESULT=failed
local_checks 'conclusion=failure'
GH_RESULT=pending
local_checks 'status=in_progress'
GH_RESULT=missing
local_checks 'no CI push run'
GH_RESULT=error
local_checks 'authentication and connectivity'
GH_RESULT=wrong-event
local_checks 'not clearly green'
GH_RESULT=green
echo 'ok: failed, pending, absent, unauthenticated and wrong-event runs fall back'

# Exercise every ignored path, including root and nested Markdown.
mkdir -p docs build nested .github/workflows
for path in README.md nested/readme.md LICENSE .gitignore build/appicon.svg \
  docs/plain.txt .github/workflows/actionlint.yml .github/dependabot.yml; do
  printf 'ignored\n' >"$path"
done
commit ignored
green
echo 'ok: only-ignored later commits reuse the relevant green run'
printf 'changed\n' >>code.rs
commit relevant
local_checks 'no CI push run'
GH_RUN_SHA=$(git rev-parse HEAD)
GH_RESULT=failed
local_checks 'conclusion=failure'
GH_RESULT=green
echo 'ok: later relevant commit never reuses older green CI'

git switch --quiet -c side
printf 'side\n' >>code.rs
git add code.rs
git commit --quiet -m side
git switch --quiet main
git merge --quiet --no-ff side -m merge
git update-ref refs/remotes/origin/main HEAD
local_checks 'no CI push run'
GH_RUN_SHA=$(git rev-parse HEAD)
green
echo 'ok: merge changes are CI-relevant'

# ci.yml skips the release commit on main; its tag's Release run counts.
printf '[package]\nname = "mdoc"\nversion = "0.1.1"\nedition = "2021"\n' >Cargo.toml
commit 'release: v0.1.1'
GH_RUN_SHA=$(git rev-parse HEAD)
GH_RESULT=skipped
GH_RELEASE_TAG=v0.1.1
local_checks 'tag v0.1.1 does not point at it'
git tag -a v0.1.1 -m 'mdoc v0.1.1'
GH_RELEASE_RESULT=missing
local_checks 'no Release run for v0.1.1'
GH_RELEASE_RESULT=pending
local_checks 'status=in_progress'
GH_RELEASE_RESULT=failed
local_checks 'conclusion=failure'
GH_RELEASE_RESULT=green
release_green() {
  release_ci_green >"$tmp/result" || fail "expected green: $(cat "$tmp/result")"
  contains "$tmp/result" "$GH_RUN_SHA is release commit v0.1.1"
  contains "$tmp/result" 'https://github.com/example/mdoc/actions/runs/43'
}
release_green
printf 'docs\n' >docs/after-release.md
commit 'docs after release'
release_green
printf 'x\n' >>code.rs
commit 'fix skipped by CI'
GH_RUN_SHA=$(git rev-parse HEAD)
local_checks 'was skipped'
GH_RESULT=green GH_RELEASE_RESULT=missing GH_RELEASE_TAG=none
echo 'ok: a release commit skipped by CI uses its tag'"'"'s Release run'

mkdir "$tmp/empty"
# shellcheck disable=SC2123 # an empty PATH is the point
(
  PATH="$tmp/empty"
  if release_ci_green >"$tmp/result"; then fail 'unexpected green without gh'; fi
)
contains "$tmp/result" 'gh is unavailable'
echo 'ok: gh unavailable'

# A malformed ignore list cannot silently accept a partial list.
awk '/^    paths-ignore: &ignored$/ { print; print "      - unquoted"; next } { print }' \
  "$root/.github/workflows/ci.yml" >.github/workflows/ci.yml
local_checks 'cannot read push paths-ignore'
cp "$root/.github/workflows/ci.yml" .github/workflows/ci.yml

# Count prompts even with EOF: simply piping /dev/null could hide a prompt.
cat >"$tmp/bash-env" <<'EOF'
read() {
  local arg
  for arg in "$@"; do
    if [ "$arg" = -p ]; then echo prompt >> "$PROMPT_LOG"; fi
  done
  builtin read "$@"
}
EOF
export BASH_ENV="$tmp/bash-env"
n=0
release_case() {
  n=$((n + 1))
  git -c core.autocrlf=false clone --quiet "$tmp/repo" "$tmp/release-$n"
  cd "$tmp/release-$n"
  git config user.name 'Release script test'
  git config user.email 'release-test@example.invalid'
  git config commit.gpgsign false
  git config tag.gpgsign false
  git config core.autocrlf false
  rm -f "$CHECK_LOG" "$PUSH_LOG" "$PROMPT_LOG"
  printf '%s' "${ANSWERS:-}" >"$tmp/answers"
  bash scripts/release.sh 0.2.0-rc.3 "$@" >"$tmp/release-output" <"$tmp/answers"
  [ "$(git log -1 --format=%s)" = 'release: v0.2.0-rc.3' ] || fail 'release commit missing'
  [ "$(git rev-parse 'v0.2.0-rc.3^{commit}')" = "$(git rev-parse HEAD)" ] || fail 'tag missing'
  [ -z "$(git status --porcelain)" ] || fail 'dirty release tree'
  contains Cargo.toml 'version = "0.2.0-rc.3"'
  contains Cargo.lock 'version = "0.2.0-rc.3"'
  contains resources/Info.plist '<string>0.2.0-rc.3</string>'
  if [ "${EXPECTED_PROMPTS:-0}" = 0 ]; then
    [ ! -e "$PROMPT_LOG" ] || fail 'unexpected prompt'
  else
    [ "$(wc -l <"$PROMPT_LOG" | tr -d '[:space:]')" = "$EXPECTED_PROMPTS" ] || fail 'missing interactive prompts'
  fi
}
release_case --yes --skip-checks-if-ci-green --no-push
[ ! -e "$CHECK_LOG" ] || fail 'green CI ran checks'
[ ! -e "$PUSH_LOG" ] || fail '--no-push pushed'
contains "$tmp/release-output" 'skipping local checks'
echo 'ok: exact non-interactive acceptance command creates commit/tag, no checks or push'
release_case --no-push --check-all --yes
contains "$CHECK_LOG" check-all
[ ! -e "$PUSH_LOG" ] || fail '--no-push lost to --yes'
GH_RESULT=failed
release_case --skip-checks-if-ci-green --yes --no-push
[ "$(cat "$CHECK_LOG")" = check ] || fail 'fallback should use check'
release_case --yes
[ "$(cat "$CHECK_LOG")" = check ] || fail '--yes should default to check'
contains "$PUSH_LOG" 'push --atomic origin main v0.2.0-rc.3'
echo 'ok: flag order, check-all, fallback, default check and automatic push selection (intercepted)'
ANSWERS=$'y\nn\n' EXPECTED_PROMPTS=2 release_case
contains "$CHECK_LOG" check-all
[ ! -e "$PUSH_LOG" ] || fail 'interactive no pushed'
echo 'ok: unflagged release retains both interactive prompts'

cd "$tmp/repo"
rm -f "$CHECK_LOG" "$PUSH_LOG" "$PROMPT_LOG"
bash scripts/release.sh --help >"$tmp/help"
for flag in --yes --check-all --skip-checks-if-ci-green --no-push --help; do
  contains "$tmp/help" "$flag"
done
if bash scripts/release.sh 0.2.0-rc.3 --typo >"$tmp/error" 2>&1; then fail 'unknown flag accepted'; fi
contains "$tmp/error" 'unknown flag: --typo'
[[ ! -e $CHECK_LOG && ! -e $PUSH_LOG ]] || fail 'help/invalid flags had side effects'
echo 'ok: help and unknown flags'
echo 'release script tests passed'
