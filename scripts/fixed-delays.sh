#!/usr/bin/env bash
# Usage: scripts/fixed-delays.sh
# Fails when the mobile apps' code waits a fixed time without saying why
# (ADR 0104, Timing): a call that sleeps or delays in apps/ios/mdoc,
# apps/ios/Shared, apps/ios/RestoreExtension, apps/ios/MdocFFI or
# apps/android/app/src/main (Swift, Kotlin, Java). The calls: anything named
# `sleep`, `usleep`, `nanosleep` or `delay` (`Task.sleep`, `Thread.sleep`, a
# clock's `.sleep`, `SystemClock.sleep`, Kotlin's `delay`, an animation's
# `.delay`), `asyncAfter`, `afterDelay:`, `schedule(after:`, a name ending in
# `Delayed` (`postDelayed`, `sendMessageDelayed`), `scheduledTimer`,
# `Timer(timeInterval:` or `Timer(fire:`, `Timer.publish`, `CountDownTimer`,
# and a run loop's `run(until:`. A call passes when its line, or the `//`
# comment lines just above it, carry a marker that names its reason and an
# issue, or for a deliberate on-screen duration the ADR that defines it:
#
#   // timing: iOS 26.2 hangs when the field takes focus too early (#634)
#   // timing: how long a toast stays (ADR 0099)
#
# Test code and UI tests live outside those folders, so they are exempt.
# Comments and string literals are not code: a call named in either is not
# a call. Bash 3.2 compatible (macOS /bin/bash); BWK awk, mawk and gawk.
# shellcheck source=lib.sh
source "$(dirname "$0")/lib.sh"

dirs=()
for dir in apps/ios/mdoc apps/ios/Shared apps/ios/RestoreExtension apps/ios/MdocFFI apps/android/app/src/main; do
  [ ! -d "$dir" ] || dirs+=("$dir")
done
[ "${#dirs[@]}" -gt 0 ] || exit 0

files=$(find "${dirs[@]}" -type f \( -name '*.swift' -o -name '*.kt' -o -name '*.java' \) | LC_ALL=C sort)
[ -n "$files" ] || exit 0

# shellcheck disable=SC2016 # awk fields, not shell ones
echo "$files" | tr '\n' '\0' | xargs -0 awk '
  FNR == 1 { above = ""; block = 0 }
  {
    line = $0
    sub(/\r$/, "", line)
    # The code on the line: without string literals, then without comments
    # (a block comment may run over several lines).
    code = line
    gsub(/"([^"\\]|\\.)*"/, "\"\"", code)
    kept = ""
    while (code != "") {
      if (block) {
        cl = index(code, "*/")
        if (cl == 0) break
        code = substr(code, cl + 2)
        block = 0
        continue
      }
      op = index(code, "/*")
      sl = index(code, "//")
      if (sl > 0 && (op == 0 || sl < op)) { kept = kept substr(code, 1, sl - 1); break }
      if (op == 0) { kept = kept code; break }
      kept = kept substr(code, 1, op - 1) " "
      code = substr(code, op + 2)
      block = 1
    }
    code = kept
    trimmed = line
    sub(/^[ \t]+/, "", trimmed)
    if (code ~ /^[ \t]*$/) {
      # A comment line joins the block above the next line of code; a blank
      # line ends it.
      if (trimmed == "") above = ""
      else above = above " " trimmed
      next
    }
    if (code ~ /(^|[^A-Za-z0-9_])((u|nano)?sleep|delay|asyncAfter|scheduledTimer|CountDownTimer)[ \t]*\(/ ||
      code ~ /Delayed[ \t]*\(/ || code ~ /afterDelay:/ || code ~ /schedule[ \t]*\([ \t]*after:/ ||
      code ~ /(^|[^A-Za-z0-9_])Timer[ \t]*\([ \t]*(timeInterval|fire):/ || code ~ /Timer\.publish[ \t]*\(/ ||
      code ~ /\.run[ \t]*\([ \t]*(mode:[^)]*)?until:/) {
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
