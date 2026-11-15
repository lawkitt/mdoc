- **Faster desktop CI.** `ci/macos` runs as two parallel jobs, the checks
  and the test tiers, under a summary of that name; the fast tier on PR
  heads runs unit and scripts in parallel under `fast (ubuntu)`. A push to
  `main` runs lint and builds the tests without running them, including the
  iOS and Android targets of `just mobile --ffi`, so the merge queue
  restores them from cache; Linux package installs retry once within 5
  minutes ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).
