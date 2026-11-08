#!/usr/bin/env bash
# Usage: scripts/test-ios-release.sh
# The iOS TestFlight scripts (ADR 0107) without Apple or GitHub: What to
# Test's checks (scripts/ios-what-to-test.sh), `just release-ios` and the
# workflow's tag checks on scratch repositories with a local origin
# (scripts/ios-release.sh), the key file from a throwaway key made here
# (scripts/ios-asc-key.sh), and the App Store Connect steps against a local
# fake of the API (scripts/ios-testflight.py). No network. Bash 3.2.
source "$(dirname "$0")/lib.sh"
root=$(pwd)
tmp=$(mktemp -d)
mock_pid=
# stop_fake: stops the fake API, quietly (bash 3.2 reports a killed job).
stop_fake() {
  [ -n "$mock_pid" ] || return 0
  kill "$mock_pid" 2>/dev/null || true
  wait "$mock_pid" 2>/dev/null || true
  mock_pid=
}
cleanup() {
  stop_fake
  rm -rf "$tmp"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
# refuses <what> <command...>: the command must fail.
refuses() {
  local what=$1
  shift
  if "$@" >"$tmp/out" 2>&1; then fail "$what: passed"; fi
}

opening="Test with documents you are allowed to share, never client files; check a screenshot before you send it."

# --- What to Test ---------------------------------------------------------
wtt=$root/scripts/ios-what-to-test.sh
printf '%s\n\nNew in this build:\n- The review bar follows the finger.\n' "$opening" >"$tmp/good.txt"
bash "$wtt" check "$tmp/good.txt" || fail "a good What to Test failed"
printf 'Try the review bar.\n' >"$tmp/no-opening.txt"
refuses "no opening line" bash "$wtt" check "$tmp/no-opening.txt"
grep -q "must open with" "$tmp/out" || fail "no opening: $(cat "$tmp/out")"
printf '%s\n---\nmore\n' "$opening" >"$tmp/dashes.txt"
refuses "a --- line" bash "$wtt" check "$tmp/dashes.txt"
{
  echo "$opening"
  perl -e 'print "x" x 4000'
} >"$tmp/long.txt"
refuses "over 4,000 characters" bash "$wtt" check "$tmp/long.txt"
# 3,800 Cyrillic letters are 7,600 bytes but fit: characters are counted.
{
  echo "$opening"
  perl -CSD -e 'print "\x{0436}" x 3800'
} >"$tmp/cyrillic.txt"
bash "$wtt" check "$tmp/cyrillic.txt" || fail "characters were counted as bytes"
: >"$tmp/empty.txt"
refuses "an empty file" bash "$wtt" check "$tmp/empty.txt"

# --- just release-ios and the tag checks ----------------------------------
# A scratch repository with the files the checks read, pushed to a bare
# origin; the scripts run from inside it, as lib.sh moves to its root.
git init -q --bare "$tmp/origin.git"
repo=$tmp/repo
git init -q "$repo"
g() { git -C "$repo" -c user.name=t -c user.email=t@example.invalid "$@"; }
mkdir -p "$repo/scripts" "$repo/apps/ios/mdoc.xcodeproj" "$repo/apps/ios/Config"
cp scripts/lib.sh scripts/ios-release.sh scripts/ios-what-to-test.sh scripts/ios-gate.sh "$repo/scripts/"
pbx() { printf '\t\t\t\tMARKETING_VERSION = %s;\n\t\t\t\tMARKETING_VERSION = %s;\n' "$1" "$1" >"$repo/apps/ios/mdoc.xcodeproj/project.pbxproj"; }
key() { printf '<dict>\n\t<key>ITSAppUsesNonExemptEncryption</key>\n\t<%s/>\n</dict>\n' "$1" >"$repo/apps/ios/Config/mdoc-Info.plist"; }
pbx 0.2.0
key false
g add -A
g commit -q -m c1
g commit -q --allow-empty -m c2
g branch -q -M main
g remote add origin "$tmp/origin.git"
g push -q origin main
good=$(g rev-parse HEAD)

gates=$tmp/gates
mkdir -p "$gates"
# gate <commit> <tier> [phone]: a gate record, on the iPhone 12 Pro unless named.
gate() { printf 'phone=%s\nudid=0000\ntier=%s\ncommit=%s\npassed=2026-11-08T00:00:00Z\n' "${3:-iPhone13,3}" "$2" "$1" >"$gates/$1"; }
release() { (cd "$repo" && MDOC_GATE_DIR="$gates" bash scripts/ios-release.sh "$@"); }

refuses "no gate record" release 0.2.0-rc.3 "$tmp/good.txt" --yes
grep -q "no gate record" "$tmp/out" || fail "no gate: $(cat "$tmp/out")"
gate "$good" detector
refuses "a detector-tier gate" release 0.2.0-rc.3 "$tmp/good.txt" --yes
gate "$good" below-floor iPhone17,1
refuses "a below-floor run on a phone that meets the floor" release 0.2.0-rc.3 "$tmp/good.txt" --yes
grep -q "meets the floor" "$tmp/out" || fail "forced tier: $(cat "$tmp/out")"
gate "$good" below-floor
touch "$repo/apps/ios/Untracked.swift"
refuses "an untracked file" release 0.2.0-rc.3 "$tmp/good.txt" --yes
grep -q "untracked" "$tmp/out" || fail "untracked: $(cat "$tmp/out")"
rm "$repo/apps/ios/Untracked.swift"
refuses "a git error as clean" bash -c "cd '$repo' && GIT_INDEX_FILE=/dev/null bash scripts/ios-gate.sh clean"
refuses "a plain version" release 0.2.0 "$tmp/good.txt" --yes
refuses "another version" release 0.3.0-rc.1 "$tmp/good.txt" --yes
grep -q "maintainer's" "$tmp/out" || fail "another version: $(cat "$tmp/out")"
refuses "bad What to Test" release 0.2.0-rc.3 "$tmp/no-opening.txt" --yes

# A commit not on origin/main, and one with the export key YES.
g checkout -q -b side
g commit -q --allow-empty -m side
gate "$(g rev-parse HEAD)" below-floor
refuses "a commit not on main" release 0.2.0-rc.3 "$tmp/good.txt" --yes
grep -q "not on origin/main" "$tmp/out" || fail "off main: $(cat "$tmp/out")"
g checkout -q main
key true
g commit -q -am yes
g push -q origin main
gate "$(g rev-parse HEAD)" below-floor
refuses "the export key YES" release 0.2.0-rc.3 "$tmp/good.txt" --yes
grep -q "ITSAppUsesNonExemptEncryption" "$tmp/out" || fail "export key: $(cat "$tmp/out")"

# HEAD behind origin/main's tip: refused unless the commit is named.
g checkout -q "$good"
refuses "a stale HEAD" release 0.2.0-rc.3 "$tmp/good.txt" --yes
grep -q "not origin/main's tip" "$tmp/out" || fail "stale HEAD: $(cat "$tmp/out")"
g checkout -q main

# The good commit: tagged and pushed with What to Test and the trailers.
release 0.2.0-rc.3 "$tmp/good.txt" --commit "$good" --yes >"$tmp/out" 2>&1 || fail "release failed: $(cat "$tmp/out")"
[ "$(git -C "$tmp/origin.git" rev-parse 'refs/tags/ios-v0.2.0-rc.3^{commit}')" = "$good" ] || fail "the tag is not on origin at $good"
message=$(g for-each-ref --format='%(contents)' refs/tags/ios-v0.2.0-rc.3)
[ "$(echo "$message" | head -1)" = "mdoc for iOS 0.2.0 rc.3 (build 2)" ] || fail "subject: $(echo "$message" | head -1)"
echo "$message" | grep -qx "Commit: $good" || fail "no commit trailer: $message"
echo "$message" | grep -q "^Gate: just ios-device-test, below-floor tier, on iPhone13,3" || fail "no gate trailer: $message"
(cd "$repo" && bash scripts/ios-what-to-test.sh from-tag ios-v0.2.0-rc.3) >"$tmp/from-tag.txt"
cmp -s "$tmp/from-tag.txt" "$tmp/good.txt" || fail "from-tag: $(cat "$tmp/from-tag.txt")"
refuses "a tag that exists" release 0.2.0-rc.3 "$tmp/good.txt" --commit "$good" --yes
grep -q "never reused" "$tmp/out" || fail "reuse: $(cat "$tmp/out")"
release 0.2.0-rc.4 "$tmp/good.txt" --commit "$good" --no-push >/dev/null 2>&1 || fail "--no-push failed"
[ -z "$(git -C "$tmp/origin.git" tag -l ios-v0.2.0-rc.4)" ] || fail "--no-push pushed"

# The workflow's checks.
(cd "$repo" && bash scripts/ios-release.sh --check-tag ios-v0.2.0-rc.3) >"$tmp/check" 2>&1 || fail "--check-tag failed: $(cat "$tmp/check")"
if ! { grep -qx "version=0.2.0" "$tmp/check" && grep -qx "build=2" "$tmp/check" && grep -qx "commit=$good" "$tmp/check"; }; then
  fail "--check-tag printed: $(cat "$tmp/check")"
fi
g tag light "$good"
refuses "a lightweight tag" bash -c "cd '$repo' && bash scripts/ios-release.sh --check-tag light"
g tag -a -m "x" ios-v0.3.0-rc.1 "$good"
refuses "a tag of another version" bash -c "cd '$repo' && bash scripts/ios-release.sh --check-tag ios-v0.3.0-rc.1"
g tag -a -m "mdoc" -m "Try it." ios-v0.2.0-rc.9 "$good"
refuses "a tag without the opening line" bash -c "cd '$repo' && bash scripts/ios-release.sh --check-tag ios-v0.2.0-rc.9"
g checkout -q "$good"
(cd "$repo" && bash scripts/ios-release.sh --check-main) >"$tmp/check" 2>&1 || fail "--check-main failed: $(cat "$tmp/check")"
grep -qx "build=2" "$tmp/check" || fail "--check-main printed: $(cat "$tmp/check")"

# The tiers by model (ADR 0088).
for pair in iPhone13,3:below-floor iPhone15,2:below-floor iPhone16,1:detector iPhone18,2:detector; do
  [ "$(bash scripts/ios-gate.sh tier "${pair%%:*}")" = "${pair#*:}" ] || fail "tier of ${pair%%:*}"
done
refuses "an iPad model" bash scripts/ios-gate.sh tier iPad16,3

# --- The key file ---------------------------------------------------------
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out "$tmp/ec.pem" 2>/dev/null
openssl pkcs8 -topk8 -nocrypt -in "$tmp/ec.pem" -out "$tmp/full.p8"
body=$(sed -e '/-----/d' "$tmp/full.p8" | tr -d '\n')
path=$(ASC_KEY_ID=ABC123 ASC_KEY_P8="$body" bash scripts/ios-asc-key.sh "$tmp/keys") || fail "a body-only key failed"
[ "$path" = "$tmp/keys/AuthKey_ABC123.p8" ] || fail "key path: $path"
cmp -s "$path" "$tmp/full.p8" || fail "the body did not make the same PEM"
ASC_KEY_ID=ABC123 ASC_KEY_P8="$(cat "$tmp/full.p8")" bash scripts/ios-asc-key.sh "$tmp/keys2" >/dev/null || fail "a full PEM failed"
cmp -s "$tmp/keys2/AuthKey_ABC123.p8" "$tmp/full.p8" || fail "the full PEM changed"
refuses "a key that is not base64" env ASC_KEY_ID=ABC123 ASC_KEY_P8='not a key!' bash scripts/ios-asc-key.sh "$tmp/keys3"
refuses "base64 that is no key" env ASC_KEY_ID=ABC123 ASC_KEY_P8=aGVsbG8= bash scripts/ios-asc-key.sh "$tmp/keys4"
if grep -q "$body" "$tmp/out"; then fail "the key's body was printed"; fi

# --- App Store Connect steps against a fake API ---------------------------
cat >"$tmp/mock.py" <<'PY'
import json, socketserver, sys
from http.server import BaseHTTPRequestHandler, HTTPServer
state = json.load(open(sys.argv[2]))
log = open(sys.argv[3], "a")
polls = {"n": 0}
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def send(self, body, code=200):
        data = json.dumps(body).encode()
        self.send_response(code); self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data))); self.end_headers(); self.wfile.write(data)
    def body(self):
        n = int(self.headers.get("Content-Length") or 0)
        return json.loads(self.rfile.read(n)) if n else None
    def do_GET(self):
        p = self.path.split("?")[0]
        assert self.headers["Authorization"] == "Bearer test-token"
        log.write("GET " + self.path + "\n"); log.flush()
        if p == "/v1/apps": return self.send({"data": [{"id": "6830176855", "attributes": {"name": "mdoc"}}]})
        if p == "/v1/builds":
            polls["n"] += 1
            if polls["n"] == 1: return self.send({"data": []})
            st = "PROCESSING" if polls["n"] == 2 else state["processing"]
            return self.send({"data": [{"id": "B1", "attributes": {"processingState": st}}]})
        if p == "/v1/builds/B1/betaBuildLocalizations": return self.send({"data": [], "links": {}})
        if p == "/v1/builds/B1/buildBetaDetail": return self.send({"data": {"attributes": {"internalBuildState": state["internal"]}}})
        if p == "/v1/betaGroups": return self.send({"data": [{"id": "G1", "attributes": {"name": "Maintainer", "isInternalGroup": True}}]})
        if p == "/v1/betaGroups/G1/relationships/builds":
            return self.send({"data": [{"id": b, "type": "builds"} for b in state["group"]], "links": {}})
        self.send({"errors": [{"code": "NOT_FOUND", "detail": p}]}, 404)
    def do_POST(self):
        b = self.body(); log.write("POST " + self.path + " " + json.dumps(b) + "\n"); log.flush()
        if self.path == "/v1/betaGroups/G1/relationships/builds":
            state["group"].append("B1"); self.send_response(204); self.end_headers(); return
        self.send({"data": {"id": "L1"}}, 201)
    def do_PATCH(self):
        b = self.body(); log.write("PATCH " + self.path + " " + json.dumps(b) + "\n"); log.flush()
        self.send({"data": {"id": "L1"}})
