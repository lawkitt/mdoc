- **The iOS test sweep.** ADR 0112's rule applied to the iOS UI tests and
  the slow unit tests: four UI tests a unit test already asserts are gone
  (a new document's title, Find names again, one pane's missing row, the
  text ending at the bottom bar's own launch), four pairs that open the same
  screen share one launch, three keep only what the real screen shows, and
  `ReviewScrollTests` waits for the text to settle instead of a fixed time
  ([ADR 0112](docs/adr/0112-ci-speed-after-the-first-fortnight.md)).
