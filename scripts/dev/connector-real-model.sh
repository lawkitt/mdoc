#!/usr/bin/env bash
# Usage: scripts/dev/connector-real-model.sh [--keep] [--claude-config <dir>]
#
# The AI connector with a real model, end to end (ADR 0126; Milestone 2
# item 12). macOS only, run by hand, never in CI: it spends the runner's
# Claude usage. It builds a debug mdoc, `mdoc-mcp` and the Vault helper
# (crates/mdoc-connector/examples/real_model.rs), seeds a new scratch dev
# profile with one Project holding a Ready lease whose people are aliases,
# and starts mdoc on it with the AI project shared (`MDOC_DEV_AI_PROJECT`, a
# debug-only hook). Then `claude -p`, with only the mdoc tools and only this
# shim as its MCP server, lists the shared documents, reads the lease,
# answers with the aliases as written and delivers the answer as a reply.
# The helper then checks that the Vault recorded the read and the delivery,
# that the reply restores to the real names, and that no real name is in
# the transcript (tool calls, tool results and the model's text). It prints
# PASS or FAIL per check and exits 0 only when all pass.
#
# The transcript (stream-json) and mdoc's log stay in the scratch folder it
# prints. The dev profile and the throwaway Claude config are deleted at the
# end unless --keep is given.
#
# Auth: the Claude config is a new folder (`CLAUDE_CONFIG_DIR`), so it has
# no login of its own; the script stops before building when it isn't
# logged in. Either pass --claude-config <dir>, a folder you logged in to
# once with `CLAUDE_CONFIG_DIR=<dir> claude auth login` (kept, never
# written by the check beyond what claude itself writes there), or run it
# from a shell where ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN is set.
# It never reads or writes ~/.claude or ~/.claude.json, and never asks for
# a login.
#
# A Claude Code session's own variables (CLAUDECODE, CLAUDE_CODE_*,
# CLAUDE_AGENT_SDK_*, CLAUDE_PID, CLAUDE_EFFORT, ANTHROPIC_BASE_URL and
# every other CLAUDE* or ANTHROPIC_* one) are unset for claude, so a run
# from inside a session is not taken for part of it. Only ANTHROPIC_API_KEY
# and CLAUDE_CODE_OAUTH_TOKEN pass through, and CLAUDE_CONFIG_DIR is set.
#
# Needs: macOS, cargo, and the Claude command-line app (`claude`, or
# MDOC_CLAUDE=<path>). MDOC_REAL_MODEL picks the model (default
# claude-sonnet-5-5).
# shellcheck source=../lib.sh
source "$(dirname "$0")/../lib.sh"

keep=0
claude_config=
while [ $# -gt 0 ]; do
  case "$1" in
    --keep) keep=1 ;;
    --claude-config)
      [ $# -ge 2 ] || die "--claude-config needs a folder"
      claude_config=$2
      shift
      ;;
    *) die "unknown argument: $1 (usage: $0 [--keep] [--claude-config <dir>])" ;;
  esac
  shift
done

[ "$(uname -s)" = Darwin ] || die "this check runs on macOS only"
claude=${MDOC_CLAUDE:-$(command -v claude || true)}
[ -n "$claude" ] || die "no claude command-line app on PATH (or set MDOC_CLAUDE)"
model=${MDOC_REAL_MODEL:-claude-sonnet-5-5}
if [ -n "$claude_config" ]; then
  [ -d "$claude_config" ] || die "no folder $claude_config"
  # Physical paths, so a link to ~/.claude is refused too.
  claude_config=$(cd "$claude_config" && pwd -P)
  for own in "$HOME" "$HOME/.claude"; do
    if [ -d "$own" ] && [ "$claude_config" = "$(cd "$own" && pwd -P)" ]; then
      die "--claude-config must be a folder of its own, not $own"
    fi
  done
fi

# Short, so the connector's socket path stays under macOS's 104 bytes.
scratch=$(mktemp -d /tmp/mdoc-real-model.XXXXXX)
profile=$scratch/profile
config=${claude_config:-$scratch/claude-config}
transcript=$scratch/transcript.jsonl
mcp_config=$scratch/mcp.json
mkdir -p "$profile" "$config"
mdoc_pid=

# Stop mdoc, forcibly after ten seconds.
stop_mdoc() {
  [ -n "$mdoc_pid" ] || return 0
  kill "$mdoc_pid" 2>/dev/null || true
  for _ in $(seq 20); do
    kill -0 "$mdoc_pid" 2>/dev/null || break
    sleep 0.5
  done
  kill -9 "$mdoc_pid" 2>/dev/null || true
  wait "$mdoc_pid" 2>/dev/null || true
  mdoc_pid=
}

