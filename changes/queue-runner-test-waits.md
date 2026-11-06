- **The queue runner's test waits for its runners, not the clock.** Every
  wait in `scripts/test-queue-runner.sh` ends once its condition holds,
  within 15 minutes (`QUEUE_TEST_WAIT_S`), so a loaded PC no longer drops
  queue entries on its 5-minute limit; a runner that misses the limit fails
  once, with its log's tail, and the checks that depend on it are skipped
  (fixes #661). Its fake `tailscale` runs once before the runners, so a
  developer Mac's first-run check of a new program no longer reads as /ci
  not served (fixes #658).
