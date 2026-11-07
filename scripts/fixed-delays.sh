#!/usr/bin/env bash
# Usage: scripts/fixed-delays.sh
# Fails when the mobile apps' code waits a fixed time without saying why
# (ADR 0104, Timing): a call that sleeps or delays (`Task.sleep`,
# `Thread.sleep`, `sleep(`, `usleep(`, `nanosleep(`, a clock's `.sleep(`,
# `asyncAfter`, `afterDelay:`, `scheduledTimer(`, Kotlin's `delay(`,
# `SystemClock.sleep(`, `postDelayed(`) in apps/ios/mdoc, apps/ios/Shared,
# apps/ios/RestoreExtension or apps/android/app/src/main. A call passes when
# its line, or the `//` comment lines just above it, carry a marker that
# names its reason and an issue, or for a deliberate on-screen duration the
# ADR that sets it:
#
#   // timing: iOS 26.2 hangs when the field takes focus too early (#634)
#   // timing: how long a toast stays (ADR 0099)
#
# Test code and UI tests live outside those folders, so they are exempt.
# Comments are not code: a call named in a comment is not a call. Bash 3.2
# compatible (macOS /bin/bash).
# shellcheck source=lib.sh
source "$(dirname "$0")/lib.sh"

dirs=()
for dir in apps/ios/mdoc apps/ios/Shared apps/ios/RestoreExtension apps/android/app/src/main; do
  [ ! -d "$dir" ] || dirs+=("$dir")
done
[ "${#dirs[@]}" -gt 0 ] || exit 0

files=$(find "${dirs[@]}" -type f \( -name '*.swift' -o -name '*.kt' -o -name '*.java' \) | LC_ALL=C sort)
[ -n "$files" ] || exit 0

# shellcheck disable=SC2016 # awk fields, not shell ones
echo "$files" | tr '\n' '\0' | xargs -0 awk '
  FNR == 1 { above = "" }
  {
    line = $0
    trimmed = line
    sub(/^[ \t]+/, "", trimmed)
    # A comment line joins the block above the next line of code.
    if (trimmed ~ /^(\/\/|\/\*|\*)/) { above = above " " trimmed; next }
    if (trimmed == "") { above = ""; next }
    code = line
    sub(/\/\/.*/, "", code)
    if (code ~ /(^|[^A-Za-z0-9_])((u|nano)?sleep|delay|asyncAfter|postDelayed|scheduledTimer)\(/ || code ~ /afterDelay:/) {
      if ((line above) !~ /timing:.*(#[0-9]+|ADR [0-9][0-9][0-9][0-9])/) {
        printf "%s:%d: %s\n", FILENAME, FNR, trimmed
        bad = 1
      }
    }
    above = ""
  }
  END { exit bad }
' || {
  echo "error: a fixed delay in app code (ADR 0104): wait for the event instead, or mark the line \`// timing: <why> (#<issue>)\` (a deliberate on-screen duration: \`(ADR <n>)\`)" >&2
  exit 1
}
