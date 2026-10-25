#!/usr/bin/env bash
# Usage: scripts/gui-free.sh
# Fails when a GUI-free crate depends on GPUI on any target, through normal
# or build dependencies (ADR 0069). A crate joins the list in the PR that
# makes it GUI-free.
source "$(dirname "$0")/lib.sh"

crates=(mdoc-store mdoc-import mdoc-pii mdoc-history mdoc-spell)
failed=0
for crate in "${crates[@]}"; do
  found=$(cargo tree --locked -p "$crate" --all-features -e normal,build --target all --prefix none --format '{p}' |
    awk '$1 ~ /^gpui/ { print $1 }' | sort -u)
  if [ -n "$found" ]; then
    echo "error: $crate depends on GPUI: $(echo "$found" | tr '\n' ' ')(ADR 0069)" >&2
    failed=1
  fi
done
exit "$failed"
