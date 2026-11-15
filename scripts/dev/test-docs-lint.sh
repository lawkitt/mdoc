#!/usr/bin/env bash
# Usage: scripts/dev/test-docs-lint.sh
# scripts/dev/docs-lint.sh on scratch repositories: a tree that fits passes,
# and each rule fails on its own, naming what broke it: every budget (an ADR's
# Status and References not counted), a CONTEXT.md entry, a README, a
# docs/design folder, dated and pr-* evidence, an unclaimed, twice-claimed or
# both-ways ADR number, and a stale index. The index names each folded ADR's
# claiming file and a permalink to its last text from history, and keeps a
# row it already has without reading history. Bash 3.2 compatible.
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

git_() { git -C "$tmp/repo" -c user.name=t -c user.email=t@example.com "$@"; }

# A repository that fits: kept ADR 0001, and 0002 committed, then deleted
# and claimed by a spec's wrapped Folds: line.
base() {
  rm -rf "$tmp/repo"
  mkdir -p "$tmp/repo/scripts/dev" "$tmp/repo/docs/adr" "$tmp/repo/docs/specs" "$tmp/repo/docs/evidence/baseline"
  cp scripts/lib.sh "$tmp/repo/scripts/"
  cp scripts/dev/docs-lint.sh "$tmp/repo/scripts/dev/"
  git_ init -q
  printf '# Kept choice\n\nStatus: Accepted, 2026-01-01.\n\n%s\n' "$(words 300)" >"$tmp/repo/docs/adr/0001-kept.md"
  printf '# Folded choice\n\nStatus: Accepted, 2026-01-02.\n' >"$tmp/repo/docs/adr/0002-folded.md"
  printf '# Development\n\n%s\n' "$(words 100)" >"$tmp/repo/docs/development.md"
  printf '# CI\n\n%s\n' "$(words 700)" >"$tmp/repo/docs/specs/ci.md"
  printf '# Glossary\n\n## Terms\n\n**Term**: %s\n\n**Other**: %s\n' "$(words 50)" "$(words 50)" >"$tmp/repo/CONTEXT.md"
  printf '# Repo\n\n%s\n' "$(words 2000)" >"$tmp/repo/README.md"
  printf '# Tool\n\n%s\n' "$(words 1400)" >"$tmp/repo/docs/specs/README.md"
  git_ add -A
  git_ commit -qm one
  git_ rm -q docs/adr/0002-folded.md
  printf '# Desktop\n\nFolds:\n0002\n\nBody.\n' >"$tmp/repo/docs/specs/desktop.md"
  git_ add -A
  git_ commit -qm two
}

run() { # mode...: runs the lint in the repository; output in $tmp/out
  rc=0
  (cd "$tmp/repo" && bash scripts/dev/docs-lint.sh "$@") >"$tmp/out" 2>&1 || rc=$?
}

expect() { # rc text what
  if [ "$rc" -ne "$1" ] || ! grep -qF -- "$2" "$tmp/out"; then
    fail "$3 (exit $rc): $(cat "$tmp/out")"
  fi
}

# The fitting tree: writing the index passes, then --check passes.
base
run
expect 0 "wrote docs/adr/README.md" "a fitting tree fails"
run --check
expect 0 "docs-lint: ok" "the index just written is stale"
index=$tmp/repo/docs/adr/README.md
grep -qF '| 0001 | [Kept choice](0001-kept.md) | Kept |' "$index" || fail "no kept row: $(cat "$index")"
grep -q '^| 0002 | Folded choice | \[specs/desktop.md\](\.\./specs/desktop.md) · \[last text\](https://github.com/lawkitt/mdoc/blob/[0-9a-f]\{40\}/docs/adr/0002-folded.md) |$' "$index" ||
  fail "no folded row with a permalink: $(cat "$index")"
link=$(sed -n 's/.*blob\/\([0-9a-f]*\)\/docs\/adr\/0002.*/\1/p' "$index")
[ "$(git_ rev-list -1 HEAD~1)" = "$link" ] || fail "the permalink is not the last commit with the text"
grep -qF '| 0003 |' "$index" && fail "0003 is in the index before anything names it"
git_ add -A
git_ commit -qm index

# A folded row already in the index keeps its title and link: no history read.
sed -i.bak 's#blob/[0-9a-f]*/docs/adr/0002-folded.md#blob/kept/docs/adr/0002-folded.md#' "$index"
rm -f "$index.bak"
run
expect 0 "wrote" "rewriting an index with a known row fails"
grep -qF 'blob/kept/docs/adr/0002-folded.md' "$index" || fail "the known row was looked up again: $(cat "$index")"

# A claim that moves to another file keeps the row's title and link, with
# no history read, and names its new file.
printf '# Desktop\n\nBody.\n' >"$tmp/repo/docs/specs/desktop.md"
printf '# Kept choice\n\nStatus: Accepted, 2026-01-01.\n\nFolds: 0002.\n\n%s\n' "$(words 10)" >"$tmp/repo/docs/adr/0001-kept.md"
run
expect 0 "wrote" "moving a claim fails"
grep -qF '| 0002 | Folded choice | [0001-kept.md](0001-kept.md) · [last text](https://github.com/lawkitt/mdoc/blob/kept/docs/adr/0002-folded.md) |' "$index" ||
  fail "a moved claim lost its row or names the wrong file: $(cat "$index")"
