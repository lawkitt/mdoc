- **The nightly checks ADR 0112's CI targets.** `scripts/ci-stats.sh` now
  measures a queue commit's time to a green gate, the Windows job, the iOS UI
  job and the queue drops a PR did not cause, and the nightly opens "CI
  targets missed" after 3 nights that missed one, closing it on a night that
  meets them all
  ([ADR 0112](docs/adr/0112-ci-speed-after-the-first-fortnight.md)).
