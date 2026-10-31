- **iOS device UI tests below the floor.** `just ios-device-test <udid>`
  picks the test by the phone's tier: below the 8 GB detector floor (the
  iPhone 12 Pro) it checks the text-recognition download, a scan recognized
  while the app is left, a .docx import that is whole after it, the note
  where Find names would be and in Settings, and marking a name by selecting
  it; `below-floor` or `detector` picks one by hand
  ([ADR 0088](docs/adr/0088-ios-on-real-devices.md)).
