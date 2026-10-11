#!/usr/bin/env bash
# Usage: scripts/ci-apple-credentials.sh setup | cleanup
# GitHub Actions only (release.yml, signed macOS builds; ADR 0039). `setup`
# imports the Developer ID certificate into a throwaway keychain for
# scripts/macos-sign.sh; `cleanup` deletes it.
# Environment (secrets of the `release` environment):
#   APPLE_CERTIFICATE           base64 of the Developer ID Application .p12
#   APPLE_CERTIFICATE_PASSWORD  its export password
# shellcheck source=lib.sh
source "$(dirname "$0")/lib.sh"

: "${RUNNER_TEMP:?not running in GitHub Actions}"
keychain="$RUNNER_TEMP/mdoc-signing.keychain-db"

case "${1:-}" in
  setup)
    : "${APPLE_CERTIFICATE:?missing secret APPLE_CERTIFICATE}"
    : "${APPLE_CERTIFICATE_PASSWORD:?missing secret APPLE_CERTIFICATE_PASSWORD}"
    keychain_password=$(openssl rand -hex 24)
    p12="$RUNNER_TEMP/developer-id.p12"
    base64 --decode <<<"$APPLE_CERTIFICATE" >"$p12"
    security create-keychain -p "$keychain_password" "$keychain"
    security set-keychain-settings -lut 3600 "$keychain"
    security unlock-keychain -p "$keychain_password" "$keychain"
    security import "$p12" -k "$keychain" -P "$APPLE_CERTIFICATE_PASSWORD" \
      -T /usr/bin/codesign -f pkcs12
    rm -f "$p12"
    # Let codesign use the key without a UI prompt.
    security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
      -k "$keychain_password" "$keychain" >/dev/null
    # Search this keychain as well as the login keychain.
    # shellcheck disable=SC2046 # one path per line, none with spaces
    security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')
    security find-identity -v -p codesigning "$keychain"
    ;;
  cleanup)
    security delete-keychain "$keychain" 2>/dev/null || true
    ;;
  *) die "usage: scripts/ci-apple-credentials.sh setup|cleanup" ;;
esac
