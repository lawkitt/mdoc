- **The iOS test sweep.** ADR 0112's rule applied to the iOS UI tests and
  the slow unit tests: three UI tests a unit test already asserts are gone
  (a new document's title, Find names again, one pane's missing row), four
  pairs that open the same screen share one launch, three keep only what
  the real screen shows, and `ReviewScrollTests` waits for the text to
  settle, laying it out as the screen would, instead of a fixed time (180 s
  to 115 s) ([ADR 0112](docs/adr/0112-ci-speed-after-the-first-fortnight.md)).
