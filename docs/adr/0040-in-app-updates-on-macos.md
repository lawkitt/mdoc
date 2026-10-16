# In-app updates on macOS

Status: Accepted, 2026-10-11. Supersedes the "detection only" decision of
[ADR 0038](0038-update-check.md) on macOS. Banner and dot placement amended by
[ADR 0048](0048-workspace-shell-redesign.md) (sidebar update card).

## Problem

ADR 0038 only tells users that a newer release exists; they must download the
`.dmg` and replace the app by hand. Beta testers fall behind, and every fix
waits on a manual step. ADR 0038 postponed installation until builds were
signed; since [ADR 0039](0039-macos-signing-and-notarization.md) the macOS app
is Developer ID signed and notarized, so the downloaded app can be checked
against an identity that the release feed cannot fake.

## Decision

- **Scope**: Apple Silicon macOS, from the 0.x betas on. Windows keeps
  **View release**.
- **Own code**: extend `src/updater.rs`, porting selected pieces rather than
  adding a dependency: the stage-and-rename swap with rollback from
  [gpui-updater](https://github.com/AprilNEA/gpui-updater) (MIT/Apache), the
  verification gates and eligibility rules from
  [OxiMux](https://github.com/nhtera/OxiMux), and the status machine and
  stale-file cleanup from Zed's `auto_update` (GPL-3). No Sparkle, no
  cargo-packager-updater, no change to the release pipeline.
- **Consent**: checks run as before (at launch if enabled, and on **Check now**),
  and only *find* updates. Nothing downloads until the user clicks
  **Download and install**.
- **Download**: the release asset `mdoc_<version>_arm64.dmg`, checked against
  the release's `SHA256SUMS`; redirects only to `github.com` and
  `*.githubusercontent.com`; progress with **Cancel**; no resume. A release
  missing either file is treated as detection-only.
- **Verification** of the `.app` copied out of the image: `codesign` integrity,
  bundle id `io.github.lawkitt.mdoc`, Team ID `F33DLM8K53`, `spctl` acceptance,
  and `CFBundleShortVersionString` equal to the release version. The feed's tag
  is a claim; the bundle's signature and version are the facts.
- **Staging and swap**: the verified `.app` waits in
  `~/Library/Caches/io.github.lawkitt.mdoc/updates/`. The installed bundle is
  replaced only after the normal quit flow (unsaved-change prompts, session
  saved, DOCX workers stopped). This happens on **Restart now**, followed by
  `cx.restart()` into the bundle, or on any ordinary quit while an update is
  staged. The swap runs in the synchronous part of the `on_app_quit` handler,
  because GPUI gives asynchronous quit work only 200 ms. The swap copies with `ditto` beside the installed bundle, verifies
  again, renames the old bundle aside, renames the new one in, and deletes the
  old one; on any failure it renames the old bundle back.
- **Eligibility**: a running build may update itself only from a `.app` that
  is signed by `F33DLM8K53`, not translocated, not on a mounted disk image, and
  whose parent directory is writable. Otherwise the update stays visible, with
  **View release** and a one-line reason (no admin prompt).
- **UI**: a dot on Settings and **Settings → Updates** (with **Download and
  install**, progress, and **Restart now**), plus an **update card** in the
  sidebar foot (ADR 0048) that opens Settings → Updates. It shows *available*,
  *downloading* with a percentage, *ready* with an inline **Restart**, and
  *failed*. × hides *available* for
  that version across launches, and *ready* or *failed* for the session.
  *Available* appears only from automatic checks. After a successful update,
  mdoc shows "mdoc was updated to x.y.z" once, linking to the release.
  The dismissed version and the last quit's outcome live in
  `<data dir>/mdoc/update.json`, not in Settings, so a Settings draft never
  overwrites them.
- **Newer release while staged**: the staged update is discarded and the newer
  version is offered, which needs a new click.
- **Testing**: unit tests (asset selection, checksums, eligibility); an
  `#[ignore]` end-to-end test with Developer ID signed fixture bundles,
  including wrong-Team-ID and tampered cases; `just smoke-update`, a local
  manual-release step in which a signed `0.0.0-dev` build updates itself from
  the latest real release through a hidden `--mdoc-self-update` flag.

## Invariants

- Nothing is downloaded without an explicit click; automatic requests stay
  the single version-only GET of ADR 0038.
- The installed bundle is never modified while mdoc is running, and never left
  missing: every swap failure restores the previous bundle.
- A bundle is installed only after both verifications pass, one when staged and
  one right before the swap.
- Swap failures are reported at the next launch and never retried silently.

## Consequences

- The first release containing the updater must be solid: users only reach a
  fix to the updater itself by downloading by hand.
- The Developer ID certificate (expires 2027-02-01) now also gates updates. A
  release signed with a renewed certificate keeps the same Team ID and still
  verifies.
- The README's privacy paragraph must state that downloads happen only on
  request.
- Windows in-app updates remain future work. They need a signature scheme of
  their own (Authenticode or a minisign key) and a handoff after exit for the
  NSIS installer.

References: [interview](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/auto-update.md).
