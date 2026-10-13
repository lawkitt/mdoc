#!/usr/bin/env bash
# Usage: scripts/macos-sign.sh app <mdoc.app> | dmg <file.dmg>
# Developer ID signing and notarization for the macOS release (ADR 0039).
#   app  signs the bundle with the hardened runtime and the entitlements, then
#        notarizes and staples it, so a copy outside the .dmg opens offline.
#   dmg  signs, notarizes and staples the disk image.
# Environment:
#   APPLE_SIGNING_IDENTITY  "Developer ID Application: <name> (<team id>)", in
#                           the keychain
#   APPLE_PROVISIONING_PROFILE  optional: a Developer ID provisioning profile for
#                           io.github.lawkitt.mdoc with Keychain sharing. Embedded
#                           with the Keychain access-group entitlements, so the
#                           case key uses the data protection keychain (ADR 0045).
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
    entitlements=resources/entitlements.plist
    if [ -n "${APPLE_PROVISIONING_PROFILE:-}" ]; then
      [ -f "$APPLE_PROVISIONING_PROFILE" ] || die "$APPLE_PROVISIONING_PROFILE not found"
      team=$(sed -n 's/.*(\([A-Z0-9]\{10\}\))$/\1/p' <<<"$APPLE_SIGNING_IDENTITY")
      [ -n "$team" ] || die "no team id in APPLE_SIGNING_IDENTITY"
      app_id="$team.io.github.lawkitt.mdoc"
      profile=$(security cms -D -i "$APPLE_PROVISIONING_PROFILE")
      [ "$(/usr/libexec/PlistBuddy -c "Print :Entitlements:com.apple.application-identifier" /dev/stdin <<<"$profile")" = "$app_id" ] ||
        die "the provisioning profile is not for $app_id"
      cp "$APPLE_PROVISIONING_PROFILE" "$target/Contents/embedded.provisionprofile"
      entitlements="$(mktemp -d)/entitlements.plist"
      cp resources/entitlements.plist "$entitlements"
      # PlistBuddy, not plutil: these keys contain dots.
      for add in "com.apple.application-identifier string $app_id" \
        "com.apple.developer.team-identifier string $team" \
        "keychain-access-groups array" "keychain-access-groups:0 string $app_id"; do
        /usr/libexec/PlistBuddy -c "Add :$add" "$entitlements" >/dev/null
      done
      echo "embedding the provisioning profile: case key in the data protection keychain"
    else
      echo "no APPLE_PROVISIONING_PROFILE: case key in the login keychain"
    fi
    # The bundle holds one executable, so signing the bundle covers all code.
    codesign --force --timestamp --options runtime \
      --entitlements "$entitlements" \
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
