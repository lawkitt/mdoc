- **The merge gate survives GitHub errors and missing runners.** Every
  GitHub call in `scripts/merge-gate.sh` is time-limited and retried, and a
  pass that skips a PR or queue commit ends red and runs again 3 minutes
  later (`merge-gate-recheck.yml`); a hosted job that never got a runner is
  re-run up to twice before it fails as "no macOS runner"; a green PR that
  auto-merge leaves out of the queue gets a warning; the gate reads a queue
  commit's own area table; its job is now "merge-gate pass"
  ([ADR 0100](docs/adr/0100-ci-reliability-after-the-cutover.md)).