run --check
expect 0 "docs-lint: ok" "the index after a moved claim is stale"

# A claim in a kept ADR counts: one in a spec too is a double claim.
printf '# Desktop\n\nFolds: 0002.\n' >"$tmp/repo/docs/specs/desktop.md"
run --check
expect 1 "ADR 0002 is claimed by more than one Folds: line (docs/adr/0001-kept.md docs/specs/desktop.md)" "a spec and a kept ADR both claim one number"

# A number claimed without history: no earlier text.
printf '# Desktop\n\nFolds: 0002.\n' >"$tmp/repo/docs/specs/desktop.md"
printf '# Kept choice\n\n%s\n' "$(words 10)" >"$tmp/repo/docs/adr/0001-kept.md"
printf '# Development\n\nFolds: 0003, 0004.\n' >"$tmp/repo/docs/development.md"
run
expect 1 "ADR 0003: no earlier text in git history" "a claim with no history passes"

# Budgets.
check_budget() { # path text what
  base
  run
  mkdir -p "$(dirname "$tmp/repo/$1")"
  printf '%s\n' "$2" >"$tmp/repo/$1"
  git_ add -A
  git_ commit -qm budget
  run --check
  expect 1 "$1:" "$3"
}
check_budget docs/development.md "$(words 2501)" "development.md over 2,500 passes"
check_budget docs/specs/ci.md "$(words 801)" "ci.md over 800 passes"
check_budget docs/specs/desktop.md "Folds: 0002. $(words 3001)" "a spec over 3,000 passes"
check_budget docs/adr/0001-kept.md "# Kept choice $(words 400)" "an ADR over 400 passes"
check_budget CONTEXT.md "$(words 3001)" "CONTEXT.md over 3,000 passes"
check_budget scripts/README.md "$(words 1501)" "a README over 1,500 passes"

# An ADR's Status, References and Terms paragraphs do not count.
base
printf '# Kept choice\n\nStatus: Accepted, 2026-01-01. Supersedes\n%s\n\n%s\n\nReferences: %s\n' \
  "$(words 300)" "$(words 390)" "$(words 300)" >"$tmp/repo/docs/adr/0001-kept.md"
run
expect 0 "docs-lint: ok" "an ADR's Status and References count"

# A CONTEXT.md entry over 60 words, named.
base
printf '# Glossary\n\n**Short**: %s\n\n**Long one**: %s\n%s\n\n## Next\n\n%s\n' \
  "$(words 10)" "$(words 40)" "$(words 30)" "$(words 200)" >"$tmp/repo/CONTEXT.md"
run
expect 1 'CONTEXT.md: "Long one" is' "a long CONTEXT.md entry passes"
grep -qF '"Short"' "$tmp/out" && fail "a short entry was reported"

# The repository README, vendor/ and the index have no budget.
base
mkdir -p "$tmp/repo/vendor/x"
printf '%s\n' "$(words 2000)" >"$tmp/repo/vendor/x/README.md"
git_ add -A
git_ commit -qm vendor
run
expect 0 "docs-lint: ok" "an exempt README fails"

# docs/design and dated or per-PR evidence; an undated folder is fine.
for d in docs/design docs/evidence/runs/2026-11-15 docs/evidence/runs/2026-11-15-native docs/evidence/pr-12; do
  base
  mkdir -p "$tmp/repo/$d"
  run
  expect 1 "$d" "$d passes"
done

# ADR numbers.
base
printf '# Kept 5\n' >"$tmp/repo/docs/adr/0005-five.md"
run
expect 1 "ADR 0003 is neither a file in docs/adr/ nor claimed" "a gap passes"
expect 1 "ADR 0004 is neither" "the second gap is not named"
base
printf '# Desktop\n\nFolds: 0002, 0001.\n' >"$tmp/repo/docs/specs/desktop.md"
run
expect 1 "ADR 0001 is a file and claimed by docs/specs/desktop.md" "a kept and claimed ADR passes"
base
printf '# CI\n\nFolds: 0002.\n\n%s\n' "$(words 10)" >"$tmp/repo/docs/specs/ci.md"
run
expect 1 "ADR 0002 is claimed by more than one Folds: line (docs/specs/ci.md docs/specs/desktop.md)" "a double claim passes"

# A stale index: --check fails and says how to fix it; writing fixes it.
base
run
git_ rm -q docs/adr/0001-kept.md 2>/dev/null || true
printf '# Mobile\n\nFolds: 0001.\n' >"$tmp/repo/docs/specs/mobile.md"
run --check
expect 1 "docs/adr/README.md is stale: run scripts/dev/docs-lint.sh" "a stale index passes"
run
expect 0 "wrote" "rewriting a stale index fails"
run --check
expect 0 "docs-lint: ok" "the rewritten index is stale"
grep -qF '| 0001 | Kept choice | [specs/mobile.md](../specs/mobile.md) · [last text](' "$index" ||
  fail "a newly folded ADR has no row from its last text: $(cat "$index")"

if [ "$failures" -ne 0 ]; then
  die "$failures docs-lint check(s) failed"
fi
echo "docs-lint: all checks passed"
