- **The iOS UI shards split by measured time.** `scripts/ios-ui-shards.sh`
  packs the UI test classes into `ci/ios (ui 1)` and `(ui 2)`, longest
  first, by `apps/ios/ui-class-times.tsv`, each class's median time over the
  green UI parts of the last 14 days (a new class counts as the median).
  Each green UI part uploads its class times, and the nightly refreshes the
  table on the branch `nightly/ui-class-times`, never on `main`
  ([ADR 0113](docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).
