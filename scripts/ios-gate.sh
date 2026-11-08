#!/usr/bin/env bash
# Usage: scripts/ios-gate.sh clean
#        scripts/ios-gate.sh tier <model>
#        scripts/ios-gate.sh record <commit> <udid> <model> <tier>
#        scripts/ios-gate.sh check <commit>
# The iPhone 12 Pro gate's record (ADR 0107), shared by
# scripts/ios-device-test.sh, which writes it, and `just release-ios`, which
# requires it. Records live in target/ios/gate/<commit> (MDOC_GATE_DIR).
#   clean   Succeeds when the checkout has no change, untracked files
#           included: Xcode builds untracked sources the tag would lack.
#   tier    The phone's tier by its model (ADR 0088): detector from the
#           iPhone 15 Pro (iPhone16,x) on, below-floor before it.
#   record  Writes the record of a pass.
#   check   Succeeds when <commit> has a below-floor pass on a phone below
#           the floor (not a stand-in memory size on a newer one), and
#           prints its gate line for the tag's trailer.
# Bash 3.2.
source "$(dirname "$0")/lib.sh"
gates=${MDOC_GATE_DIR:-target/ios/gate}
usage="usage: scripts/ios-gate.sh clean | tier <model> | record <commit> <udid> <model> <tier> | check <commit>"

tier_of() {
  local major
  case "$1" in
    iPhone[0-9]*,*) major=${1#iPhone} && major=${major%%,*} ;;
    *) die "not an iPhone model: $1" ;;
  esac
  if [ "$major" -ge 16 ]; then echo detector; else echo below-floor; fi
}

field() { sed -n "s/^$1=//p" "$2"; }

case "${1:-}" in
  clean)
    [ -z "$(git status --porcelain --untracked-files=normal)" ]
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
    [ "$(field tier "$record")" = below-floor ] || die "the gate record for $commit is not the below-floor tier"
    phone=$(field phone "$record")
    [ "$(tier_of "$phone")" = below-floor ] ||
      die "the gate for $commit ran on $phone, which meets the floor; the gate is a phone below it (the iPhone 12 Pro)"
    echo "just ios-device-test, below-floor tier, on $phone, passed $(field passed "$record")"
    ;;
  *) die "$usage" ;;
esac
