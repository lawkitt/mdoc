- **A desktop-only change skips the detection gate.** App-only files
  (`src/`, `vendor/`, `build/`, `tests/` outside the recognition fixtures)
  no longer select `ci/detection`, so such a queue commit runs `ci/macos`
  and `ci/windows` without detection shards; core, the recognition
  fixtures, `resources/`, the recognition crates and the gate's own script
  and workflow still run it
  ([ADR 0113](docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).
