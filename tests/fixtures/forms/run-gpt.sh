#!/bin/sh
# Usage: sh tests/fixtures/forms/run-gpt.sh <prompts-dir> <answers-dir> [jobs]
# Runs each prompt through Codex (GPT) once, in an empty temporary directory,
# read-only sandbox, stdin closed, 15-minute timeout. Skips answers already
# there. The answer is Codex's last message (-o); the full log sits beside it
# as <name>.log.
set -eu
if [ "${1:-}" = --one ]; then
  p=$2
  a=$3/$(basename "$p")
  [ -s "$a" ] && exit 0
  d=$(mktemp -d)
  cd "$d"
  timeout 900 codex exec -m gpt-6.2-sol -c model_reasoning_effort=high \
    --skip-git-repo-check -s read-only --ephemeral -o "$a" "$(cat "$p")" \
    </dev/null >"$a.log" 2>&1 || echo "failed: $p (see $a.log)" >&2
  cd /
  rmdir "$d" 2>/dev/null || true
  exit 0
fi
prompts=$(cd "$1" && pwd)
mkdir -p "$2"
answers=$(cd "$2" && pwd)
jobs=${3:-4}
self=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")
for p in "$prompts"/*.txt; do
  printf '%s\n' "$p"
done | xargs -P "$jobs" -I{} sh "$self" --one {} "$answers"
