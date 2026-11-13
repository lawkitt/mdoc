#!/usr/bin/env python3
"""The UI test classes' measured times, which scripts/ios-ui-shards.sh packs
into ci-ios.yml's UI shards (ADR 0113).

Usage:
  scripts/ios-ui-class-times.py collect <xcresult folder>
  scripts/ios-ui-class-times.py fetch --days <n> <folder>
  scripts/ios-ui-class-times.py table <folder> [--days <n>]

collect  On the Mac, after a UI part passed: each class's summed test case
         durations in the .xcresult bundles in the folder (`xcrun
         xcresulttool get test-results tests`), one `<class>\t<seconds>`
         line each, sorted. ci-ios.yml uploads it as the artifact
         `ios-ui-times-<part>-<attempt>` (a few hundred bytes: the
         xcresults themselves, about 100 MB, upload only on a failure).
fetch    Downloads every such artifact of the last <n> days into the
         folder, one `<artifact id>.tsv` each (GH_REPO names the repository,
         GH_TOKEN reads it through `gh`).
table    The committed table (apps/ios/ui-class-times.tsv) from a folder of
         collected files: per class, the median of its times over the files
         that have it, in whole seconds, and how many there were; --days
         (14) is the window its header names.

Standard library only (and `gh` for fetch, `xcrun` for collect).
"""

import io
import json
import re
import statistics
import subprocess
import sys
import time
import zipfile
from datetime import datetime, timedelta, timezone
from pathlib import Path

ARTIFACT = re.compile(r"^ios-ui-times-ui[0-9]+-[0-9]+$")
HEADER = """\
# The UI test classes' times on hosted macos-26 (ADR 0113), which
# scripts/ios-ui-shards.sh packs into ci-ios.yml's UI shards, longest first,
# each into the shard with less so far. Per class: the median, over the green
# UI parts of the last {days} days (the `ios-ui-times-*` artifacts), of its
# test cases' summed durations in whole seconds, and how many parts measured
# it. Written by scripts/ios-ui-class-times.py; the nightly refreshes it
# (scripts/ios-ui-class-times.sh), replacing a row only with 3 parts or more.
# A class missing here counts as the median.
"""


def duration(node):
    """A test node's duration in seconds (`durationInSeconds`, or the
    `duration` text such as `1m 52s` from an older xcresulttool)."""
    if "durationInSeconds" in node:
        return float(node["durationInSeconds"])
    total = 0.0
    for amount, unit in re.findall(r"([0-9.]+)\s*(h|m|s)", node.get("duration", "")):
        total += float(amount) * {"h": 3600, "m": 60, "s": 1}[unit]
    return total


def class_times(tree):
    """{class: seconds} over the UI test bundles' test cases in a
    `test-results tests` JSON. A retried case's repetitions are its
    children; the case's own duration covers them."""
    times = {}

    def walk(node, ui):
        kind = node.get("nodeType", "")
        if kind.endswith(" bundle"):
            ui = kind == "UI test bundle"
        elif kind == "Test Case":
            if ui:
                cls = node.get("nodeIdentifier", "").split("/")[0]
                if cls:
                    times[cls] = times.get(cls, 0.0) + duration(node)
            return
        for child in node.get("children", []):
            walk(child, ui)

    for node in tree.get("testNodes", []):
        walk(node, False)
    return times


def collect(folder):
    bundles = sorted(Path(folder).glob("*.xcresult"))
    if not bundles:
        sys.exit(f"error: no .xcresult in {folder}")
    times = {}
    for bundle in bundles:
        out = subprocess.run(
            ["xcrun", "xcresulttool", "get", "test-results", "tests", "--path", str(bundle)],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
        for cls, seconds in class_times(json.loads(out)).items():
            times[cls] = times.get(cls, 0.0) + seconds
    if not times:
        sys.exit(f"error: no UI test case in {folder}")
    for cls in sorted(times):
        print(f"{cls}\t{times[cls]:.1f}")


def gh(*args, binary=False):
    """`gh api` with 3 tries, 10 s apart."""
    for attempt in range(3):
        result = subprocess.run(["gh", "api", *args], capture_output=True)
        if result.returncode == 0:
            return result.stdout if binary else json.loads(result.stdout)
        if attempt == 2:
            sys.exit(f"error: gh api {' '.join(args)}: {result.stderr.decode(errors='replace').strip()}")
        time.sleep(10)


def fetch(days, folder):
    out = Path(folder)
    out.mkdir(parents=True, exist_ok=True)
    cutoff = datetime.now(timezone.utc) - timedelta(days=days)
    count = 0
    # Newest first: stop at the first page that reaches past the cutoff.
    for page in range(1, 101):
        listing = gh(f"repos/{{owner}}/{{repo}}/actions/artifacts?per_page=100&page={page}")
        artifacts = listing.get("artifacts", [])
        for a in artifacts:
            created = datetime.fromisoformat(a["created_at"].replace("Z", "+00:00"))
            if created < cutoff or a.get("expired") or not ARTIFACT.match(a["name"]):
                continue
            data = gh(f"repos/{{owner}}/{{repo}}/actions/artifacts/{a['id']}/zip", binary=True)
            with zipfile.ZipFile(io.BytesIO(data)) as z:
                names = [n for n in z.namelist() if n.endswith(".tsv")]
                if len(names) != 1:
                    print(f"warning: artifact {a['id']} ({a['name']}) holds {len(names)} .tsv files; skipped", file=sys.stderr)
                    continue
                (out / f"{a['id']}.tsv").write_bytes(z.read(names[0]))
                count += 1
        if len(artifacts) < 100 or any(
            datetime.fromisoformat(a["created_at"].replace("Z", "+00:00")) < cutoff for a in artifacts
        ):
            break
    print(f"fetched {count} UI parts' times from the last {days} days", file=sys.stderr)


def table(folder, days):
    samples = {}
    files = sorted(Path(folder).glob("*.tsv"))
    for path in files:
        for n, line in enumerate(path.read_text().splitlines(), 1):
            if not line.strip() or line.startswith("#"):
                continue
            fields = line.split("\t")
            try:
                cls, seconds = fields[0], float(fields[1])
            except (IndexError, ValueError):
                sys.exit(f"error: {path}:{n}: not `<class>\\t<seconds>`: {line}")
            samples.setdefault(cls, []).append(seconds)
    if not samples:
        sys.exit(f"error: no class times in {folder}")
    sys.stdout.write(HEADER.format(days=days))
    for cls in sorted(samples):
        print(f"{cls}\t{round(statistics.median(samples[cls]))}\t{len(samples[cls])}")


def main(argv):
    # LF on Windows too: the table is committed.
    sys.stdout.reconfigure(newline="\n")
    usage = __doc__.split("\n\n")[1]
    if len(argv) == 2 and argv[0] == "collect":
        collect(argv[1])
    elif len(argv) == 4 and argv[0] == "fetch" and argv[1] == "--days" and argv[2].isdigit():
        fetch(int(argv[2]), argv[3])
    elif len(argv) in (2, 4) and argv[0] == "table":
        days = 14
        if len(argv) == 4:
            if argv[2] != "--days" or not argv[3].isdigit():
                sys.exit(usage)
            days = int(argv[3])
        table(argv[1], days)
    else:
        sys.exit(usage)


if __name__ == "__main__":
    main(sys.argv[1:])
