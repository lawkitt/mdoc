- **Faster macOS screen renders, fewer render-file conflicts.** The headless
  render test takes 72 s instead of 234 s on the M4: `gpui-pre-apple`,
  `image` and `fdeflate` build at `opt-level` 2 in the dev profile (cold test
  build +3%), and PNGs are encoded on a background thread as on Windows.
  Scenes live in one module per feature and the test's screen list in
  `tests/headless_screens.txt`, merged by union (fixes #232).
