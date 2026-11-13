- **The detection gate runs on hosted Windows.** `ci/detection` moved from
  hosted `macos-26` to `windows-latest`, in 8 shards from the job pool
  Windows shares with Linux, so it no longer waits for one of the five macOS
  runners. Hosted Windows is now the reference platform: the 256 recognition
  snapshots were recorded there again (193 had moved past the 1e-4 score
  tolerance from the Mac's), `just detection-gate` runs on Windows, and
  agents on the Mac take refreshed snapshots from CI with `just
  snapshots-fetch`. `just recognize --check` prints each fixture's model time,
  and a manual run of the gate with `record` uploads every changed snapshot
  ([ADR 0113](docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).
