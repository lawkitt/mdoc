#!/usr/bin/env python3
"""Usage: scripts/ios-failed-tests.py part <part> <xcresult folder>
          scripts/ios-failed-tests.py summary <result> <part>...

A failed hosted iOS job names its failing tests (ADR 0113), as the PC's
status does (scripts/queue-runner.sh): an error annotation
`Failed in ci/ios (<part>): <first test> and <n> more`, or
`Failed in ci/ios (<part>)` when it names none, and every name in the job
summary ($GITHUB_STEP_SUMMARY). A name is the quarantine list's
`ios:<target>/<class>/<method>` (.config/quarantine.txt), so ADR 0112's
"same test elsewhere" rule can read it.

part     in a failed matrix part of ci-ios.yml, on its Mac: the failed test
         cases of each .xcresult in the folder (`xcrun xcresulttool get
         test-results tests`), sorted; writes `<key>_tests=<names>` (the part
         without spaces, the names space-separated) to $GITHUB_OUTPUT for
         ci/ios. A part that failed outside its tests (a build, the Release
         build), or whose xcresult cannot be read, names none.
summary  in ci/ios, with `needs.ios.outputs` as JSON in $IOS_OUTPUTS
         (`<key>`: the part's job status, `<key>_tests`: its names): exits
         0 when <result> is success, otherwise annotates each part that did
         not pass, with its tests, or its status when it did not fail
         (cancelled, or "did not finish" when it wrote none), and exits 1.
"""
import json
import os
import subprocess
import sys
from pathlib import Path


def failed_tests(tree):
    """The failed test cases in xcresulttool's `test-results tests` JSON."""
    names = set()

    def walk(node, target):
        kind = node.get("nodeType", "")
        # "Unit test bundle" or "UI test bundle": its name is the target.
        if kind.endswith(" bundle"):
            target = node["name"]
        elif kind == "Test Case":
            if node.get("result") == "Failed":
                # nodeIdentifier is `<class>/<method>()`.
                names.add(f"ios:{target}/{node['nodeIdentifier'].removesuffix('()')}")
            return
        for child in node.get("children", []):
            walk(child, target)

    for node in tree["testNodes"]:
        walk(node, "?")
    return sorted(names)


def escape(text):
    # A workflow command's data escapes %, CR and LF, % first.
    return text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def report(job, names, why=None):
    """One error annotation as the PC words its status, and a summary section."""
    line = f"Failed in {job}"
    if names:
        line += f": {names[0]}"
        if len(names) > 1:
            line += f" and {len(names) - 1} more"
    elif why:
        line = f"{job}: {why}"
    print(f"::error::{escape(line)}")
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a") as summary:
            summary.write(f"### {job}\n\n")
            if names:
                summary.write(f"Failing tests ({len(names)}):\n\n")
                summary.writelines(f"- `{name}`\n" for name in names)
            else:
                summary.write((why or "No failing test: it failed outside its tests, or its results could not be read.") + "\n")
            summary.write("\n")


def part(name, folder):
    names = set()
    for bundle in sorted(Path(folder).glob("*.xcresult")):
        try:
            out = subprocess.run(
                ["xcrun", "xcresulttool", "get", "test-results", "tests", "--path", str(bundle), "--compact"],
                check=True, capture_output=True, text=True,
            ).stdout
            names.update(failed_tests(json.loads(out)))
        except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError, AttributeError) as error:
            detail = getattr(error, "stderr", None) or error
            print(f"::warning::{escape(f'could not read {bundle.name}: {detail}'.strip())}")
    names = sorted(names)
    report(f"ci/ios ({name})", names)
    path = os.environ.get("GITHUB_OUTPUT")
    if path:
        with open(path, "a") as output:
            output.write(f"{name.replace(' ', '')}_tests={' '.join(names)}\n")


def summary(result, parts):
    if result == "success":
        return 0
    outputs = json.loads(os.environ.get("IOS_OUTPUTS") or "{}")
    bad = 0
    for name in parts:
        key = name.replace(" ", "")
        status = outputs.get(key) or ""
        if status == "success":
            continue
        bad += 1
        names = (outputs.get(f"{key}_tests") or "").split()
        # A part that failed reads as the PC's "Failed in"; any other
        # status (cancelled), or none, says so.
        why = None if names or status == "failure" else status or "did not finish"
        report(f"ci/ios ({name})", names, why)
    if not bad:
        report("ci/ios", [], f"parts {result}")
    return 1


def main(args):
    if len(args) == 3 and args[0] == "part":
        part(args[1], args[2])
        return 0
    if len(args) >= 3 and args[0] == "summary":
        return summary(args[1], args[2:])
    sys.exit(__doc__.split("\n\n")[0])


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
