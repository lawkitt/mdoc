- **The iOS app builds with Xcode 27.2 and the iOS 27.2 SDK.** App Store
  Connect warned on build 1025 (ITMS-90725) that uploads need the iOS 27
  SDK from April 2027. `ci/ios` and the TestFlight upload now run on
  GitHub's `xcode-27` image and test on the iOS 27.2 Simulator it ships;
  building or testing the app on a Mac needs Xcode 27.2.
