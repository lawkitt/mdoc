- **iOS CI builds the Release configuration.** `ci/ios` now passes only
  when `ci/ios (tests)` and the new `ci/ios-release` (`just ios-release`:
  the app unsigned in the Release configuration, as an archive compiles it)
  both pass, so a Release-only compile error fails the merge queue instead
  of the archive. The tests job downloads the Simulator runtime only when
  the image has none of the pinned major.minor, in the background while the
  core builds ([ADR 0100](docs/adr/0100-ci-reliability-after-the-cutover.md)).
