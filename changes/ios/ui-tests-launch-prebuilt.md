- **UI tests keep importing their seed at each launch.** A launch that
  copied a prebuilt library instead was tried on TableEditingUITests, the
  class with the most launches, on hosted CI: it saved under a second a
  launch (warm launches 12.4 s to the document row, 12.0 s with the copy),
  short of the 5 seconds that would have taken it to every class, so it
  was dropped. A launch's time is XCUITest starting the app, not the
  import ([ADR 0113](docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).
