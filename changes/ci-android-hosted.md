- **A hosted Android job, merge-gate's Android fallback.** `ci-android.yml`
  (check `ci/android (hosted)`, 40 minutes) runs `just android` on
  `ubuntu-latest` with KVM; `merge-gate` dispatches it on a queue commit
  when the PC is silent, stale or erring, or the group holds a fork or bot
  PR, so a group that needs `ci/android` no longer fails with the PC off
  ([ADR 0095](docs/adr/0095-ci-on-a-merge-queue.md),
  [ADR 0100](docs/adr/0100-ci-reliability-after-the-cutover.md)).
