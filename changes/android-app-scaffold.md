- **Android app scaffold.** `apps/android` is a Jetpack Compose app on the
  Rust core's Kotlin bindings: Home lists stored documents, the case key is
  wrapped by the Android Keystore, nothing goes into backups or device
  transfers, and parity tests check the core against the desktop goldens on
  the JVM. Windows local CI gains the Android job (`just android`)
  ([ADR 0089](docs/adr/0089-android-app.md)).