class Server(HTTPServer):
    # No reverse lookup of the address (HTTPServer's getfqdn can hang offline).
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]
server = Server(("127.0.0.1", 0), H)
with open(sys.argv[1], "w") as f:
    f.write(str(server.server_port))
server.serve_forever()
PY
# fake <processing> <internal> <group builds as JSON>: starts the fake API.
fake() {
  stop_fake
  rm -f "$tmp/port" "$tmp/api.log"
  printf '{"processing": "%s", "internal": "%s", "group": %s}' "$1" "$2" "$3" >"$tmp/state.json"
  python3 "$tmp/mock.py" "$tmp/port" "$tmp/state.json" "$tmp/api.log" &
  mock_pid=$!
  local n=0
  while [ ! -s "$tmp/port" ]; do
    n=$((n + 1))
    [ "$n" -lt 100 ] || fail "the fake API did not start"
    sleep 0.1
  done
}
asc() {
  ASC_TOKEN=test-token ASC_API_BASE="http://127.0.0.1:$(cat "$tmp/port")" ASC_POLL_SECONDS=0.05 ASC_GROUP_WAIT_SECONDS=0.5 \
    python3 scripts/ios-testflight.py "$@"
}

fake VALID IN_BETA_TESTING '["B1"]'
asc check-auth >"$tmp/out" 2>&1 || fail "check-auth failed: $(cat "$tmp/out")"
asc finish --version 0.2.0 --build 830 --notes "$tmp/good.txt" >"$tmp/out" 2>&1 || fail "finish failed: $(cat "$tmp/out")"
grep -q 'filter%5Bversion%5D=830' "$tmp/api.log" || fail "the build was not looked up by number: $(cat "$tmp/api.log")"
grep '^POST /v1/betaBuildLocalizations' "$tmp/api.log" | grep -q '"whatsNew": "Test with documents' ||
  fail "What to Test was not set: $(cat "$tmp/api.log")"
