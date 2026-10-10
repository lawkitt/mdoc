#!/usr/bin/env bash
# Usage: scripts/package.sh
# Builds this machine's release artifacts into dist/ (ADR 0037):
#   macOS arm64  mdoc_<version>_arm64.dmg
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
    cargo packager --release -f app -f dmg
    dmg=$(find target/release -maxdepth 1 -name 'mdoc_*.dmg' | head -1)
    [ -n "$dmg" ] || die "cargo-packager produced no .dmg in target/release"
    mv "$dmg" "dist/mdoc_${v}_arm64.dmg"
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
