#!/usr/bin/env bash
# Usage: scripts/ci/test-ci-areas.sh
#
# The area table (scripts/ci/ci-areas.txt) and its readers in lib.sh and
# scripts/ci/ci-areas.sh: ci.yml's and main-caches.yml's paths-ignore repeat the
# table's `none` lines word for word, and each file routes to the areas the
# table gives it. No network or builds. Bash 3.2 compatible.
# shellcheck source=../lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
failures=0

check() { # description command...
  local what=$1
  shift
  if "$@"; then
    echo "ok: $what"
  else
    echo "FAIL: $what" >&2
    failures=$((failures + 1))
  fi
}

not() { ! "$@"; }

# A Windows checkout may have it with CRLF.
table=$(tr -d '\r' <scripts/ci/ci-areas.txt)

# ci.yml's paths-ignore is the table's `none` lines, word for word.
# The first paths-ignore list in the YAML on stdin, one pattern per line.
paths_ignore() {
  awk '
    done { next }
    inlist {
      if ($0 ~ /^[[:space:]]*-[[:space:]]/) {
        sub(/^[[:space:]]*-[[:space:]]*/, "")
        sub(/[[:space:]]+#.*$/, "")
        gsub(/^["'\'']|["'\''][[:space:]]*$/, "")
        print
        next
      }
      done = 1
      next
    }
    /^[[:space:]]*paths-ignore:/ { inlist = 1 }
  '
}
ignored=$(tr -d '\r' <.github/workflows/ci.yml | paths_ignore)
none=$(awk '$1 == "none" { print $2 }' <<<"$table")
check "ci.yml's paths-ignore list is read" [ -n "$ignored" ]
check "ci.yml's paths-ignore is the table's none lines, in order" [ "$ignored" = "$none" ]
check "main-caches.yml's paths-ignore is the same" \
  [ "$(tr -d '\r' <.github/workflows/main-caches.yml | paths_ignore)" = "$none" ]
# The lines above the first none line are exceptions to it: the fixture
# folders, whose Markdown tests read. GitHub's paths-ignore takes no `!`, so
# ci.yml's push filter skips them.
check "the exceptions are fixture folders, so a README under apps/ or crates/ is docs" \
  [ -z "$(grep -vE '^[[:space:]]*(#|$)' <<<"$table" | awk '$1 == "none" { exit } { print $2 }' | grep -vE '^(tests|crates/(\*\*|[^/*]+))/fixtures/')" ]
check "the none lines are one block" \
  [ "$(grep -vE '^[[:space:]]*(#|$)' <<<"$table" | awk '$1 == "none" { inside = 1; next } inside { after = 1 } after && $1 == "none" { bad = 1 } END { print bad + 0 }')" = 0 ]
check "every line names known areas and one glob" \
  [ -z "$(grep -vE '^[[:space:]]*(#|$)' <<<"$table" | grep -vE '^(none|core|desktop|recognition|ios|android|ci)(,(desktop|ios|android))*[[:space:]]+[^[:space:]]+$')" ]
# The desktop jobs follow the table through ci.yml's areas job.
check "ci.yml's areas job reads the table" grep -q 'run: scripts/ci/ci-areas.sh --github' .github/workflows/ci.yml
check "hosted clippy and test wait for it" \
  [ "$(grep -cE '^    needs: areas$' .github/workflows/ci.yml)" = 3 ] # and detection-head
check "and run only for the desktop area" grep -q "if: &desktop \${{ needs.areas.outputs.desktop == 'true' }}" .github/workflows/ci.yml
# A docs-only PR head skips the fast tier's work, by the same table: merge-gate
# does not read it there.
check "both fast jobs have the docs-only step" \
  [ "$(grep -cE '^      - (&docs-only|\*docs-only)$' .github/workflows/ci.yml)" = 2 ]
# shellcheck disable=SC2016 # the workflow's text, not an expansion
check "which reads the table" grep -q 'if areas=$(scripts/ci/ci-areas.sh) && \[ -z "$areas" \]; then' .github/workflows/ci.yml
check "and their test steps wait on it" \
  [ "$(grep -cE "^      - if: .*steps\.docs-only\.outputs\.docs-only != 'true'$" .github/workflows/ci.yml)" = 2 ]

# Routing: a file takes the first matching line's areas, desktop by default.
areas_of() { printf '%s\0' "$@" | changed_areas "$table"; }
routes() { # expected files...
  local want=$1
  shift
  check "$* -> ${want:-no area}" [ "$(areas_of "$@")" = "$want" ]
}
routes ios apps/ios/mdoc/App/MdocApp.swift
routes ios apps/ios/mdoc.xcodeproj/project.pbxproj scripts/ios/build-ios-core.sh scripts/ios/ios-survey.sh scripts/ios/ios-toolchain.sh scripts/ios/ios-scan-pages.swift
routes android apps/android/app/build.gradle.kts
routes android scripts/android/build-android-core.sh scripts/android/android-emulator.sh scripts/android/android-package-xml.py scripts/android/install-android-sdk.sh
routes "core desktop recognition ios android ci" crates/mdoc-ffi/src/lib.rs
routes "core desktop recognition ios android ci" Cargo.lock
routes "core desktop recognition ios android ci" scripts/lib.sh
routes "core desktop recognition ios android ci" justfile apps/ios/mdoc/App/MdocApp.swift
routes "core desktop recognition ios android ci" scripts/ci/ci-areas.txt
routes desktop src/main.rs
routes desktop tests/render.rs tests/fixtures/import/a.docx vendor/gpui-pre-macos/src/lib.rs build/windows.rc
routes "desktop recognition" .github/actions/setup/action.yml .github/workflows/ci-detection.yml scripts/ci/detection-gate.sh
routes "desktop recognition" resources/pseudonymization-model.json
routes "desktop recognition" resources/other-input.json crates/mdoc-detect/src/lib.rs
routes desktop scripts/release/release.sh scripts/release/test-release.sh scripts/ci/quarantine.sh scripts/dev/test.sh
# The CI machinery (ADR 0100): the script suites, not the desktop jobs.
routes ci scripts/ci/merge-gate.sh scripts/ci/test-merge-gate.sh scripts/queue-runner.sh scripts/ci/test-queue-runner.sh scripts/ci/queue-runner-setup.ps1
routes ci scripts/ci/ci-cache-cleanup.sh scripts/ci/ci-stats.sh scripts/ci/deny-ci.sh scripts/ci/nightly-issue.sh
routes ci .github/workflows/ci.yml .github/workflows/ci-scripts.yml .github/workflows/merge-gate.yml .github/workflows/nightly.yml .github/workflows/main-caches.yml
# A workflow that builds or tests the app keeps the area of what it runs.
routes desktop .github/workflows/ci-desktop.yml .github/workflows/queue.yml .github/workflows/release.yml
routes ios .github/workflows/ci-ios.yml
routes android .github/workflows/ci-android.yml
routes "core desktop recognition ios android ci" scripts/ci/ci-areas.sh
routes "desktop ci" scripts/ci/merge-gate.sh src/main.rs
routes "ios ci" scripts/ci/merge-gate.sh apps/ios/a.swift
routes "desktop ios" scripts/ios/mobile-check.sh
routes "desktop android" scripts/android/install-android-ndk.sh
routes desktop scripts/ios/test-ios-release.sh scripts/ci/hub-gate.sh scripts/ci/snapshots-fetch.sh scripts/desktop/package.sh
routes ci scripts/release/ci-apple-credentials.sh scripts/ci/ci-screens.sh
routes "desktop ios" src/main.rs apps/ios/mdoc/App/MdocApp.swift
routes "ios android" apps/ios/a.swift apps/android/b.kt
routes recognition tests/fixtures/recognition/en/claim/x.md
routes recognition tests/fixtures/recognition/en/claim/x.model.json
# Fixture Markdown is read by tests: each takes its folder's area.
routes desktop tests/fixtures/import/text.docx.md tests/fixtures/mobile-markdown/review.md tests/fixtures/welcome.md
routes desktop tests/fixtures/pseudonymization/README.md
routes "desktop recognition" crates/mdoc-pii/fixtures/restore/en-summary.reply.md
routes "core desktop recognition ios android ci" crates/mdoc-markdown/fixtures/from_html/chatgpt-ios.md
routes "" README.md docs/adr/0095-ci-on-a-merge-queue.md LICENSE
routes "" apps/ios/README.md crates/mdoc-ffi/README.md changes/ios/x.md
routes "ios" apps/ios/README.md apps/ios/a.swift
check "no files runs every area" [ "$(changed_areas "$table" </dev/null)" = "core desktop recognition ios android ci" ]
check "a table with CRLF line endings routes the same" \
  [ "$(printf 'apps/ios/a.swift\0' | changed_areas "${table//$'\n'/$'\r\n'}")" = ios ]
check "docs.rs is not under docs/" [ "$(areas_of docs.rs)" = desktop ]
# A newer table read by an older reader: an area name it does not know
# runs every job, never none (hub review on #622).
check "an unknown area name counts as core" \
  [ "$(printf 'some/file\0' | changed_areas $'newarea  some/file\nnone  **')" = "core desktop recognition ios android ci" ]
check "also beside a known one" \
  [ "$(printf 'some/file\0' | changed_areas $'ios,newarea  some/file')" = "core desktop recognition ios android ci" ]
check "known names still route as before" \
  [ "$(printf 'some/file\0b.md\0' | changed_areas $'ios,android  some/file\nnone  **/*.md')" = "ios android" ]
# A queue commit's own change (docs_only): 1 to 299 files, none in an area.
docs_only_of() { printf '%s\0' "$@" | docs_only "$table"; }
check "docs files are docs only" docs_only_of README.md docs/a.md changes/x.md
check "fixture Markdown is not" not docs_only_of README.md tests/fixtures/import/a.md
check "a code file is not" not docs_only_of README.md src/main.rs
check "no files is not" not docs_only_of
many=()
for i in $(seq 1 300); do many+=("docs/$i.md"); done
check "300 files are not (GitHub lists no more)" not docs_only_of "${many[@]}"
check "299 are" docs_only_of "${many[@]:1}"
# shellcheck disable=SC2317,SC2329 # called through check
matches() { [[ $1 =~ $(glob_regex "$2") ]]; }
check "* does not cross directories" not matches scripts/x/ios-a.sh "scripts/ios-*.sh"
check "a dot is literal" not matches build/appiconxsvg build/appicon.svg
check "** crosses directories" matches docs/a/b/c.png "docs/**"
check "**/ matches at the root and below" eval 'matches b.md "**/*.md" && matches a/b.md "**/*.md"'
check "affects reads the list" eval 'affects ios "desktop ios" && ! affects android "desktop ios"'
# The CLI's GitHub mode: anything but a PR or a known push runs every area.
every="areas=core desktop recognition ios android ci"$'\n'"desktop=true"
check "a manual run runs every area" [ "$(EVENT=workflow_dispatch bash scripts/ci/ci-areas.sh --github)" = "$every" ]
check "a release call runs every area" [ "$(EVENT=push RELEASE_CALL=true BEFORE=1234567890123456789012345678901234567890 bash scripts/ci/ci-areas.sh --github)" = "$every" ]
check "a new branch's push runs every area" [ "$(EVENT=push BEFORE=0000000000000000000000000000000000000000 bash scripts/ci/ci-areas.sh --github)" = "$every" ]

[ "$failures" -eq 0 ] || die "$failures area table check(s) failed"
echo "area table: all checks passed"
