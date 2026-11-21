- **CI: the iOS core builds once per run.** `ci/ios (core)` restores it
  from `main`'s cache or builds it, and the unit and UI parts download it as
  an artifact instead of each building it; its cache key hashes the Xcode
  that builds it.
