- **iOS device UI tests.** `just ios-device-test <udid>` runs the review
  flow and a long-jobs test on a connected iPhone, through the real
  background task the Simulator refuses: a camera scan downloads text
  recognition on Wi-Fi and is recognized while the app is left, Find names
  runs the detector, and a .docx and a text PDF come in. The recipe copies
  the fixtures into the app's container with `devicectl` and fails on a new
  crash report ([ADR 0088](docs/adr/0088-ios-on-real-devices.md)).
