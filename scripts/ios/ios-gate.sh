#!/usr/bin/env bash
# Usage: scripts/ios/ios-gate.sh clean
#        scripts/ios/ios-gate.sh tier <model>
#        scripts/ios/ios-gate.sh record <commit> <udid> <model> <tier>
#        scripts/ios/ios-gate.sh check <commit>
# The iPhone 12 Pro gate's record, shared by
# scripts/ios/ios-device-test.sh, which writes it, and `just release-ios`, which
# requires it. Records live in target/ios/gate/<commit> (MDOC_GATE_DIR).
#   clean   Succeeds when the checkout has no change, untracked files
#           included: Xcode builds untracked sources the tag would lack.
#   tier    The phone's tier by its model (a 4 GB floor):
#           below-floor for the 3 GB iPhone SE 2nd generation (iPhone12,8),
#           detector for every other iPhone iOS 26 runs on.
#   record  Writes the record of a pass.
#   check   Succeeds when <commit> has a detector-tier pass at the phone's
#           own tier (not a forced one) on a phone older than the iPhone 15
#           Pro, the slowest class that finds names (the iPhone 12 Pro), and
#           prints its gate line for the tag's trailer.
# Bash 3.2.
source "$(dirname "$0")/../lib.sh"
gates=${MDOC_GATE_DIR:-target/ios/gate}
usage="usage: scripts/ios/ios-gate.sh clean | tier <model> | record <commit> <udid> <model> <tier> | check <commit>"

tier_of() {
  local major
  case "$1" in
    iPhone[0-9]*,*) major=${1#iPhone} && major=${major%%,*} ;;
    *) die "not an iPhone model: $1" ;;
  esac
  if [ "$1" = iPhone12,8 ]; then echo below-floor; else echo detector; fi
}

field() { sed -n "s/^$1=//p" "$2"; }

case "${1:-}" in
  clean)
    # A git error is not a clean checkout.
    status=$(git status --porcelain --untracked-files=normal) || die "git status failed"
    [ -z "$status" ]
    ;;
  tier)
    tier_of "${2:?$usage}"
    ;;
  record)
    [ $# = 5 ] || die "$usage"
    mkdir -p "$gates"
    printf 'phone=%s\nudid=%s\ntier=%s\ncommit=%s\npassed=%s\n' "$4" "$3" "$5" "$2" \
      "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$gates/$2"
    echo "gate recorded: $gates/$2"
    ;;
  check)
    commit=${2:?$usage}
    record=$gates/$commit
    [ -f "$record" ] || die "no gate record for $commit: run \`just ios-device-test <12 Pro UDID>\` on it first"
    [ "$(field commit "$record")" = "$commit" ] || die "the gate record $record names another commit"
    phone=$(field phone "$record")
    tier=$(field tier "$record")
    [ "$tier" = "$(tier_of "$phone")" ] ||
      die "the gate record for $commit forced the $tier tier on $phone; the gate runs at the phone's own tier"
    [ "$tier" = detector ] ||
      die "the gate for $commit ran on $phone, below the floor; the gate finds names (the iPhone 12 Pro)"
    major=${phone#iPhone} && major=${major%%,*}
    [ "$major" -lt 16 ] ||
      die "the gate for $commit ran on $phone; the gate is an older, slower phone (the iPhone 12 Pro)"
    echo "just ios-device-test, $tier tier, on $phone, passed $(field passed "$record")"
    ;;
  *) die "$usage" ;;
esac
