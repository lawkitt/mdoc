#!/usr/bin/env bash
# Usage: scripts/package.sh
# Builds this machine's release artifacts into dist/ (ADR 0037):
#   macOS arm64  mdoc_<version>_arm64.dmg, Developer ID signed and notarized
#                when APPLE_SIGNING_IDENTITY is set (scripts/macos-sign.sh);
#                MDOC_REQUIRE_SIGNING=1 refuses an unsigned build
#   Windows x64  mdoc_<version>_x64-setup.exe, plus mdoc_<version>_x64.msi for
#                stable versions (MSI ProductVersion rejects pre-release tags)
source "$(dirname "$0")/lib.sh"
require_shipped_target "packaging"

v=$(version)
rm -rf dist
mkdir -p dist

# Fetch dependencies first: cargo-packager runs `cargo metadata` for config
# discovery and reports any failure there as a missing configuration.
cargo fetch --locked

case "$(platform)" in
  macos-arm64)
    # The .app only: cargo-packager would sign a .dmg ad hoc, and a .dmg it
    # builds would repack the ad-hoc .app; both are done below instead.
    cargo packager --release -f app
    app=target/release/mdoc.app
    if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
      scripts/macos-sign.sh app "$app"
    elif [ -n "${MDOC_REQUIRE_SIGNING:-}" ]; then
      die "MDOC_REQUIRE_SIGNING is set but APPLE_SIGNING_IDENTITY is not"
    else
      echo "APPLE_SIGNING_IDENTITY not set: ad-hoc signed, not notarized"
    fi
    # Without this entitlement the hardened runtime refuses the downloaded
    # PDFium and ONNX Runtime libraries, and OCR/pseudonymization break only
    # in packaged builds.
    entitlements=$(codesign -d --entitlements - --xml "$app" 2>/dev/null)
    [[ $entitlements == *com.apple.security.cs.disable-library-validation* ]] ||
      die "mdoc.app is missing the disable-library-validation entitlement"
    dmg="dist/mdoc_${v}_arm64.dmg"
    stage=$(mktemp -d)
    ditto "$app" "$stage/mdoc.app"
    ln -s /Applications "$stage/Applications"
    hdiutil create -quiet -volname mdoc -srcfolder "$stage" -fs HFS+ -format UDZO -ov "$dmg"
    rm -rf "$stage"
    if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
      scripts/macos-sign.sh dmg "$dmg"
    fi
    ;;
  windows-x64)
    redist=$(scripts/vc-redist.sh)
    # Bundled by the NSIS preinstall section in Cargo.toml (MDOC_VC_REDIST);
    # absolute, because makensis resolves paths from its own directory.
    MDOC_VC_REDIST=$(cygpath -wa "$redist") cargo packager --release -f nsis
    exe=$(find target/release -maxdepth 1 -name 'mdoc_*-setup.exe' | head -1)
    [ -n "$exe" ] || die "cargo-packager produced no installer in target/release"
    mv "$exe" "dist/mdoc_${v}_x64-setup.exe"
    if [[ $v != *-* ]]; then
      rm -rf target/wix
      cargo wix -p mdoc --no-build --nocapture
      msi=$(find target/wix -maxdepth 1 -name '*.msi' | head -1)
      [ -n "$msi" ] || die "cargo-wix produced no .msi in target/wix"
      mv "$msi" "dist/mdoc_${v}_x64.msi"
    fi
    ;;
esac

echo "--- dist ---"
ls -l dist
