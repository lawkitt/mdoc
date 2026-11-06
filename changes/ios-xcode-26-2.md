- **iOS CI stops downloading a Simulator runtime.** The pinned Xcode and
  iOS Simulator are now 26.2, which the hosted `macos-26` image ships, so
  `ci/ios` no longer fetches about 8 GB each run; building or testing the
  iOS app on a Mac needs Xcode 26.2 (fixes #630).
