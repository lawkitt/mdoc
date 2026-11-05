- **The merge gate no longer fails on a missing Android fallback.** After
  the gate's reliability change, counting the manual runs of
  `ci-android.yml`, which does not exist yet, failed every pass on a queue
  commit that needs `ci/android`; a 404 there now means none
  ([ADR 0100](docs/adr/0100-ci-reliability-after-the-cutover.md)).
