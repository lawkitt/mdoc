- **CI: the queue runner's suite takes about 57 s instead of 96.** Its
  job over the time limit runs on a pass of its own with a 20 s limit,
  instead of waiting out the 60 s the heartbeat job needs. It was the only
  script suite over 60 s on Linux
  ([ADR 0112](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0112-ci-speed-after-the-first-fortnight.md)).
