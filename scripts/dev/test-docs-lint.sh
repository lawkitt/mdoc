#!/usr/bin/env bash
# Usage: scripts/dev/test-docs-lint.sh
# scripts/dev/docs-lint.sh on scratch repositories: a tree that fits passes,
# and each rule fails on its own, naming what broke it: every ceiling (an
# ADR's Status paragraph and development.md's fleet-profile block not
# counted), a CONTEXT.md entry, a README, ADR numbers against Last number
# and Retired, a docs/design folder, dated and pr-* evidence, and a
# changelog fragment release.sh would reject. Bash 3.2 compatible.
source "$(dirname "$0")/../lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

failures=0
fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

words() { # n: n words on one line
  local i=0 out=
  while [ "$i" -lt "$1" ]; do
    out="$out w"
    i=$((i + 1))
  done
  echo "$out"
}

# A repository that fits.
base() {
  rm -rf "$tmp/repo"
  mkdir -p "$tmp/repo/scripts/dev" "$tmp/repo/docs/adr" "$tmp/repo/docs/specs" "$tmp/repo/docs/evidence/baseline"
  cp scripts/lib.sh "$tmp/repo/scripts/"
  cp scripts/dev/docs-lint.sh "$tmp/repo/scripts/dev/"
  git -C "$tmp/repo" init -q
  printf '# Kept choice\n\nStatus: Accepted, 2026-01-01.\n\n%s\n' "$(words 200)" >"$tmp/repo/docs/adr/0001-kept.md"
  printf '# Architecture decisions\n\nLast number: 0001.\n' >"$tmp/repo/docs/adr/README.md"
  printf '# Development\n\n%s\n' "$(words 100)" >"$tmp/repo/docs/development.md"
  printf '# CI\n\n%s\n' "$(words 400)" >"$tmp/repo/docs/specs/ci.md"
  printf '# Glossary\n\n## Terms\n\n**Term**: %s\n\n**Other**: %s\n' "$(words 50)" "$(words 50)" >"$tmp/repo/CONTEXT.md"
  printf '# Repo\n\n%s\n' "$(words 2000)" >"$tmp/repo/README.md"
  printf '# Tool\n\n%s\n' "$(words 700)" >"$tmp/repo/docs/specs/README.md"
  git -C "$tmp/repo" add -A
}

run() { # args...: runs the lint in the repository; output in $tmp/out
  rc=0
  (cd "$tmp/repo" && git add -A && bash scripts/dev/docs-lint.sh "$@") >"$tmp/out" 2>&1 || rc=$?
}

expect() { # rc text what
  if [ "$rc" -ne "$1" ] || ! grep -qF -- "$2" "$tmp/out"; then
    fail "$3 (exit $rc): $(cat "$tmp/out")"
  fi
}

base
run
expect 0 "docs-lint: ok" "a fitting tree fails"
run --check
expect 1 "usage:" "an argument is accepted"

# Ceilings.
check_ceiling() { # path text what
  base
  mkdir -p "$(dirname "$tmp/repo/$1")"
  printf '%s\n' "$2" >"$tmp/repo/$1"
  run
  expect 1 "$1:" "$3"
}
check_ceiling docs/development.md "$(words 1501)" "development.md over 1,500 passes"
check_ceiling docs/specs/ci.md "$(words 501)" "ci.md over 500 passes"
check_ceiling docs/specs/originals.md "$(words 1501)" "a spec over 1,500 passes"
check_ceiling docs/adr/0003-new.md "# New choice $(words 250)" "an ADR over 250 passes"
check_ceiling scripts/README.md "$(words 801)" "a README over 800 passes"

# An ADR's Status paragraph and development.md's fleet-profile block do not count.
base
printf '# Kept choice\n\nStatus: Accepted, 2026-01-01. Replaces\n%s\n\n%s\n' \
  "$(words 300)" "$(words 240)" >"$tmp/repo/docs/adr/0001-kept.md"
# shellcheck disable=SC2016 # literal backticks
printf '# Development\n\n%s\n\n```toml fleet-profile\n%s\n```\n\n%s\n' \
  "$(words 700)" "$(words 900)" "$(words 700)" >"$tmp/repo/docs/development.md"
run
expect 0 "docs-lint: ok" "an ADR's Status or the fleet profile counts"

# A CONTEXT.md entry over 60 words, named; CONTEXT.md over 2,000.
base
printf '# Glossary\n\n**Short**: %s\n\n**Long one**: %s\n%s\n\n## Next\n\n%s\n' \
  "$(words 10)" "$(words 40)" "$(words 30)" "$(words 2000)" >"$tmp/repo/CONTEXT.md"
run
expect 1 'CONTEXT.md: "Long one" is' "a long CONTEXT.md entry passes"
grep -qF '"Short"' "$tmp/out" && fail "a short entry was reported"
check_ceiling CONTEXT.md "$(words 2001)" "CONTEXT.md over 2,000 passes"

