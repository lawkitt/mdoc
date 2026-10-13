#!/usr/bin/env bash
# Usage: scripts/keychain-launch-check.sh [/Applications/mdoc.app]
# Release check (ADR 0045): launching mdoc never shows the macOS Keychain
# password dialog, even when a case key exists that this build cannot read
# silently. Run on a clean macOS user account with the signed release
# installed and quit.
#
# It seeds a login-keychain case key with `security add-generic-password`,
# whose access list trusts only /usr/bin/security, exactly like a key made by
# another build. Then it launches the app and watches for the dialog. Expected:
# no dialog, and the bar "Saved cases are locked…" with Unlock… in the window.
# The seeded key and cases folder are removed afterwards.
# shellcheck source=lib.sh
source "$(dirname "$0")/lib.sh"

app=${1:-/Applications/mdoc.app}
[ -d "$app" ] || die "$app not found"
service=io.github.lawkitt.mdoc
cases="$HOME/Library/Application Support/mdoc/cases"
pgrep -xq mdoc && die "quit mdoc first"
security find-generic-password -s "$service" -a case-key >/dev/null 2>&1 &&
  die "a case key already exists; run this on a clean user account"
[ -e "$cases" ] && die "$cases already exists; run this on a clean user account"

cleanup() {
  pkill -x mdoc 2>/dev/null || true
  security delete-generic-password -s "$service" -a case-key >/dev/null 2>&1 || true
  rm -rf "$cases"
}
trap cleanup EXIT

security add-generic-password -s "$service" -a case-key -X "$(openssl rand -hex 32)"
mkdir -p "$cases"
open "$app"
echo "launched $(basename "$app"); watching for a Keychain dialog for 15 s"

dialog() {
  pgrep -xq SecurityAgent || { echo 0; return; }
  osascript -e 'tell application "System Events" to count windows of process "SecurityAgent"' \
    2>/dev/null || echo unknown
}
for _ in $(seq 15); do
  sleep 1
  case $(dialog) in
    0 | "") ;;
    unknown)
      read -rp "Cannot read windows (allow Terminal in Accessibility to automate). \
Did a Keychain password dialog appear? [y/N] " answer
      [[ $answer == [yY]* ]] && die "FAIL: launch showed the Keychain dialog"
      break
      ;;
    *) die "FAIL: launch showed the Keychain dialog" ;;
  esac
done
read -rp "Does the window show \"Saved cases are locked…\" with Unlock…? [y/N] " answer
[[ $answer == [yY]* ]] || die "FAIL: no locked-cases bar"
echo "PASS: no Keychain dialog at launch; cases locked"
