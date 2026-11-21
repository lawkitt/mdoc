- **Branch iOS runs test what they touched.** A `ci-ios.yml` dispatch on a
  branch runs, by default, the UI shards holding the UI test classes it
  changed against `main`, else the unit part (`parts: all` or a list such
  as `unit,ui 2` picks others), one dispatch at a time; the queue and the
  nightly still run every part, and only a run of every part is `ci/ios`
  (#1041).
