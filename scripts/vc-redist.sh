#!/usr/bin/env bash
# Usage: scripts/vc-redist.sh
# Downloads the x64 Visual C++ Redistributable bundled into the NSIS installer
# and prints its path. Microsoft's link always serves the latest build, so the
# file is checked by its Authenticode signature rather than a pinned hash.
source "$(dirname "$0")/lib.sh"
[ "$(platform)" = windows-x64 ] || die "the VC++ Redistributable is only bundled on Windows x64"

url="https://aka.ms/vs/17/release/vc_redist.x64.exe"
out="target/vc-redist/vc_redist.x64.exe"
mkdir -p "$(dirname "$out")"
curl --fail --silent --show-error --location --retry 3 --output "$out" "$url"

# Valid chain and a Microsoft Corporation signer, or no installer. The
# PowerShell is single-quoted on purpose: its $ variables are PowerShell's.
# shellcheck disable=SC2016
MDOC_VC_REDIST=$(cygpath -w "$out") powershell.exe -NoProfile -NonInteractive -Command '
  $s = Get-AuthenticodeSignature -LiteralPath $env:MDOC_VC_REDIST
  if ($s.Status -ne "Valid" -or $s.SignerCertificate.Subject -notmatch "O=Microsoft Corporation") {
    [Console]::Error.WriteLine("vc_redist.x64.exe signature check failed: $($s.Status) $($s.SignerCertificate.Subject)")
    exit 1
  }' >&2 || die "refusing to bundle an unverified VC++ Redistributable"

echo "$out"
