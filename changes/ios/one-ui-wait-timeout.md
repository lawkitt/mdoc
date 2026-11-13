- **UI tests wait 15 seconds for a state to appear.** One shared timeout,
  `MdocUITest.timeout`, replaces the 5-second waits for an element, a value
  or a state, giving a slow hosted runner room; waits for something to go
  away keep their shorter ones, and a look for something that may rightly
  never show keeps 5 seconds as `MdocUITest.probeTimeout`. The unused
  `MDOC_UI_RULES_FIND_NAMES` launch flag is gone
  ([ADR 0113](docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).
