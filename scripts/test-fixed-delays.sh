#!/usr/bin/env bash
# Usage: scripts/test-fixed-delays.sh
# scripts/fixed-delays.sh on scratch trees (ADR 0104): an unmarked fixed
# delay in app code fails and names its line; one marked with its reason and
# an issue, or a deliberate duration with its ADR, passes, on its own line or
# in the comment just above; a marker without a reference, a marker on the
# line above a blank line, and a delay in test code are judged as they
# should be. Bash 3.2 compatible.
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# tree <name>: a scratch repository with the check, since lib.sh moves to the
# root of the repository the script sits in.
tree() {
  rm -rf "${tmp:?}/$1"
  mkdir -p "$tmp/$1/scripts" "$tmp/$1/apps/ios/mdoc" "$tmp/$1/apps/android/app/src/main/kotlin"
  cp scripts/fixed-delays.sh scripts/lib.sh "$tmp/$1/scripts/"
}

# put <name> <path> <text>: writes a file into the tree.
put() {
  mkdir -p "$(dirname "$tmp/$1/$2")"
  printf '%s\n' "$3" >"$tmp/$1/$2"
}

# expect <rc> <name>: runs the check in the tree; its output is in $tmp/out.
expect() {
  local rc=0
  bash "$tmp/$2/scripts/fixed-delays.sh" >"$tmp/out" 2>&1 || rc=$?
  [ "$rc" = "$1" ] || fail "$2: exited $rc, not $1: $(cat "$tmp/out")"
}

# An empty tree, and one without the app folders, pass.
tree empty
expect 0 empty
rm -rf "$tmp/empty/apps"
expect 0 empty

# Each unmarked call fails, with its file and line.
calls=(
  'try await Task.sleep(for: .seconds(1))'
  'try? await Task.sleep(nanoseconds: 1_000)'
  'DispatchQueue.main.asyncAfter(deadline: .now() + 1) { done() }'
  'Thread.sleep(forTimeInterval: 0.5)'
  'sleep(1)'
  'usleep(500)'
  'try await clock.sleep(for: .seconds(1))'
  'perform(#selector(go), with: nil, afterDelay: 1)'
  'Timer.scheduledTimer(withTimeInterval: 1, repeats: false) { _ in go() }'
)
for call in "${calls[@]}"; do
  tree swift
  put swift apps/ios/mdoc/Wait.swift "func go() async throws {
    $call
}"
  expect 1 swift
  grep -qF "apps/ios/mdoc/Wait.swift:2: $call" "$tmp/out" || fail "swift '$call' not named: $(cat "$tmp/out")"
done
for call in 'delay(250)' 'kotlinx.coroutines.delay(250)' 'Thread.sleep(2_000)' 'SystemClock.sleep(100)' 'handler.postDelayed({ go() }, 100)'; do
  tree kotlin
  put kotlin apps/android/app/src/main/kotlin/Wait.kt "fun go() {
    $call
}"
  expect 1 kotlin
  grep -qF "Wait.kt:2: $call" "$tmp/out" || fail "kotlin '$call' not named: $(cat "$tmp/out")"
done
# The Shared and RestoreExtension folders are app code too.
for dir in apps/ios/Shared apps/ios/RestoreExtension; do
  tree shared
  put shared "$dir/Wait.swift" 'try await Task.sleep(for: .seconds(1))'
  expect 1 shared
done

# Marked on the line, or in the comment block just above: passes.
tree marked
put marked apps/ios/mdoc/Find.swift '// Focus once the sheet is up.
// timing: iOS 26.2 hangs on an early focus (#634)
try? await Task.sleep(for: .milliseconds(250))
try? await Task.sleep(for: .milliseconds(250)) // timing: the same bug (#634)
// timing: how long a toast stays,
// which the UI system sets (ADR 0099)
try? await Task.sleep(for: .seconds(4))'
put marked apps/android/app/src/main/kotlin/Net.kt '    // timing: polls for a network (#697)
    Thread.sleep(2_000)'
expect 0 marked

# A marker that names no issue or ADR, or that sits above a blank line or a
# line of code, does not count.
tree unmarked
put unmarked apps/ios/mdoc/A.swift '// timing: just to be safe
try await Task.sleep(for: .seconds(1))'
expect 1 unmarked
grep -qF 'A.swift:2:' "$tmp/out" || fail "a marker without a reference passed: $(cat "$tmp/out")"
tree gap
put gap apps/ios/mdoc/B.swift '// timing: a bug (#1)

try await Task.sleep(for: .seconds(1))'
expect 1 gap
tree code
put code apps/ios/mdoc/C.swift 'let x = 1 // timing: a bug (#1)
try await Task.sleep(for: .seconds(1))'
expect 1 code
grep -qF 'C.swift:2:' "$tmp/out" || fail "a marker on a line of code covered the next: $(cat "$tmp/out")"
# One marked call does not cover the next one in the same file.
tree next
put next apps/ios/mdoc/D.swift '// timing: a bug (#1)
try await Task.sleep(for: .seconds(1))
go()
try await Task.sleep(for: .seconds(1))'
expect 1 next
grep -qF 'D.swift:4:' "$tmp/out" || fail "the second call passed: $(cat "$tmp/out")"
grep -qF 'D.swift:2:' "$tmp/out" && fail "the marked call failed: $(cat "$tmp/out")"

# Not calls: comments, longer names, and test code outside the app folders.
tree other
# shellcheck disable=SC2016 # Swift backticks, not shell ones
put other apps/ios/mdoc/E.swift '/// No `Task.sleep` here: wait for the event.
// sleep(1) would be wrong
/* asyncAfter */
let startDelay(x) = 1
func animationDelay() {}
go() // then Task.sleep(for: .seconds(1)) in a comment'
put other apps/ios/mdocTests/WaitTests.swift 'Thread.sleep(forTimeInterval: 0.01)'
put other apps/ios/mdocUITests/SurveyUITests.swift 'sleep(1)'
put other apps/android/app/src/test/kotlin/WaitTest.kt 'Thread.sleep(500)'
put other apps/android/app/src/androidTest/kotlin/SurveyTest.kt 'delay(500)'
expect 0 other

# Only Swift, Kotlin and Java are read.
tree types
put types apps/ios/mdoc/notes.txt 'Task.sleep(for: .seconds(1))'
expect 0 types

# The repository itself passes.
bash scripts/fixed-delays.sh >"$tmp/out" 2>&1 || fail "the repository has an unmarked fixed delay: $(cat "$tmp/out")"

echo "fixed-delays: ok"
