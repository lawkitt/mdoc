#!/usr/bin/env bash
# Usage: scripts/verify-version.sh <tag>
# Fails unless <tag> is v<version> for the version in Cargo.toml and
# resources/Info.plist. The release workflow runs this before building.
source "$(dirname "$0")/lib.sh"

tag=${1:?usage: verify-version.sh <tag>}
expected="v$(version)"
[ "$tag" = "$expected" ] || die "tag $tag does not match Cargo.toml ($expected); make releases with \`just release\`"

for key in CFBundleShortVersionString CFBundleVersion; do
  plist=$(awk -v k="<key>$key</key>" 'found { gsub(/.*<string>|<\/string>.*/, ""); print; exit } index($0, k) { found = 1 }' resources/Info.plist)
  [ "v$plist" = "$expected" ] || die "Info.plist $key is $plist, expected ${expected#v}"
done

lock=$(awk '/^name = "mdoc"$/ { getline; gsub(/^version = "|"$/, ""); print; exit }' Cargo.lock)
[ "v$lock" = "$expected" ] || die "Cargo.lock has mdoc $lock, expected ${expected#v}"

echo "$tag matches the repository version"
