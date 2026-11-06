- **iOS CI reuses main's built core.** `main` saves the built iOS core
  (`target/ios`), keyed on a hash of everything its build reads, and both
  `ci/ios` jobs restore it and skip `just ios-core` on an exact hit; a
  miss rebuilds it and fails if the build read a file the key does not hash
  (fixes #631).
