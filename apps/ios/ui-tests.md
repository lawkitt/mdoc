# mdoc iOS UI tests

The app's UI tests (`mdocUITests`) and the device tests. Building and running
them: [the app's README](README.md).

## Device UI tests

The Simulator refuses `BGContinuedProcessingTask`, so only a phone runs a
long job through the real background task. Run them before every TestFlight
build, with the phone connected, unlocked, online and with UI Automation on
(Settings → Developer). Turn Handoff off (Settings → General → AirPlay &
Continuity) for the run: the tests' Paste reads the clipboard, and a copy
on a Mac with the same Apple Account replaces it, which once pasted a Mac's
copy into a test. Each run downloads text recognition (18 MB); the
detector tier reuses the phone's own name detector or downloads it once
(about 630 MB):

```sh
just ios-device-test <hardware UDID>
just ios-device-test <hardware UDID> below-floor   # or detector: not by the phone's model
```

`scripts/ios/ios-device-test.sh` copies the fixtures into the app's container
with `devicectl` (the app ships none), runs `ReviewFlowUITests` and the
tier's test, and fails on a new `mdoc-*.ips` crash report. The tier comes
from the phone's model: every iPhone but the 3 GB SE 2nd generation runs
`DeviceLongJobsUITests` (real downloads, Find names while the test leaves
the app past iOS's grace); the SE, or `below-floor`, runs
`DeviceBelowFloorUITests`. Results go to `target/ios/device-test-*.xcresult`.

## Timing

Editing's budgets are a keystroke's restyle under 8 ms and a structural core
call under 16 ms on the 37-page legal opinion; `LivePreviewUITests` measures
both nightly (`TEST_RUNNER_MDOC_TIMING_TESTS=1`) without a threshold.

## How UI tests launch the app

UI tests launch the app with `-mdoc-ui-testing` (`LaunchConfiguration` reads
every flag once): its own Vault, Vault key and preferences, below the
detector floor. It imports `MDOC_SEED_FILE` and scans it with the structured
rules alone, the only such scan, so review needs no model. On a phone the
seed reads the copy `just ios-device-test` put in the container.
`MDOC_KEEP_LIBRARY=1` keeps the library across launches; `MDOC_OCR_TESTS=1`
runs the tests that download the OCR model (with `MDOC_REAL_DOWNLOADS=1`);
`MDOC_UI_DETECTOR_TIER=1` gives the detector tier. `MDOC_UI_READINESS`
(`changed` or `excluded`) makes every saved document read as changed since
its review or excluded from AI, states only a computer makes today.
`MDOC_MARKED_LIFETIME` (seconds) makes a marked copy leave the clipboard that
soon, so a test sees which copies are marked.
