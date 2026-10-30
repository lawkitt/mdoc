- **Windows ships the `.exe` installer only.** Releases no longer include an
  `.msi`, and `just setup` no longer installs WiX. If you installed mdoc from
  an `.msi`, remove it in Settings › Apps before running the `.exe`; your
  cases are kept, because the `.msi` never touches `%LOCALAPPDATA%`
  ([ADR 0083](docs/adr/0083-fewer-release-candidates.md)).
