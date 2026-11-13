- **CI targets by the kind of commit.** `scripts/ci-stats.sh` classifies
  each queue commit as core, desktop-only, iOS-only or other by the jobs it
  ran, and reports ADR 0113's targets: a median of 20 minutes for a core
  commit, 15 for a desktop-only one, 25 for every commit and 15 for the
  slower iOS UI shard. Each target has a start date, so the nightly's issue
  judges only data from after the change it depends on.
