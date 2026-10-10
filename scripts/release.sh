#!/usr/bin/env bash
# Usage: scripts/release.sh <version> [--no-push]
# Makes the release commit and tag (ADR 0037): checks main is clean and
# current, runs the checks, sets the version, names the CHANGELOG section for
# stable versions, commits `release: v<version>`, tags, and pushes on y/N.
source "$(dirname "$0")/lib.sh"

new=${1:?usage: release.sh <version> [--no-push]}
new=${new#v}
push=1
[ "${2:-}" = "--no-push" ] && push=0
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

if confirm "Also run the model and OCR smoke tests (just check-all, downloads models)?"; then
  just check-all
else
  just check
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
  if confirm "Push main and $tag to origin (starts the release build)?"; then
    git push --atomic origin main "$tag"
    echo "pushed; the draft release appears at https://github.com/lawkitt/mdoc/releases once the build finishes"
    exit 0
  fi
fi
echo "not pushed; when ready: git push --atomic origin main $tag"
