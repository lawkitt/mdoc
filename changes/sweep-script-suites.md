- **CI: the queue runner's suite takes about 57 s instead of 96.** Its
  job over the time limit runs on a pass of its own with a 20 s limit,
  instead of waiting out the 60 s the heartbeat job needs. It was the only
  script suite over 60 s on Linux
  ([ADR 0112](docs/adr/0112-ci-speed-after-the-first-fortnight.md)).
