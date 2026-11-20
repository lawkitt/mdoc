# In-app updates on macOS

Status: Accepted, 2026-10-11.

## Why

A fix helps only once it is installed, and users rarely download one by
hand. Apple Silicon macOS installs updates in the app; Windows only detects
them and offers View release. The feed's tag is a claim; the bundle's
signature and version are the facts: a downloaded `.app` installs only after
`codesign`, Team ID `F33DLM8K53`, `spctl` and version checks pass, once when
staged and again right before the swap. The swap runs in the synchronous
part of `on_app_quit`, because GPUI gives asynchronous quit work only
200 ms, and every failure restores the previous bundle. Nothing downloads
without a click, and the automatic version check sends no document content,
path or setting. An install that can't update itself (translocated, on a
disk image, read-only) gets View release, never an admin prompt.

## Rejected

- **Sparkle, cargo-packager-updater or a release-pipeline change**: mdoc
  ports only the pieces it needs into `src/updater.rs`: gpui-updater's
  stage-and-rename swap, OxiMux's verification gates and Zed's status
  machine.
- **Installing on Windows now**: it needs signed Windows builds and a
  handoff after exit for the NSIS installer.

## Revisit when

Windows builds are signed. A fix to the updater itself reaches users only
by a manual download.
