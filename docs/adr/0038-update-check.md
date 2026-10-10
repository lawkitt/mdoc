# Update check

Status: Accepted, 2026-10-10; not yet implemented. Interview record:
[CI and release pipeline design](../design/ci-release-pipeline.md). Ported from
Zorite's `src/updater.rs`.

## Problem

Once releases exist, users have no way to learn that a newer build is out.
Automatic installation needs signed builds
([ADR 0037](0037-ci-and-release-pipeline.md)), which `0.x` doesn't have.

## Decision

- **Detection only**: mdoc never downloads or installs a new version. It
  compares the newest GitHub Release with its own `CARGO_PKG_VERSION` (semver)
  and, when newer, shows a dot on the control that opens Settings and fills a
  **Settings → Updates** section (after Spelling) with the version, release
  notes and **View release**, which opens the release page.
- **When**: once at launch and on **Check now**; no background polling.
  Enabled by default with an "Automatically check for updates" switch.
- **Pre-releases**: a pre-release build always considers pre-releases; a stable
  build has an "Include pre-releases" switch, off by default.
- **Failures are silent** (logged only): offline behaves like "no update".
- **Request**: one unauthenticated GET to the GitHub Releases API for
  `lawkitt/mdoc` with an mdoc User-Agent, over the existing `ureq` 3 client.
  The README states this and that no document data is sent.

## Invariants

- The check never runs on the UI thread and never blocks startup.
- No document content, path or setting other than the version is sent.
- Turning the switch off stops all automatic requests; Check now remains manual.

## Consequences

- Auto-install is reconsidered when signed `1.0` builds exist.
