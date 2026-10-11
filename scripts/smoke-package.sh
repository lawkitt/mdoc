#!/usr/bin/env bash
# Usage: scripts/smoke-package.sh
# Release gate on what ships (ADR 0037): runs `mdoc --mdoc-self-test` from the
# packaged artifact after `just package`. On macOS that is the app inside the
# .dmg, under its real hardened-runtime signature, which test builds never
# have; on Windows the release mdoc.exe the installer carries. The self-test
# downloads the OCR runtime and model into throwaway storage and recognizes
# the English qualification fixture. A Developer ID build must also pass
# Gatekeeper with stapled tickets (ADR 0039); MDOC_REQUIRE_SIGNING=1 refuses
# any other build.
# shellcheck source=lib.sh
source "$(dirname "$0")/lib.sh"
require_shipped_target "smoke-package"

v=$(version)
fixtures=tests/fixtures/ocr-qualification
args=(--mdoc-self-test "$PWD/$fixtures/english.pdf" "$PWD/$fixtures/english.txt")

case "$(platform)" in
  macos-arm64)
    dmg="dist/mdoc_${v}_arm64.dmg"
    [ -f "$dmg" ] || die "$dmg not found; run \`just package\` first"
    mount=$(mktemp -d)
    hdiutil attach -nobrowse -readonly -mountpoint "$mount" "$dmg" >/dev/null
    trap 'hdiutil detach -quiet "$mount" || true' EXIT
    app="$mount/mdoc.app"
    # Fail loudly if the shipped app ever stops being hardened: the gate
    # exists to test that signature.
    # Captured first: `grep -q` stops reading early, and under pipefail the
    # cut-off codesign would fail the check.
    signature=$(codesign -dv "$app" 2>&1)
    [[ $signature == *"flags="*runtime* ]] ||
      die "the packaged app is not signed with the hardened runtime"
    # Release builds must pass Gatekeeper as a user's Mac would see them:
    # Developer ID signed, notarized, with the ticket stapled to both.
    if [[ $signature == *"Authority=Developer ID Application"* ]]; then
      xcrun stapler validate "$dmg"
      xcrun stapler validate "$app"
      spctl --assess --type open --context context:primary-signature -v "$dmg"
      spctl --assess --type execute -v "$app"
    elif [ -n "${MDOC_REQUIRE_SIGNING:-}" ]; then
      die "the packaged app is not signed with a Developer ID"
    fi
    "$app/Contents/MacOS/mdoc" "${args[@]}"
    ;;
  windows-x64)
    exe=target/release/mdoc.exe
    [ -f "$exe" ] || die "$exe not found; run \`just package\` first"
    "$exe" "${args[@]}"
    ;;
esac
