# Shared helpers for scripts/*.sh. Sourced, never executed.
# shellcheck shell=bash

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

die() {
  echo "error: $*" >&2
  exit 1
}

# The [package] version in Cargo.toml — the single source of truth (ADR 0037).
version() {
  awk '
    /^\[/ { in_pkg = ($0 == "[package]") }
    in_pkg && /^version = / { gsub(/"/, "", $3); print $3; exit }
  ' Cargo.toml
}

is_semver() {
  [[ $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+(\.[0-9A-Za-z]+)*)?$ ]]
}

# Shipped targets (ADR 0037): macos-arm64 and windows-x64; anything else is
# reported as-is so callers can refuse it.
platform() {
  local os arch
  os=$(uname -s)
  arch=$(uname -m)
  case "$os/$arch" in
    Darwin/arm64) echo macos-arm64 ;;
    MINGW*/x86_64 | MSYS*/x86_64 | CYGWIN*/x86_64) echo windows-x64 ;;
    *) echo "$os-$arch" ;;
  esac
}

require_shipped_target() {
  case "$(platform)" in
    macos-arm64 | windows-x64) ;;
    *) die "$1 runs only on Apple Silicon macOS or Windows x64 (this is $(platform))" ;;
  esac
}

# y/N prompt; true only for y/Y. Strips the carriage return a Windows console
# appends, which would otherwise make every answer "no".
confirm() {
  local answer
  read -r -p "$1 [y/N] " answer || return 1
  answer=${answer%$'\r'}
  [[ $answer =~ ^[Yy]$ ]]
}

# Where the Android toolchain lives (ADR 0089): ~/Library/Android on the Mac;
# on Windows the E: Dev Drive's E:\Android when there is one, else
# %LOCALAPPDATA%\Android. MDOC_ANDROID_HOME overrides both.
android_home() {
  if [ -n "${MDOC_ANDROID_HOME:-}" ]; then
    echo "$MDOC_ANDROID_HOME"
  elif [ "$(platform)" = macos-arm64 ]; then
    echo "$HOME/Library/Android"
  elif [ -d /e ]; then
    echo /e/Android
  else
    echo "$(cygpath -u "$LOCALAPPDATA")/Android"
  fi
}

# A path native Windows programs (cargo, clang, Gradle) read: E:/Android/...
# on Windows, unchanged elsewhere.
native_path() {
  if command -v cygpath >/dev/null; then cygpath -m "$1"; else echo "$1"; fi
}

# sha <1|256> <file>: the file's SHA-1 or SHA-256 in hex.
sha() {
  if command -v "sha$1sum" >/dev/null; then
    "sha$1sum" "$2" | awk '{ print $1 }'
  else
    shasum -a "$1" "$2" | awk '{ print $1 }'
  fi
}

# curl flags for large pinned downloads: a stalled transfer (under 10 KB/s for
# a minute) is dropped and retried rather than waited on forever.
CURL_PATIENT=(--fail --silent --show-error --location --connect-timeout 30
  --speed-limit 10240 --speed-time 60 --retry 5 --retry-all-errors)

# fetch_pinned <url> <size> <sha1> <file>: downloads unless a verified copy is
# there, and checks the size and SHA-1 Google publishes for its Android
# archives (it publishes no SHA-256).
fetch_pinned() {
  if [ -f "$4" ] && [ "$(wc -c <"$4" | tr -d ' ')" = "$2" ] && [ "$(sha 1 "$4")" = "$3" ]; then
    return
  fi
  echo "downloading $(basename "$1") ($(($2 / 1048576)) MiB)" >&2
  curl "${CURL_PATIENT[@]}" --output "$4.part" "$1"
  [ "$(wc -c <"$4.part" | tr -d ' ')" = "$2" ] || die "$1: size mismatch"
  [ "$(sha 1 "$4.part")" = "$3" ] || die "$1: checksum mismatch"
  mv -f "$4.part" "$4"
}

# fetch_sha256 <url> <sha256> <file>: downloads unless a verified copy is there.
fetch_sha256() {
  if [ -f "$3" ] && [ "$(sha 256 "$3")" = "$2" ]; then
    return
  fi
  curl "${CURL_PATIENT[@]}" --output "$3.part" "$1"
  [ "$(sha 256 "$3.part")" = "$2" ] || die "$1 failed its checksum"
  mv -f "$3.part" "$3"
}

# Loads scripts/tools.env into the environment.
load_tools() {
  set -a
  # shellcheck source=tools.env
  source scripts/tools.env
  set +a
}

# --- areas (ADR 0091) ---------------------------------------------------------

# A GitHub filter glob as an anchored extended regex: ** crosses directories,
# * and ? do not.
glob_regex() {
  local glob=$1 re='' c i=0
  while [ "$i" -lt "${#glob}" ]; do
    c=${glob:i:1}
    if [ "${glob:i:3}" = '**/' ]; then
      re+='(.*/)?'
      i=$((i + 3))
      continue
    elif [ "${glob:i:2}" = '**' ]; then
      re+='.*'
      i=$((i + 2))
      continue
    fi
    case "$c" in
      '*') re+='[^/]*' ;;
      '?') re+='[^/]' ;;
      . | \\ | + | '(' | ')' | '|' | ^ | '$' | '{' | '}' | '[' | ']') re+="\\$c" ;;
      *) re+=$c ;;
    esac
    i=$((i + 1))
  done
  printf '^%s$' "$re"
}

# The areas the files on stdin touch, NUL-separated as `git diff -z` prints
# them (newline-separated output quotes unusual names, which would then match
# no line), by the area table given as text (scripts/ci-areas.txt): each
# file takes the areas of the table's first line that matches it, desktop
# when none does. A table with CRLF line endings reads the same. Prints them on one line in
# the order core, desktop, ios, android; core brings in every area. Nothing
# (an empty line) when every file is in no area (docs only); every area for
# an empty list of files.
changed_areas() { # table-text
  local areas glob f i match seen=' ' a out=''
  local -a line_areas=() line_res=()
  while read -r areas glob _; do
    case "$areas" in '' | '#'*) continue ;; esac
    [ -n "$glob" ] || die "area table: no glob after '$areas'"
    line_areas+=("$areas")
    line_res+=("$(glob_regex "$glob")")
  done <<<"${1//$'\r'/}"
  while IFS= read -r -d '' f; do
    [ -n "$f" ] || continue
    match=desktop
    for i in "${!line_res[@]}"; do
      if [[ $f =~ ${line_res[i]} ]]; then
        match=${line_areas[i]}
        break
      fi
    done
    seen+="${match//,/ } "
  done
  [ "$seen" != ' ' ] || seen=' core '
  [[ $seen != *' core '* ]] || seen=' core desktop ios android '
  for a in core desktop ios android; do
    [[ $seen != *" $a "* ]] || out+="${out:+ }$a"
  done
  echo "$out"
}

# True when the areas list ($2, from changed_areas) runs area $1's job.
affects() { [[ " $2 " == *" $1 "* ]]; }

# The areas between two commits, by this checkout's table.
diff_areas() { # from to
  local table
  table=$(cat scripts/ci-areas.txt)
  git diff --no-renames --name-only -z "$1" "$2" | changed_areas "$table"
}
