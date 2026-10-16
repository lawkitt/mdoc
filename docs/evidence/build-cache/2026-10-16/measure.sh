#!/usr/bin/env bash
# Usage: measure.sh <throwaway checkout> <results.tsv> [config...]
# ADR 0050 measurement. Rewrites Cargo.toml and leaf sources in the checkout,
# so give it a disposable worktree of the commit under test, never a working
# copy. Each configuration starts every cold step from an empty target/.
set -euo pipefail

repo=$1 out=$2
shift 2
[ $# -gt 0 ] && configs=("$@") || configs=(default unpacked packed)
cd "$repo"
[ -z "$(git status --porcelain)" ] || { echo "checkout is not clean" >&2; exit 1; }

now() { perl -MTime::HiRes=time -e 'printf "%.1f\n", time'; }
kib() { [ -d target ] && du -sk target | cut -f1 || echo 0; }
objs() { [ -d target ] && find target -name '*.o' | wc -l | tr -d ' ' || echo 0; }

[ -s "$out" ] || printf 'config\tstep\titer\tseconds\ttarget_kib\tdelta_kib\tobject_files\texit\n' > "$out"

step() { # config step iter command...
  local config=$1 name=$2 iter=$3; shift 3
  local before start code=0 after
  before=$(kib); start=$(now)
  "$@" > /dev/null 2>&1 || code=$?
  after=$(kib)
  printf '%s\t%s\t%s\t%.1f\t%s\t%s\t%s\t%s\n' "$config" "$name" "$iter" \
    "$(echo "$(now) - $start" | bc)" "$after" "$((after - before))" "$(objs)" "$code" | tee -a "$out"
}

configure() {
  git checkout -q -- Cargo.toml src crates
  case $1 in
    default) ;;
    unpacked|packed) cat >> Cargo.toml <<EOF

[profile.dev]
debug = "line-tables-only"
split-debuginfo = "$1"

[profile.dev.package."*"]
debug = 0
EOF
    ;;
    *) echo "unknown config $1" >&2; exit 1 ;;
  esac
}

# Each cycle changes a real function body. `mdoc_edit` touches the binary
# crate only; `editor_edit` touches mdoc-editor, which mdoc then rebuilds against.
mdoc_edit() { perl -pi -e 's/"Just now[^"]*"/"Just now '"$1"'"/' src/recents.rs; }
editor_edit() {
  perl -0pi -e 's/(fn shape_cell_lines\([^{]*\{)(\n    std::hint::black_box\(\d+\);)?/$1\n    std::hint::black_box('"$1"');/' \
    crates/mdoc-editor/src/tables.rs
}

for config in "${configs[@]}"; do
  configure "$config"
  rm -rf target
  step "$config" cold-build 0 cargo build --locked
  rm -rf target
  step "$config" cold-test 0 just test
  for i in $(seq 1 10); do
    mdoc_edit "$i"
    step "$config" warm-mdoc-test-no-run "$i" cargo test --locked -p mdoc --no-run
  done
  for i in $(seq 1 10); do
    editor_edit "$i"
    step "$config" warm-editor-mdoc-test-no-run "$i" cargo test --locked -p mdoc --no-run
  done
  for i in $(seq 11 15); do
    editor_edit "$i"
    step "$config" warm-editor-lint "$i" just lint
  done
done
git checkout -q -- Cargo.toml src crates