cleanup() {
  stop_mdoc
  if [ "$keep" = 0 ]; then
    rm -rf "$profile"
    [ -n "$claude_config" ] || rm -rf "$config"
    # Nothing to read when it stopped before running anything.
    rmdir "$scratch" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# claude, with no Claude Code session's variables and the config above.
run_claude() {
  (
    for name in $(compgen -e); do
      case "$name" in
        ANTHROPIC_API_KEY | CLAUDE_CODE_OAUTH_TOKEN) ;;
        CLAUDE* | ANTHROPIC_*) unset "$name" ;;
      esac
    done
    export CLAUDE_CONFIG_DIR=$config
    cd "$scratch"
    "$claude" "$@"
  )
}

echo "Scratch folder: $scratch"
if ! run_claude auth status --text </dev/null >/dev/null 2>&1; then
  die "the Claude config in $config is not logged in; pass --claude-config <dir> or set ANTHROPIC_API_KEY (see this script's header)"
fi

echo "Building mdoc, mdoc-mcp and the Vault helper…"
cargo build --locked -p mdoc -p mdoc-mcp
cargo build --locked -p mdoc-connector --example real_model
# Where cargo put them, even with a target folder set elsewhere: a stale
# mdoc that ignores the dev profile would open the real one.
target=$(cargo metadata --locked --no-deps --format-version 1 |
  sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
[ -n "$target" ] || die "cargo named no target folder"
bin=$target/debug
helper=$bin/examples/real_model
shim=$bin/mdoc-mcp
for built in "$bin/mdoc" "$shim" "$helper"; do
  [ -x "$built" ] || die "no $built after the build"
done
case "$shim$profile" in
  *\"* | *\\*) die "a path holds a quote or backslash, which the MCP config can't take: $shim" ;;
esac

echo "Seeding the dev profile…"
project=$(MDOC_DEV_PROFILE=$profile "$helper" seed)

echo "Starting mdoc on the dev profile…"
MDOC_DEV_PROFILE=$profile MDOC_DEV_AI_PROJECT=$project \
  "$bin/mdoc" --mdoc-connector-launch </dev/null >"$scratch/mdoc.log" 2>&1 &
mdoc_pid=$!
# The profile sits under /tmp, short enough for the socket's usual path;
# a longer one would move it to a short folder (#1311).
socket=$profile/data/mdoc/connector/mcp.sock
for _ in $(seq 120); do
  [ -S "$socket" ] && break
  kill -0 "$mdoc_pid" 2>/dev/null || die "mdoc ended early; see $scratch/mdoc.log"
  sleep 0.5
done
[ -S "$socket" ] || die "mdoc's connector didn't listen within a minute; see $scratch/mdoc.log"

cat >"$mcp_config" <<EOF
{"mcpServers": {"mdoc": {"type": "stdio", "command": "$shim", "args": [], "env": {"MDOC_DEV_PROFILE": "$profile"}}}}
EOF

prompt='Use only the mdoc tools. List the documents shared in mdoc, read the Ready lease, and answer: who rents the flat, from whom, for how much a month, and from when? Write every name exactly as the document writes it. Show your answer here, then deliver it to mdoc as a reply.'
tools=mcp__mdoc__list_documents,mcp__mdoc__get_document,mcp__mdoc__deliver_reply

echo "Running claude -p with $model…"
status=0
run_claude -p "$prompt" \
  --model "$model" \
  --mcp-config "$mcp_config" --strict-mcp-config \
  --setting-sources user \
  --tools "" --allowedTools "$tools" --permission-mode dontAsk \
  --no-session-persistence \
  --output-format stream-json --verbose \
  </dev/null >"$transcript" 2>"$scratch/claude.log" || status=$?

stop_mdoc

echo "Checking…"
if [ "$status" = 0 ]; then
  echo "PASS claude ended cleanly"
else
  echo "FAIL claude exited $status (see $scratch/claude.log)"
fi
checked=0
MDOC_DEV_PROFILE=$profile "$helper" check "$transcript" || checked=$?

echo "Transcript: $transcript"
if [ "$status" = 0 ] && [ "$checked" = 0 ]; then
  echo "PASS: the real-model check passed"
else
  echo "FAIL: the real-model check failed"
  exit 1
fi