if grep -q '^POST /v1/betaGroups' "$tmp/api.log"; then fail "added a build automatic distribution had added"; fi

# Not in the group: the script adds it.
fake VALID READY_FOR_BETA_TESTING '[]'
asc finish --version 0.2.0 --build 830 --notes "$tmp/good.txt" >"$tmp/out" 2>&1 || fail "finish with an add failed: $(cat "$tmp/out")"
grep -q '^POST /v1/betaGroups/G1/relationships/builds' "$tmp/api.log" || fail "the build was not added to the group"

# Failures: Apple's processing ends INVALID; export compliance missing; a timeout.
fake INVALID IN_BETA_TESTING '["B1"]'
refuses "an INVALID build" asc finish --version 0.2.0 --build 830 --notes "$tmp/good.txt"
grep -q "ended INVALID" "$tmp/out" || fail "INVALID: $(cat "$tmp/out")"
fake VALID MISSING_EXPORT_COMPLIANCE '[]'
refuses "missing export compliance" asc finish --version 0.2.0 --build 830 --notes "$tmp/good.txt"
grep -q "export compliance" "$tmp/out" || fail "compliance: $(cat "$tmp/out")"
fake PROCESSING IN_BETA_TESTING '["B1"]'
refuses "a build that never finishes" asc finish --version 0.2.0 --build 830 --notes "$tmp/good.txt" --timeout 1
grep -q "timed out" "$tmp/out" || fail "timeout: $(cat "$tmp/out")"

echo "ios-release: ok"
