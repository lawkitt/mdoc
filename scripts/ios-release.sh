#!/usr/bin/env bash
# Usage: scripts/ios-release.sh <x.y.z>-rc.<n> <what-to-test file> [--commit <sha>]
#        [--yes] [--no-push]
#        scripts/ios-release.sh --check-tag <tag>
#        scripts/ios-release.sh --check-main
# Tags an iOS TestFlight build (ADR 0107): an annotated `ios-v<x.y.z>-rc.<n>`
# on a `main` commit (HEAD, or --commit) that passed the iPhone 12 Pro gate
# (`just ios-device-test`, the below-floor tier, which records the pass in
# target/ios/gate/<commit>). No commit is made. The annotation's body is
# What to Test (scripts/ios-what-to-test.sh checks it); below a `---` line,
# trailers name the build and the gate. Pushing the tag starts
# ios-release.yml, which uploads the build. Refuses a commit not on
# origin/main, an `x.y.z` other than its MARKETING_VERSION (a new version is
# the maintainer's), an export key other than NO, and a tag that exists.
#   --commit <sha>  Tag this commit instead of HEAD.
#   --yes           Push without asking.
#   --no-push       Make the tag only (wins over --yes).
# ios-release.yml's checks, which print version=, build= and commit= lines:
#   --check-tag <tag>  A pushed tag: annotated, on origin/main, matching its
#                      commit's version and export key, with a valid What
#                      to Test.
#   --check-main       HEAD for a validate run: on origin/main, with the
#                      export key NO.
# Bash 3.2.
source "$(dirname "$0")/lib.sh"

usage="usage: scripts/ios-release.sh <x.y.z>-rc.<n> <what-to-test file> [--commit <sha>] [--yes] [--no-push] | --check-tag <tag> | --check-main"
gates=${MDOC_GATE_DIR:-target/ios/gate}
project=apps/ios/mdoc.xcodeproj/project.pbxproj
plist=apps/ios/Config/mdoc-Info.plist

# marketing_version <sha>: the one MARKETING_VERSION the project has there.
marketing_version() {
  local versions
  versions=$(git show "$1:$project" | sed -n 's/^[[:space:]]*MARKETING_VERSION = \(.*\);$/\1/p' | sort -u)
  [ -n "$versions" ] && [ "$(echo "$versions" | wc -l | tr -d ' ')" = 1 ] ||
    die "MARKETING_VERSION at $1 is not one value: $(echo "$versions" | tr '\n' ' ')"
  echo "$versions"
}

# check_commit <sha>: on origin/main, with the export key NO.
check_commit() {
  git fetch --quiet origin main
  git merge-base --is-ancestor "$1" origin/main || die "$1 is not on origin/main"
  git show "$1:$plist" | tr -d '\t\n ' | grep -q '<key>ITSAppUsesNonExemptEncryption</key><false/>' ||
    die "$plist at $1 does not declare ITSAppUsesNonExemptEncryption NO (ADR 0097)"
}

# split_version <x.y.z>-rc.<n>: sets marketing and rc.
split_version() {
  [[ $1 =~ ^([0-9]+\.[0-9]+\.[0-9]+)-rc\.([1-9][0-9]*)$ ]] ||
    die "the version must be <x.y.z>-rc.<n>, as 0.2.0-rc.3 (the plain version is the maintainer's)"
  marketing=${BASH_REMATCH[1]}
  rc=${BASH_REMATCH[2]}
}

check_tag() {
  local tag=$1 sha notes current
  [ "$(git cat-file -t "refs/tags/$tag" 2>/dev/null)" = tag ] || die "$tag is not an annotated tag here"
  split_version "${tag#ios-v}"
  [ "ios-v$marketing-rc.$rc" = "$tag" ] || die "$tag is not an ios-v<x.y.z>-rc.<n> tag"
  sha=$(git rev-parse "refs/tags/$tag^{commit}")
  check_commit "$sha"
  current=$(marketing_version "$sha")
  [ "$current" = "$marketing" ] || die "$tag's version is not MARKETING_VERSION ($current) at $sha"
  notes=$(mktemp)
  bash scripts/ios-what-to-test.sh from-tag "$tag" >"$notes"
  bash scripts/ios-what-to-test.sh check "$notes"
  rm -f "$notes"
  echo "version=$marketing"
  echo "build=$(git rev-list --count "$sha")"
  echo "commit=$sha"
}

check_main() {
  local sha current
  sha=$(git rev-parse HEAD)
  check_commit "$sha"
  current=$(marketing_version "$sha")
  echo "version=$current"
  echo "build=$(git rev-list --count "$sha")"
  echo "commit=$sha"
}

case "${1:-}" in
  --check-tag)
    check_tag "${2:?$usage}"
    exit 0
    ;;
  --check-main)
    check_main
    exit 0
    ;;
esac

version=
notes=
commit=HEAD
yes=0
push=1
while [ $# -gt 0 ]; do
  case "$1" in
    --commit)
      commit=${2:?$usage}
      shift
      ;;
    --yes) yes=1 ;;
    --no-push) push=0 ;;
    -h | --help)
      sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    -*) die "$usage" ;;
    *)
      if [ -z "$version" ]; then
        version=$1
      elif [ -z "$notes" ]; then
        notes=$1
      else
        die "$usage"
      fi
      ;;
  esac
  shift
done
[ -n "$version" ] && [ -n "$notes" ] || die "$usage"
split_version "$version"
tag=ios-v$version

bash scripts/ios-what-to-test.sh check "$notes"
sha=$(git rev-parse --verify "$commit^{commit}") || die "no commit $commit"
check_commit "$sha"
current=$(marketing_version "$sha")
[ "$current" = "$marketing" ] ||
  die "MARKETING_VERSION at $sha is $current, not $marketing; a new version is the maintainer's"

git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null && die "tag $tag exists here; tags are never reused"
[ -z "$(git ls-remote --tags origin "refs/tags/$tag")" ] || die "tag $tag exists on origin; tags are never reused"

# The gate: a below-floor pass recorded for exactly this commit.
gate=$gates/$sha
[ -f "$gate" ] || die "no gate record for $sha: run \`just ios-device-test <12 Pro UDID>\` on it first"
grep -qx 'tier=below-floor' "$gate" || die "the gate record for $sha is not the below-floor tier"
grep -qx "commit=$sha" "$gate" || die "the gate record $gate names another commit"
phone=$(sed -n 's/^phone=//p' "$gate")
passed=$(sed -n 's/^passed=//p' "$gate")

build=$(git rev-list --count "$sha")
message=$(mktemp)
trap 'rm -f "$message"' EXIT
{
  echo "mdoc for iOS $marketing rc.$rc (build $build)"
  echo
  perl -0777 -pe 's/\s+\z/\n/' "$notes"
  echo
  echo "---"
  echo "Build: $build"
  echo "Commit: $sha"
  echo "Gate: just ios-device-test, below-floor tier, on $phone, passed $passed"
} >"$message"

git tag --annotate --cleanup=verbatim --file="$message" "$tag" "$sha"
echo "tagged $tag on $sha (build $build)"

if [ "$push" = 1 ]; then
  if [ "$yes" = 1 ] || confirm "Push $tag to origin (starts the TestFlight upload)?"; then
    git push origin "refs/tags/$tag"
    echo "pushed; ios-release.yml builds and uploads it: https://github.com/lawkitt/mdoc/actions/workflows/ios-release.yml"
    exit 0
  fi
fi
echo "not pushed; when ready: git push origin refs/tags/$tag"
