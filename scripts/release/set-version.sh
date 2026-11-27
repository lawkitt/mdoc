#!/usr/bin/env bash
# Usage: scripts/release/set-version.sh <version>
# Writes <version> (no leading "v") into Cargo.toml, Cargo.lock and
# apps/desktop/resources/Info.plist. Used by `just release` and by manual-dispatch builds.
source "$(dirname "$0")/../lib.sh"

new=${1:?usage: set-version.sh <version>}
new=${new#v}
is_semver "$new" || die "not a SemVer version: $new"

awk -v v="$new" '
  /^\[/ { in_pkg = ($0 == "[workspace.package]") }
  in_pkg && /^version = / && !done { print "version = \"" v "\""; done = 1; next }
  { print }
' Cargo.toml >Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml

# The <string> following CFBundleShortVersionString / CFBundleVersion.
awk -v v="$new" '
  pending { sub(/<string>[^<]*<\/string>/, "<string>" v "</string>"); pending = 0 }
  /<key>CFBundle(ShortVersionString|Version)<\/key>/ { pending = 1 }
  { print }
' apps/desktop/resources/Info.plist >apps/desktop/resources/Info.plist.tmp && mv apps/desktop/resources/Info.plist.tmp apps/desktop/resources/Info.plist

cargo update --workspace --quiet

[ "$(version)" = "$new" ] || die "Cargo.toml still reports $(version)"
echo "version set to $new"
