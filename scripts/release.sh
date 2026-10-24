#!/usr/bin/env bash
# Usage: scripts/release.sh <version> [--yes] [--check-all]
#        [--skip-checks-if-ci-green] [--no-push] [--help]
# Makes the release commit and tag (ADR 0037): checks main is clean and
# current, runs the checks, sets the version, names the CHANGELOG section for
# stable versions, commits `release: v<version>`, tags, and pushes on y/N.
source "$(dirname "$0")/lib.sh"
source scripts/release-ci.sh  # lib.sh has moved to the repository root

usage() {
  cat <<'EOF'
Usage: scripts/release.sh <version> [flags]
  --yes                      Do not prompt; use check and confirm the push
  --check-all                Run check-all instead of offering the smoke tests
  --skip-checks-if-ci-green  Skip local checks when origin/main has green CI
  --no-push                  Create the commit and tag without pushing (wins over --yes)
  --help                     Show this help
Flags may appear in any order after the version. --yes uses check unless
--check-all is given. Unclear CI status always falls back to local checks.
EOF
}

[ "$#" -gt 0 ] || { usage >&2; exit 1; }
[ "$1" != --help ] || { usage; exit 0; }
new=$1
shift
new=${new#v}
push=1
yes=0
check_all=0
skip_checks=0
for flag in "$@"; do
  case "$flag" in
    --yes) yes=1 ;;
    --check-all) check_all=1 ;;
    --skip-checks-if-ci-green) skip_checks=1 ;;
    --no-push) push=0 ;;
    --help) usage; exit 0 ;;
    *) die "unknown flag: $flag (see --help)" ;;
  esac
done
is_semver "$new" || die "not a SemVer version: $new"
tag="v$new"

[ "$(git rev-parse --abbrev-ref HEAD)" = main ] || die "releases are made from main"
[ -z "$(git status --porcelain)" ] || die "the working tree has uncommitted changes"
git fetch --quiet --tags origin main
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || die "main differs from origin/main; pull or push first"
! git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "tag $tag already exists"
[ "$new" != "$(version)" ] || die "the repository is already at $new"

stable=1
[[ $new == *-* ]] && stable=0
if [ "$stable" = 1 ]; then
  # Unreleased must have content to become the release's highlights.
  body=$(awk '/^## Unreleased/ { grab = 1; next } grab && /^## / { exit } grab' CHANGELOG.md | tr -d '[:space:]')
  [ -n "$body" ] || die "CHANGELOG.md has no Unreleased entries for $new"
fi

if [ "$skip_checks" = 1 ] && release_ci_green; then
  # The tag's Release workflow still runs full CI before packaging.
  [ "$check_all" = 0 ] || echo "--check-all not run: CI is green; the Release run adds the OCR smoke test"
else
  if [ "$check_all" = 1 ] || { [ "$yes" = 0 ] && confirm "Also run the model and OCR smoke tests (just check-all, downloads models)?"; }; then
    just check-all
  else
    just check
  fi
fi

scripts/set-version.sh "$new"
if [ "$stable" = 1 ]; then
  today=$(date +%Y-%m-%d)
  awk -v head="## [$new] - $today" '
    !done && /^## Unreleased/ { print; print ""; print head; done = 1; next }
    { print }
  ' CHANGELOG.md > CHANGELOG.md.tmp && mv CHANGELOG.md.tmp CHANGELOG.md
fi

git add Cargo.toml Cargo.lock resources/Info.plist CHANGELOG.md
git commit --quiet -m "release: $tag"
git tag -a "$tag" -m "mdoc $tag"
echo "created release commit and tag $tag"

if [ "$push" = 1 ]; then
  if [ "$yes" = 1 ] || confirm "Push main and $tag to origin (starts the release build)?"; then
    git push --atomic origin main "$tag"
    echo "pushed; the draft release appears at https://github.com/lawkitt/mdoc/releases once the build finishes"
    exit 0
  fi
fi
echo "not pushed; when ready: git push --atomic origin main $tag"
