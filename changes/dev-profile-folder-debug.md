- **A dev profile folder for debug builds.** A debug build on macOS or
  Windows started with `MDOC_DEV_PROFILE=<folder>` keeps its settings,
  session, Vault, models and a plain Vault key file in that folder, never
  asks the Keychain or Windows data protection, and says "dev profile" in
  its window title. Release builds ignore it
  ([#1153](https://github.com/lawkitt/mdoc/issues/1153)).
