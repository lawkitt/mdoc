#!/usr/bin/env python3
"""Usage: scripts/ios-failed-tests.py part <part> <status> <xcresult folder>
          scripts/ios-failed-tests.py summary <result> <part>...

A failed hosted iOS job names its failing tests (ADR 0113), as the PC's
status does (scripts/queue-runner.sh): an error annotation
`Failed in ci/ios (<part>): <first test> and <n> more`, or
`Failed in ci/ios (<part>)` when it names none, and every name in the job
summary ($GITHUB_STEP_SUMMARY). A hosted UI test that failed and passed on
its one retry (`just ios-test` with MDOC_IOS_TEST_RETRY=1) keeps the job
green but is named too: a warning annotation
`Passed on retry in ci/ios (<part>): <test>, <test>...` (one a part, every
test), and a list in the job summary. A name is the quarantine list's `ios:<target>/<class>/<method>`
(.config/quarantine.txt), so ADR 0112's "same test elsewhere" rule and its
quarantine after two unrelated failures can read it
(scripts/ci-stats.sh reads ci/ios's annotations).

part     in a matrix part of ci-ios.yml, on its Mac, with the job's status
         so far: the failed and the retried test cases of each .xcresult
         in the folder (`xcrun xcresulttool get test-results tests`),
         sorted. Annotates the failed ones when <status> is not success and
         the retried ones always; writes `<key>_tests=<names>` and
         `<key>_retried=<names>` (the part without spaces, the names
         space-separated) for ci/ios, and `retried=<count>`, to
         $GITHUB_OUTPUT. A part that failed outside its tests (a build, the
         Release build), or whose xcresult cannot be read, names none.
summary  in ci/ios, with `needs.ios.outputs` as JSON in $IOS_OUTPUTS
         (`<key>`: the part's job status, `<key>_tests` and `<key>_retried`:
         its names): repeats each part's retried tests, then exits 0 when
         <result> is success, otherwise annotates each part that did not
         pass, with its tests, or its status when it did not fail
         (cancelled, or "did not finish" when it wrote none), and exits 1.
"""
import json
import os
import subprocess
import sys
from pathlib import Path


def test_results(tree):
    """`(failed, retried)`: the test cases in xcresulttool's `test-results
    tests` JSON that failed, and those that failed and passed on a retry.
    A retried case has a Failed and a Passed repetition under it (Xcode
    words the case itself Passed or Failed by version), so a case with a
    Passed repetition is never a failure."""
    failed, retried = set(), set()

    def repetitions(node):
        for child in node.get("children", []):
            if child.get("nodeType") == "Repetition":
                yield child.get("result")
            else:
                yield from repetitions(child)

    def walk(node, target):
        kind = node.get("nodeType", "")
        # "Unit test bundle" or "UI test bundle": its name is the target.
        if kind.endswith(" bundle"):
            target = node["name"]
        elif kind == "Test Case":
            # nodeIdentifier is `<class>/<method>()`.
            name = f"ios:{target}/{node['nodeIdentifier'].removesuffix('()')}"
            runs = set(repetitions(node))
            if "Failed" in runs and "Passed" in runs:
                retried.add(name)
            elif node.get("result") == "Failed":
                failed.add(name)
            return
        for child in node.get("children", []):
            walk(child, target)

    for node in tree["testNodes"]:
        walk(node, "?")
    # A case retried on one device and failed on another is a failure.
    return sorted(failed), sorted(retried - failed)


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


def report_retried(job, names):
    """One warning annotation naming every retried test, and a summary
    section: GitHub keeps only 10 warnings a step, so one a test would lose
    some for scripts/ci-stats.sh."""
    if not names:
        return
    print(f"::warning::{escape(f'Passed on retry in {job}: ' + ', '.join(names))}")
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a") as summary:
            summary.write(f"### {job}: passed on retry\n\n")
            summary.write(f"Failed, then passed on the one retry ({len(names)}); each counts as an unrelated failure toward quarantine (ADR 0113):\n\n")
            summary.writelines(f"- `{name}`\n" for name in names)
            summary.write("\n")


def part(name, status, folder):
    failed, retried = set(), set()
    for bundle in sorted(Path(folder).glob("*.xcresult")):
        try:
            out = subprocess.run(
                ["xcrun", "xcresulttool", "get", "test-results", "tests", "--path", str(bundle), "--compact"],
                check=True, capture_output=True, text=True,
            ).stdout
            bad, again = test_results(json.loads(out))
            failed.update(bad)
            retried.update(again)
        except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError, AttributeError) as error:
            detail = getattr(error, "stderr", None) or error
            print(f"::warning::{escape(f'could not read {bundle.name}: {detail}'.strip())}")
    failed, retried = sorted(failed), sorted(retried - failed)
    job = f"ci/ios ({name})"
    if status != "success":
        report(job, failed)
    report_retried(job, retried)
    path = os.environ.get("GITHUB_OUTPUT")
    if path:
        key = name.replace(" ", "")
        with open(path, "a") as output:
            output.write(f"{key}_tests={' '.join(failed)}\n")
            output.write(f"{key}_retried={' '.join(retried)}\n")
            output.write(f"retried={len(retried)}\n")


def summary(result, parts):
    outputs = json.loads(os.environ.get("IOS_OUTPUTS") or "{}")
    for name in parts:
        report_retried(f"ci/ios ({name})", (outputs.get(f"{name.replace(' ', '')}_retried") or "").split())
    if result == "success":
        return 0
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
    if len(args) == 4 and args[0] == "part":
        part(args[1], args[2], args[3])
        return 0
    if len(args) >= 3 and args[0] == "summary":
        return summary(args[1], args[2:])
    sys.exit(__doc__.split("\n\n")[0])


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
