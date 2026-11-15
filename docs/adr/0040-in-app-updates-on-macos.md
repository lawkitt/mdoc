# In-app updates on macOS

Status: Accepted, 2026-10-11.

## Decision

- **Scope:** Apple Silicon macOS installs updates in the app. Windows only
  detects them and offers View release, until signed Windows builds and a
  handoff after exit for the NSIS installer exist.
- **Own code** in `src/updater.rs`, porting selected pieces (the
  stage-and-rename swap from gpui-updater, verification gates from OxiMux,
  the status machine from Zed's `auto_update`) rather than adding Sparkle,
  cargo-packager-updater or a release-pipeline change.
- **Detection:** the newest GitHub Release is compared with
  `CARGO_PKG_VERSION` (semver) through one unauthenticated GET, at launch if
  enabled and on Check now.
- **Download:** `mdoc_<version>_arm64.dmg`, checked against the release's
  `SHA256SUMS`, redirected only to `github.com` and
  `*.githubusercontent.com`, with Cancel and no resume. A release missing
  either file is detection-only.
- **Verification** of the `.app` from the image: `codesign` integrity, bundle
  id `io.github.lawkitt.mdoc`, Team ID `F33DLM8K53`, `spctl` acceptance, and
  `CFBundleShortVersionString` equal to the release version. The feed's tag
  is a claim; the bundle's signature and version are the facts.
- **Staging and swap:** the verified app waits in
  `~/Library/Caches/io.github.lawkitt.mdoc/updates/`. The swap runs after the
  normal quit flow, in the synchronous part of `on_app_quit`, because GPUI
  gives asynchronous quit work only 200 ms: copy with `ditto` beside the
  installed bundle, verify again, rename the old one aside, rename the new
  one in, delete the old one, and rename it back on any failure.
- **Eligibility:** only a bundle signed by `F33DLM8K53`, not translocated, not
  on a disk image, in a writable folder updates itself; otherwise the update
  shows View release with a one-line reason and never an admin prompt.
- **Testing:** unit tests for asset selection, checksums and eligibility; an
  ignored end-to-end test with signed fixture bundles, including wrong-Team-ID
  and tampered ones; `just smoke-update` before a release.

## Invariants

- Nothing downloads without an explicit click; the only automatic request is
  the version check, which sends no document content, path or setting.
- The installed bundle is never modified while mdoc runs and never left
  missing: every swap failure restores the previous bundle.
- A bundle is installed only after both verifications pass, one when staged
  and one right before the swap.
- Swap failures are reported at the next launch and never retried silently.

## Consequences

Users reach a fix to the updater itself only by downloading by hand. The
Developer ID certificate also gates updates; a renewed certificate keeps the
Team ID and still verifies.

References: [full text before the docs doctor](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0040-in-app-updates-on-macos.md),
[interview](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/auto-update.md).
