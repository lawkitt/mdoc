- **The merge gate no longer waits forever on a job GitHub lost.** A queue
  commit's hosted check that is missing once its Queue run has completed
  (a summary job never created), and a job cancelled after it started (an
  internal error, a lost runner), are now infrastructure failures: re-run
  up to twice, then failed as "did not run" or "cancelled"; a job that hit
  its own timeout still fails at once (fixes #635,
  [ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).
