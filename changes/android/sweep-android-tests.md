- **CI: two duplicate Android emulator tests go.** Home listing the library
  is already the first half of the long-press delete test, and a core that
  fails to open now checks its reason and the missing empty state in its
  one end-to-end test; the scaffold test class no longer opens the app
  ([ADR 0112](docs/adr/0112-ci-speed-after-the-first-fortnight.md)).
