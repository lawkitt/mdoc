#!/usr/bin/env bash
# Usage: scripts/smoke-update.sh [path/to/mdoc_<version>_arm64.dmg]
# In-app update check on real artifacts (ADR 0040), run by hand before a
# release that changes the updater. Copies the app out of a Developer ID
# signed .dmg OLDER than the latest published release into a throwaway
# Applications folder, runs `mdoc --mdoc-self-update` there (the app's own
# check, download, SHA256SUMS, staging, Developer ID gates and swap), then
# checks that the swapped app is the latest release and passes Gatekeeper.
#
# An older signed .dmg: a `workflow_dispatch` Release run with version
# 0.0.0-dev and **sign** ticked, or locally `scripts/set-version.sh 0.0.0-dev`
# then `just package` with the signing identity (revert the version after).
# shellcheck source=lib.sh
source "$(dirname "$0")/lib.sh"

[ "$(platform)" = macos-arm64 ] || die "smoke-update runs on Apple Silicon macOS only"
dmg=${1:-dist/mdoc_$(version)_arm64.dmg}
[ -f "$dmg" ] || die "$dmg not found; pass a signed .dmg older than the latest release"

work=$(mktemp -d)
mount="$work/mount"
mkdir -p "$mount" "$work/Applications"
trap 'hdiutil detach -quiet "$mount" 2>/dev/null || true; rm -rf "$work"' EXIT
hdiutil attach -nobrowse -readonly -mountpoint "$mount" "$dmg" >/dev/null
ditto "$mount/mdoc.app" "$work/Applications/mdoc.app"
hdiutil detach -quiet "$mount"

app="$work/Applications/mdoc.app"
plist_version() { plutil -extract CFBundleShortVersionString raw -o - "$1/Contents/Info.plist"; }
before=$(plist_version "$app")
echo "installed copy: mdoc $before"

"$app/Contents/MacOS/mdoc" --mdoc-self-update

after=$(plist_version "$app")
[ "$after" != "$before" ] || die "mdoc $before did not update; is it older than the latest release?"
codesign --verify --deep --strict \
  -R '=anchor apple generic and identifier "io.github.lawkitt.mdoc" and certificate leaf[subject.OU] = "F33DLM8K53"' \
  "$app"
spctl --assess --type execute -v "$app"
for leftover in "$work/Applications/.mdoc.app.update" "$work/Applications/.mdoc.app.old"; do
  [ ! -e "$leftover" ] || die "staging leftover beside the app: $leftover"
done
echo "smoke-update passed: mdoc $before -> $after"
