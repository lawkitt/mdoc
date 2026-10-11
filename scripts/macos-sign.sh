#!/usr/bin/env bash
# Usage: scripts/macos-sign.sh app <mdoc.app> | dmg <file.dmg>
# Developer ID signing and notarization for the macOS release (ADR 0039).
#   app  signs the bundle with the hardened runtime and the entitlements, then
#        notarizes and staples it, so a copy outside the .dmg opens offline.
#   dmg  signs, notarizes and staples the disk image.
# Environment:
#   APPLE_SIGNING_IDENTITY  "Developer ID Application: <name> (<team id>)", in
#                           the keychain
#   Notarization, either:
#     APPLE_KEYCHAIN_PROFILE  a `xcrun notarytool store-credentials` profile
#   or the Apple ID with an app-specific password (CI):
#     APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID
# shellcheck source=lib.sh
source "$(dirname "$0")/lib.sh"

[ $# -eq 2 ] || die "usage: scripts/macos-sign.sh app|dmg <path>"
kind=$1
target=$2
[ -e "$target" ] || die "$target not found"
: "${APPLE_SIGNING_IDENTITY:?set APPLE_SIGNING_IDENTITY to the Developer ID Application identity}"

if [ -n "${APPLE_KEYCHAIN_PROFILE:-}" ]; then
  notary_auth=(--keychain-profile "$APPLE_KEYCHAIN_PROFILE")
elif [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
  notary_auth=(--apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID")
else
  die "notarization needs APPLE_KEYCHAIN_PROFILE, or APPLE_ID, APPLE_PASSWORD and APPLE_TEAM_ID"
fi

# Submits a .zip or .dmg and waits; on rejection prints Apple's log, which
# names the offending file.
notarize() {
  local file=$1 result id
  echo "notarizing $(basename "$file") (usually a few minutes)"
  result=$(xcrun notarytool submit "$file" "${notary_auth[@]}" --wait --output-format json)
  id=$(plutil -extract id raw - <<<"$result")
  if [ "$(plutil -extract status raw - <<<"$result")" != Accepted ]; then
    xcrun notarytool log "$id" "${notary_auth[@]}" >&2 || true
    die "notarization of $(basename "$file") was not accepted (submission $id)"
  fi
}

case "$kind" in
  app)
    # The bundle holds one executable, so signing the bundle covers all code.
    codesign --force --timestamp --options runtime \
      --entitlements resources/entitlements.plist \
      --sign "$APPLE_SIGNING_IDENTITY" "$target"
    codesign --verify --strict --verbose=2 "$target"
    zip="$(mktemp -d)/mdoc.zip"
    ditto -c -k --keepParent "$target" "$zip"
    notarize "$zip"
    xcrun stapler staple "$target"
    ;;
  dmg)
    codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$target"
    notarize "$target"
    xcrun stapler staple "$target"
    ;;
  *) die "unknown kind $kind; use app or dmg" ;;
esac