# The repository README and vendor/ have no ceiling.
base
mkdir -p "$tmp/repo/vendor/x"
printf '%s\n' "$(words 2000)" >"$tmp/repo/vendor/x/README.md"
run
expect 0 "docs-lint: ok" "an exempt README fails"

# ADR numbers.
base
printf '# Twin\n' >"$tmp/repo/docs/adr/0001-twin.md"
run
expect 1 "ADR 0001 has more than one file" "two files with one number pass"
base
printf '# Next\n' >"$tmp/repo/docs/adr/0002-next.md"
run
expect 1 "ADR 0002 is above docs/adr/README.md's Last number (0001)" "an ADR above Last number passes"
printf '# Architecture decisions\n\nLast number: 0002.\n' >"$tmp/repo/docs/adr/README.md"
run
expect 0 "docs-lint: ok" "raising Last number does not fix it"
printf '# Architecture decisions\n' >"$tmp/repo/docs/adr/README.md"
run
expect 1 "has no 'Last number: NNNN' line" "a README without Last number passes"

# Every number up to Last number is one file or Retired, never both.
base
printf '# Five\n' >"$tmp/repo/docs/adr/0005-five.md"
printf 'Last number: 0006.\n\nRetired: 0002-0003, 0006.\n' >"$tmp/repo/docs/adr/README.md"
run
expect 1 "ADR 0004 is neither a file nor Retired" "a gap passes"
grep -qF "ADR 0002" "$tmp/out" && fail "a retired range was reported"
printf 'Last number: 0006.\n\nRetired: 0002-0004, 0006.\n' >"$tmp/repo/docs/adr/README.md"
run
expect 0 "docs-lint: ok" "retired ranges and items fail"
printf '# Reused\n' >"$tmp/repo/docs/adr/0003-reused.md"
run
expect 1 "ADR 0003 is a file and Retired" "a reused number passes"
printf 'Last number: 0006.\n\nRetired: 0002-0004, 6.\n' >"$tmp/repo/docs/adr/README.md"
run
expect 1 'Retired item "6" is not NNNN' "a malformed Retired item passes"

# docs/design and dated or per-PR evidence; an undated folder is fine.
for d in docs/design docs/evidence/runs/2026-11-15 docs/evidence/runs/2026-11-15-native docs/evidence/pr-12; do
  base
  mkdir -p "$tmp/repo/$d"
  run
  expect 1 "$d" "$d passes"
done

# Changelog fragments, by release.sh's rules: good ones in each folder and
# the folder READMEs pass; each broken rule fails, naming the file.
fragments() { # writes good fragments and READMEs
  mkdir -p "$tmp/repo/changes/ios" "$tmp/repo/changes/android" "$tmp/repo/changes/mobile"
  printf -- '- Desktop.\n' >"$tmp/repo/changes/desk_top-1.2.md"
  printf -- '- iOS.\n' >"$tmp/repo/changes/ios/phone.md"
  printf -- '- Android.\n' >"$tmp/repo/changes/android/droid.md"
  printf -- '- Both apps.\n' >"$tmp/repo/changes/mobile/both.md"
  printf '# Changelog fragments\n' >"$tmp/repo/changes/README.md"
  printf '# Changelog\n\n## Unreleased\n' >"$tmp/repo/CHANGELOG.md"
}
base
fragments
run
expect 0 "docs-lint: ok" "good fragments fail"
printf 'Not a bullet.\n' >"$tmp/repo/changes/ios/plain.md"
run
expect 1 "changes/ios/plain.md must start with a '- ' CHANGELOG bullet" "a fragment that is not a bullet passes"
base
run
fragments
printf 'Not a bullet.\n' >"$tmp/repo/changes/mobile/plain.md"
run
expect 1 "changes/mobile/plain.md must start with a '- ' CHANGELOG bullet" "a mobile fragment that is not a bullet passes"
base
run
fragments
printf -- '- Spaced.\n' >"$tmp/repo/changes/two words.md"
run
expect 1 "changes/two words.md: name fragments with letters" "a badly named fragment passes"
base
run
fragments
printf '# Changelog\n\n## 1.0.0\n' >"$tmp/repo/CHANGELOG.md"
run
expect 1 "CHANGELOG.md has no ## Unreleased section for changes/" "fragments with no Unreleased section pass"
rm "$tmp/repo/changes/desk_top-1.2.md"
run
expect 0 "docs-lint: ok" "app fragments alone need the desktop Unreleased section"
# A dot-named fragment counts, as release.sh finds it, for its bullet and
# for the Unreleased section.
printf 'Hidden.\n' >"$tmp/repo/changes/.hidden.md"
run
expect 1 "changes/.hidden.md must start with a '- ' CHANGELOG bullet" "a dot-named fragment is skipped"
expect 1 "CHANGELOG.md has no ## Unreleased section for changes/" "a dot-named fragment needs no Unreleased section"

if [ "$failures" -ne 0 ]; then
  die "$failures docs-lint check(s) failed"
fi
echo "docs-lint: all checks passed"
