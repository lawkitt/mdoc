#!/usr/bin/env python3
"""App Store Connect steps of an iOS TestFlight build (ADR 0107).

Usage:
  scripts/ios-testflight.py check-auth
  scripts/ios-testflight.py finish --version <x.y.z> --build <n> --notes <file>
                                   [--group Maintainer] [--timeout 2700]

check-auth  Proves the key works: finds the app by its bundle ID.
finish      After the upload: waits until Apple has processed the build
            (fails on FAILED or INVALID, or after --timeout seconds), sets
            its What to Test (en-US) from --notes, then waits until the build
            is in the internal group --group, adding it there if automatic
            distribution has not.

Auth: ASC_KEY_ID, ASC_ISSUER_ID and ASC_KEY_PATH (a .p8 file); the token
comes from `xcrun altool --generate-jwt` and is renewed every 10 minutes.
ASC_TOKEN, when set, is used as is, and ASC_API_BASE replaces the API's
address (scripts/test-ios-release.sh uses both). Prints no secret.
Standard library only.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

BUNDLE_ID = "io.github.lawkitt.mdoc"
LOCALE = "en-US"
POLL_SECONDS = float(os.environ.get("ASC_POLL_SECONDS", "30"))


class Fail(Exception):
    pass


class Api:
    def __init__(self):
        self.base = os.environ.get("ASC_API_BASE", "https://api.appstoreconnect.apple.com").rstrip("/")
        self.token = None
        self.token_time = 0.0

    def bearer(self):
        fixed = os.environ.get("ASC_TOKEN")
        if fixed:
            return fixed
        if self.token is None or time.time() - self.token_time > 600:
            self.token = generate_jwt()
            self.token_time = time.time()
        return self.token

    def call(self, method, path, body=None, params=None):
        url = self.base + path
        if params:
            url += "?" + urllib.parse.urlencode(params)
        data = json.dumps(body).encode() if body is not None else None
        for attempt in range(4):
            request = urllib.request.Request(url, data=data, method=method)
            request.add_header("Authorization", "Bearer " + self.bearer())
            if data is not None:
                request.add_header("Content-Type", "application/json")
            try:
                with urllib.request.urlopen(request, timeout=60) as response:
                    text = response.read()
                    return json.loads(text) if text else {}
            except urllib.error.HTTPError as error:
                detail = error.read().decode(errors="replace")
                if error.code >= 500 and attempt < 3:
                    time.sleep(5 * (attempt + 1))
                    continue
                raise Fail(f"{method} {path}: HTTP {error.code}: {apple_errors(detail)}")
            except (urllib.error.URLError, TimeoutError) as error:
                if attempt < 3:
                    time.sleep(5 * (attempt + 1))
                    continue
                raise Fail(f"{method} {path}: {error}")
        raise Fail(f"{method} {path}: no answer")

    def all(self, path, params=None):
        """Every item of a paged list."""
        page = self.call("GET", path, params=params)
        items = list(page.get("data", []))
        while page.get("links", {}).get("next"):
            nxt = urllib.parse.urlsplit(page["links"]["next"])
            page = self.call("GET", nxt.path, params=dict(urllib.parse.parse_qsl(nxt.query)))
            items.extend(page.get("data", []))
        return items


def apple_errors(text):
    try:
        errors = json.loads(text).get("errors", [])
        return "; ".join(f"{e.get('code')}: {e.get('detail') or e.get('title')}" for e in errors) or text[:500]
    except ValueError:
        return text[:500]


def generate_jwt():
    names = ("ASC_KEY_ID", "ASC_ISSUER_ID", "ASC_KEY_PATH")
    missing = [n for n in names if not os.environ.get(n)]
    if missing:
        raise Fail("missing " + ", ".join(missing))
    result = subprocess.run(
        ["xcrun", "altool", "--generate-jwt", "--apiKey", os.environ["ASC_KEY_ID"],
         "--apiIssuer", os.environ["ASC_ISSUER_ID"], "--p8-file-path", os.environ["ASC_KEY_PATH"]],
        capture_output=True, text=True)
    found = re.findall(r"[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+", result.stdout + result.stderr)
    tokens = [t for t in found if t.startswith("eyJ")]
    if result.returncode != 0 or not tokens:
        raise Fail(f"altool --generate-jwt failed (exit {result.returncode})")
    return tokens[-1]


def log(message):
    print(message, flush=True)


def app_id(api):
    apps = api.call("GET", "/v1/apps", params={"filter[bundleId]": BUNDLE_ID, "limit": "1"})["data"]
    if not apps:
        raise Fail(f"no app with bundle ID {BUNDLE_ID} in App Store Connect")
    return apps[0]["id"], apps[0]["attributes"].get("name", "")


def wait_until(deadline, what, probe):
    """Calls probe() until it returns a value; fails at the deadline."""
    while True:
        value = probe()
        if value is not None:
            return value
        if time.time() > deadline:
            raise Fail(f"timed out waiting for {what}")
        time.sleep(POLL_SECONDS)


def processed_build(api, app, version, build, deadline):
    def probe():
        builds = api.call("GET", "/v1/builds", params={
            "filter[app]": app, "filter[version]": build,
            "filter[preReleaseVersion.version]": version, "limit": "1"})["data"]
        if not builds:
            log(f"build {build} not in App Store Connect yet")
            return None
        state = builds[0]["attributes"].get("processingState")
        log(f"build {build}: {state}")
        if state in ("FAILED", "INVALID"):
            raise Fail(f"Apple's processing of build {build} ended {state}; see App Store Connect and its email")
        return builds[0]["id"] if state == "VALID" else None
    return wait_until(deadline, f"build {build} to be processed", probe)


def set_what_to_test(api, build_id, text):
    for item in api.all(f"/v1/builds/{build_id}/betaBuildLocalizations"):
        if item["attributes"].get("locale") == LOCALE:
            api.call("PATCH", f"/v1/betaBuildLocalizations/{item['id']}", {"data": {
                "type": "betaBuildLocalizations", "id": item["id"],
                "attributes": {"whatsNew": text}}})
            return
    api.call("POST", "/v1/betaBuildLocalizations", {"data": {
        "type": "betaBuildLocalizations",
        "attributes": {"locale": LOCALE, "whatsNew": text},
        "relationships": {"build": {"data": {"type": "builds", "id": build_id}}}}})


def in_group(api, group_id, build_id):
    return any(b["id"] == build_id for b in api.all(f"/v1/betaGroups/{group_id}/relationships/builds",
                                                    params={"limit": "200"}))


def ensure_in_group(api, app, build_id, name, deadline):
    groups = api.call("GET", "/v1/betaGroups", params={"filter[app]": app, "filter[name]": name})["data"]
    group = next((g for g in groups if g["attributes"].get("name") == name), None)
    if group is None:
        raise Fail(f"no beta group named {name}")
    if not group["attributes"].get("isInternalGroup"):
        raise Fail(f"{name} is not an internal group; agents stop at internal testers (ADR 0091)")

    def probe():
        detail = api.call("GET", f"/v1/builds/{build_id}/buildBetaDetail")["data"]["attributes"]
        state = detail.get("internalBuildState")
        log(f"internal testing: {state}")
        if state == "MISSING_EXPORT_COMPLIANCE":
            raise Fail("the build is missing export compliance; ITSAppUsesNonExemptEncryption should answer it (ADR 0097)")
        if state == "PROCESSING_EXCEPTION":
            raise Fail("App Store Connect reports a processing exception for the build")
        if in_group(api, group["id"], build_id):
            return True
        return None if state in (None, "PROCESSING") else False

    # Automatic distribution adds the build within minutes; give it ten
    # before adding it ourselves.
    soon = min(deadline, time.time() + 600)
    try:
        if wait_until(soon, f"the build to reach {name}", probe):
            return
    except Fail as error:
        if "timed out" not in str(error):
            raise
    log(f"adding the build to {name}")
    api.call("POST", f"/v1/betaGroups/{group['id']}/relationships/builds",
             {"data": [{"type": "builds", "id": build_id}]})
    if not in_group(api, group["id"], build_id):
        raise Fail(f"the build is not in {name} after adding it")


def summary(lines):
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a") as out:
            out.write("\n".join(lines) + "\n")
    for line in lines:
        log(line)


def main():
    parser = argparse.ArgumentParser(description="App Store Connect steps of an iOS TestFlight build (ADR 0107)")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("check-auth")
    finish = sub.add_parser("finish")
    finish.add_argument("--version", required=True)
    finish.add_argument("--build", required=True)
    finish.add_argument("--notes", required=True)
    finish.add_argument("--group", default="Maintainer")
    finish.add_argument("--timeout", type=int, default=2700)
    args = parser.parse_args()

    api = Api()
    try:
        app, name = app_id(api)
        if args.command == "check-auth":
            log(f"App Store Connect key works: {name} ({BUNDLE_ID}, app {app})")
            return 0
        with open(args.notes, encoding="utf-8") as f:
            notes = f.read().strip()
        deadline = time.time() + args.timeout
        build_id = processed_build(api, app, args.version, args.build, deadline)
        set_what_to_test(api, build_id, notes)
        log("What to Test set")
        ensure_in_group(api, app, build_id, args.group, deadline)
        summary([f"### mdoc for iOS {args.version} (build {args.build})", "",
                 f"Processed, What to Test set, and in the internal group {args.group}."])
        return 0
    except Fail as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
