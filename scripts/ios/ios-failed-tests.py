#!/usr/bin/env python3
"""Usage: scripts/ios/ios-failed-tests.py part <part> <status> <xcresult folder>
          scripts/ios/ios-failed-tests.py summary <result> <part>...
          scripts/ios/ios-failed-tests.py runner <xcresult folder>

A failed hosted iOS job names its failing tests, as the PC's
status does (scripts/queue-runner.sh): an error annotation
`Failed in ci/ios (<part>): <first test> and <n> more`, or
`Failed in ci/ios (<part>)` when it names none, and every name in the job
summary ($GITHUB_STEP_SUMMARY). A hosted UI test that failed and passed on
its one retry (`just ios-test` with MDOC_IOS_TEST_RETRY=1) keeps the job
green but is named too: a warning annotation
`Passed on retry in ci/ios (<part>): <test>, <test>...` (one a part, every
test), and a list in the job summary. A name is the quarantine list's `ios:<target>/<class>/<method>`
(.config/quarantine.txt), so the "same test elsewhere" rule and its
quarantine after two unrelated failures can read it
(scripts/ci/ci-stats.sh reads ci/ios's annotations).

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
         its names) and the core job's result in $CORE_RESULT: repeats each
         part's retried tests, then exits 0 when <result> is success,
         otherwise annotates the core job when it did not pass (its parts
         never ran), else each part that did not pass, with its tests, or
         its status when it did not fail (cancelled, or "did not finish"
         when it wrote none), and exits 1.
runner   in a UI part's Tests step (scripts/ios/ios-test-part.sh), after
         it failed: exits 0 when its test runner failed to start, so the
         step may run once more (#1065), otherwise 1. That is, the folder
         has an .xcresult, each reads, and its only test cases are the
         runner's own errors (xcresulttool's "System Failures" suite),
         `mdocUITests-Runner (<pid>) encountered an error`, each failure
         message of which says the runner crashed while preparing to run
         tests or failed to initialize for UI testing: no test case ran.
"""
import json
import os
import re
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


# A test runner's own error, which xcresulttool lists as a test case of its
# target's "System Failures" suite, and the failures it reports when it
# never started: run 44940648698's `ci/ios (ui 1)` hit both (#1065).
RUNNER_ERROR = re.compile(r"\S+-Runner \(\d+\) encountered an error")
RUNNER_START = ("crashed while preparing to run tests", "failed to initialize for UI testing")


def runner_failed(tree):
    """Whether the test runner failed to start: in xcresulttool's
    `test-results tests` JSON, every test case is a runner's own error
    (RUNNER_ERROR) with failure messages, each saying it never started
    (RUNNER_START), and there is at least one."""
    cases = []

    def walk(node):
        if node.get("nodeType") == "Test Case":
            cases.append(node)
            return
        for child in node.get("children", []):
            walk(child)

    for node in tree["testNodes"]:
        walk(node)
    for case in cases:
        if not RUNNER_ERROR.fullmatch(case.get("nodeIdentifier", "")):
            return False
        # Its failure messages, the only children it has.
        messages = [child.get("name", "") for child in case.get("children", [])]
        if not messages or not all(any(start in message for start in RUNNER_START) for message in messages):
            return False
    return bool(cases)


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
    some for scripts/ci/ci-stats.sh."""
    if not names:
        return
    print(f"::warning::{escape(f'Passed on retry in {job}: ' + ', '.join(names))}")
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a") as summary:
            summary.write(f"### {job}: passed on retry\n\n")
            summary.write(f"Failed, then passed on the one retry ({len(names)}); each counts as an unrelated failure toward quarantine:\n\n")
            summary.writelines(f"- `{name}`\n" for name in names)
            summary.write("\n")


# What reading an .xcresult's tests can raise: xcresulttool missing or
# failing, or JSON that is not its tree.
UNREADABLE = (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError, AttributeError)


def tests_tree(bundle):
    """xcresulttool's `test-results tests` JSON for an .xcresult."""
    out = subprocess.run(
        ["xcrun", "xcresulttool", "get", "test-results", "tests", "--path", str(bundle), "--compact"],
        check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(out)


def part(name, status, folder):
    failed, retried = set(), set()
    for bundle in sorted(Path(folder).glob("*.xcresult")):
        try:
            bad, again = test_results(tests_tree(bundle))
            failed.update(bad)
            retried.update(again)
        except UNREADABLE as error:
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
    core = os.environ.get("CORE_RESULT") or "success"
    if core != "success":
        report("ci/ios (core)", [], None if core == "failure" else core)
        return 1
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


def runner(folder):
    bundles = sorted(Path(folder).glob("*.xcresult"))
    try:
        never_started = bool(bundles) and all(runner_failed(tests_tree(bundle)) for bundle in bundles)
    except UNREADABLE:
        never_started = False
    return 0 if never_started else 1


def main(args):
    if len(args) == 4 and args[0] == "part":
        part(args[1], args[2], args[3])
        return 0
    if len(args) >= 3 and args[0] == "summary":
        return summary(args[1], args[2:])
    if len(args) == 2 and args[0] == "runner":
        return runner(args[1])
    sys.exit(__doc__.split("\n\n")[0])


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
