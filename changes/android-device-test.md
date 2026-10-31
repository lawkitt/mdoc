- **Android device tests.** `just android-device-test` runs the Android
  app's tests and a long-job test on a phone by its adb serial or on
  Firebase Test Lab phones: the model download, text recognition and Find
  names go on while mdoc is off screen and resume after Android kills its
  process, with no crash or ANR
  ([ADR 0089](docs/adr/0089-android-app.md)).
