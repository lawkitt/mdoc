- **UI tests keep importing their seed at each launch.** A launch that
  copied a prebuilt library instead was tried on TableEditingUITests, the
  class with the most launches, on hosted CI: it saved under a second a
  launch (warm launches 12.4 s to the document row, 12.0 s with the copy),
  short of the 5 seconds that would have taken it to every class, so it
  was dropped. A launch's time is XCUITest starting the app, not the
  import ([ADR 0113](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).
