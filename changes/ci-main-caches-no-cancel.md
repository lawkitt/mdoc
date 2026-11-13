- **CI: `main`'s cache builds are no longer cancelled by the next merge.**
  A run cancelled while saving left the iOS core's cache key reserved and
  never written, so every iOS job rebuilt the core; `main-caches.yml` and
  `ci.yml` on `main` now let a running build finish.
