# Auto-update: design interview

Status: complete, 2026-10-11; decisions recorded in
[ADR 0040](../adr/0040-in-app-updates-on-macos.md). Builds on
[ADR 0038](../adr/0038-update-check.md) (detection only) and
[ADR 0039](../adr/0039-macos-signing-and-notarization.md) (signed macOS builds,
which 0039 names as the trigger for reconsidering automatic updates).

## Starting point

- `src/updater.rs` (235 lines, ported from Zorite): one GET to the GitHub
  Releases API at launch and on **Check now**, SemVer comparison, pre-release
  handling, toolbar dot + **Settings → Updates** with **View release**. Nothing
  is downloaded.
- Release artifacts (ADR 0037/0039): `mdoc_<v>_arm64.dmg` (Developer ID
  signed, notarized and stapled; `.app` inside also stapled),
  `mdoc_<v>_x64-setup.exe` (NSIS, cargo-packager, unsigned),
  `mdoc_<v>_x64.msi` (stable only, WiX, `InstallScope=perMachine`, unsigned),
  `SHA256SUMS`, build-provenance attestations. Releases are drafts published
  by hand.

## Prior art survey (awesome-gpui apps + Zed)

84 app repositories in [awesome-gpui](https://github.com/zed-industries/awesome-gpui)
were scanned; about 45 have update code, most of it detection only (like
mdoc today). The ones that actually install:

| Project | Licence | macOS install | Windows install | Trust | Notes |
| --- | --- | --- | --- | --- | --- |
| **Zed** `crates/auto_update*` | GPL-3 (compatible with mdoc) | mount `.dmg`, `rsync --delete` over the running `.app`; restart | Inno Setup `/verysilent /update=true` stages into `{app}\updates`; `auto_update_helper.exe` swaps files with job/rollback list after exit (Restart Manager) | HTTPS to Zed's own server only | Polls hourly; private server; `Downloading{progress}`/`Installing`/`Updated` status machine; stale temp-dir cleanup; re-poll after system wake |
| **gpui-updater** (AprilNEA, vendored by GetCat; used by OpenLogi) | MIT/Apache | mount `.dmg` read-only, `ditto` to a sibling `.mdoc.app.new`, rename old aside, rename new in, roll back on failure; restart into the bundle path | bare `.exe` rename trick, or `.msi` staged via a `.cmd` run after exit (`msiexec /passive`) — **no NSIS support** | `SHA256SUMS` + optional minisign, policy `BestEffort/Checksum/Signature/Strict` | Same stack as mdoc (`ureq` 3 rustls, `semver`, `gpui-pre` 0.3). v0.2.0 (2026-10-11) split into `-core` + adapters and added Sparkle appcast/Ed25519 + a Sparkle driver |
| **OxiMux** `crates/auto-update` | (check) | download → mount → stage → verify; **swap on quit** | separate module | minisign-signed manifest **plus** `codesign` integrity, bundle id + **Team ID pinned at boot**, `spctl` (revocation-aware), staged bundle's own `CFBundleShortVersionString` must match | Refuses to update when not a bundle, **translocated**, parent not writable, or ad-hoc signed; GitHub-host allowlist on redirects |
| **based** | — | `cargo-packager-updater` (`.app.tar.gz`) | `cargo-packager-updater` (NSIS, passive) | minisign key from `cargo packager signer`, `latest.json` built in CI | Same packager as mdoc; release needs extra `.app.tar.gz` + `.sig` artifacts |
| **cadence**, **jayjay** | — | Sparkle 2 framework + appcast | — | Sparkle EdDSA | Native macOS UI, but an Objective-C framework in the bundle |
| termy, monocurl, hunk, waku, zaku, tty7 | various | dmg/tar swaps, some with helper binaries | some | SHA-256 / Ed25519 | Variations of the above |

### Practices worth borrowing

1. **Separate check / download / verify / install / restart** as distinct
   states (Zed, gpui-updater): `Checking → Downloading{progress} → Installing →
   ReadyToRestart`, with errors silent on automatic checks and shown on manual ones.
2. **Never trust the feed alone** (OxiMux): verify the downloaded `.app`'s
   codesign identity against the running app's Team ID, run `spctl`, and read
   the version from the staged bundle rather than the tag.
3. **Stage on the same volume, swap by rename, roll back on failure**
   (gpui-updater) — the app is never left missing.
4. **Know when not to self-update** (OxiMux, based, Zed): dev builds, App
   Translocation, non-writable install dir, package-manager installs → fall
   back to **View release**.
5. **Windows needs the old process gone** before files are replaced: hand off
   to the installer or a helper after exit (Zed, gpui-updater MSI path).
6. **Clean up** temp/staging dirs on next launch (Zed).
7. **Show what's new after the restart** (based: pending release-notes flag).

## Decisions

### Round 1 (2026-10-11)

1. **Ship during 0.x betas**, not after 1.0. Betas are where fast updates and
   updater bugs matter most. Consequence: the first release carrying the
   installer must be solid, because a broken updater can only be repaired by a
   manual download.
2. **macOS only** at first. Windows keeps **View release** (unsigned `.exe`
   would be only as trustworthy as a checksum served beside it; the `.msi` is
   per-machine and elevates). Windows is a later step, together with a
   signature scheme.
3. **Own port into `src/updater.rs`**, as with the Zorite port: gpui-updater's
   stage-and-rename swap with rollback (MIT/Apache), OxiMux's verification
   gates, Zed's status machine and cleanup (GPL-3, compatible). No new updater
   dependency, no Sparkle, no cargo-packager-updater.
4. **Trust = Developer ID, not the feed**: before replacing the installed app,
   the downloaded update must match `SHA256SUMS`, and the staged `.app` must
   pass `codesign` verification, carry bundle id `io.github.lawkitt.mdoc` and
   Team ID `F33DLM8K53`, be accepted by `spctl`, and contain the version the
   release claims. No extra minisign key on macOS.
5. **User-initiated install**: automatic checks only *find* updates. The user
   clicks **Download and install**, then **Restart**. Nothing is downloaded
   without a click.
6. **Check timing unchanged**: at launch and on **Check now**; no periodic
   polling.

### Round 2 (2026-10-11)

7. **Swap only when mdoc stops**: **Download and install** downloads,
   verifies and *stages* the new `.app`; the installed bundle is replaced
   after the normal quit flow (unsaved-change prompts, session saved, DOCX
   workers shut down) — on **Restart now**, followed by `cx.restart()`, and on
   any ordinary quit while an update is staged. Rationale: mdoc re-executes
   itself (`--mdoc-docx-worker` via `current_exe()`), so a swap under a running
   process would version-skew those workers.
8. **Ineligible installs fall back to View release** with a one-line reason;
   no admin escalation. Ineligible: not inside a `.app` (dev / `cargo run`),
   App Translocation, running from a mounted `.dmg`, parent directory not
   writable, running build not Developer ID signed by `F33DLM8K53`.
9. **Download rules**: asset by exact name `mdoc_<version>_arm64.dmg`;
   SHA-256 from the release's `SHA256SUMS`; redirects only to `github.com` /
   `*.githubusercontent.com`; files under
   `~/Library/Caches/io.github.lawkitt.mdoc/updates/`, cleaned at launch;
   progress with **Cancel**; no resume. A release without the `.dmg` or its
   checksum entry is detection-only (View release). The release pipeline is
   unchanged.
10. **UI**: the toolbar dot stays; **Settings → Updates** gains **Download and
    install** → progress + Cancel → "ready" + **Restart now**. **Plus** a
    clickable update banner in the main window that opens Settings → Updates
    (details in round 3).
11. **After the update**: a one-time "Updated to mdoc x.y.z" notice with a
    **What's new** link, driven by a pending-version preference field.
12. **Tests**: unit tests (asset selection, checksum, eligibility); an
    `#[ignore]` local end-to-end test with Developer-ID-signed fixture bundles
    (accept, wrong Team ID, tampered bundle); `just smoke-update` — a signed
    `0.0.0-dev` build in a temporary Applications-like folder updates itself
    from the real latest release via a hidden `--mdoc-self-update` flag, then
    `spctl` checks the result. Local / manual release step, not a CI gate.

### Round 3 (2026-10-11)

13. **Banner**: a slim full-width strip at the top of the workspace, built like
    the existing session notice but tinted with the accent (not warning
    amber), with its own id so it can coexist with the session notice. The
    whole strip opens Settings → Updates; keyboard- and screen-reader-operable
    like the notice.
14. **Banner states**: *available* ("mdoc x is available — Update…"),
    *downloading* ("Downloading mdoc x… NN%"), *ready* ("mdoc x is ready —
    Restart to update", with an inline **Restart**), *failed* ("Couldn't update
    to mdoc x — Details…").
15. **Dismiss (×)**: *available* is dismissed per version and remembered across
    launches (dot and Settings still show it; a newer version brings the banner
    back); *ready* and *failed* are dismissed for the session only.
16. **Automatic-check switch off**: *available* never appears unasked; the
    user-initiated states (*downloading*, *ready*, *failed*) still show.
17. **Swap failure at quit**: keep the old app (rename-aside rollback), delete
    the staged copy, persist the error, show *failed* at next launch with
    **View release**; never retry silently.

### Round 4 (2026-10-11)

18. **Staging in Caches**: the verified `.app` waits in
    `~/Library/Caches/io.github.lawkitt.mdoc/updates/mdoc.app`. At quit it is
    `ditto`-copied to a hidden sibling of the installed bundle, its signature is
    checked again, and then the bundles are swapped by rename. Nothing hidden lives
    in `/Applications` between download and quit.
19. **A newer release discards the staged one**: the staged update is deleted
    and the newer version is offered as *available* (a new click is required).
20. **Records**: ADR 0040 replaces ADR 0038's detection-only rule; README
    privacy text and a CHANGELOG entry land with the implementation.

### Implementation notes (2026-10-11)

- Decision 11's "pending-version preference field" became a small state file,
  `<data dir>/mdoc/update.json` (dismissed banner version, last quit's
  install outcome): Settings saves whole drafts, which would overwrite
  machine-written fields. The *updated* notice is the banner's fifth state.
- The swap runs in the synchronous part of the `on_app_quit` handler: GPUI
  allows asynchronous quit work only 200 ms (`SHUTDOWN_TIMEOUT`), but the
  handler body itself runs to completion. `cx.restart()` reaches the same
  handler through the platform quit, so Restart now and plain quit share one
  swap path.

## Glossary

- **Update check** — the existing detection-only request (ADR 0038).
- **Install** (in this design) — replacing the installed mdoc with a newer
  release from inside the app.
- **Staged update** — a downloaded, verified `mdoc.app` waiting in Caches to
  replace the installed bundle at the next quit or restart.
- **Swap** — the quit-time replacement: copy beside the installed bundle,
  re-verify, rename old aside, rename new in, delete old (or roll back).
- **Eligible install** — a running mdoc that may update itself (signed by
  `F33DLM8K53`, inside a writable, non-translocated `.app` not on a disk image).
- **Update banner** — the clickable strip at the top of the workspace showing
  *available*, *downloading*, *ready* or *failed*.
