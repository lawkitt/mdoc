# Fewer release candidates

Status: Accepted, 2026-10-30. Amended by [ADR 0084](0084-menu-bar-shelf.md)
(a feature after rc.1). Amends
[ADR 0059](0059-roadmap-after-0-1-0.md) (acceptance per release) and
[ADR 0037](0037-ci-and-release-pipeline.md) (Windows artifacts, release CI).

## Problem

A release run takes about 40 minutes on hosted runners (v0.2.1: called CI
13, `just package` 17 on macOS with notarization and 24 on Windows, smoke and
draft 1). `docs/release-run.md` cut a new RC after every acceptance run with a
must-fix Fail, so 0.2.0 took rc.1–rc.4 in three days, each with its own
release run and acceptance pass. The called CI repeats checks `main` already
ran, and the Windows `.msi` is a second installer to build, test and keep
apart from the `.exe`, for no users yet.

## Decision

- **One RC per minor by default.** After rc.1's acceptance runs, must-fix
  fixes merge to `main` and are re-checked on a local unsigned
  `just package` build on the acceptance machines, only the rows each fix
  touches. Then the stable is tagged and gets a smoke run, as a patch does.
- **Another RC only** when a must-fix fix touches what only a signed,
  installed build exercises: the installer, signing or notarization, the
  update check or in-app update, or the case key in the keychain. Such work
  should land before rc.1. A feature added after rc.1 also makes a new RC;
  full acceptance then runs only on the last RC, and earlier ones get an
  install and smoke check ([ADR 0084](0084-menu-bar-shelf.md)).
- **Publishing.** A patch or stable publishes once both agent runs are done
  with no must-fix Fail; rows only the maintainer can do may follow and are
  listed in the report.
- **Tag runs reuse `main`'s CI.** A tag run skips the called CI when the
  release commit changes only the version lines (`Cargo.toml`,
  `Cargo.lock`), `resources/Info.plist`, `CHANGELOG.md` and `changes/`, and
  its parent has a successful hosted `ci.yml` push run on `main`. If that run
  is still going, the tag run waits up to about 20 minutes for it rather than
  starting its own. Otherwise CI runs as before. The `version` job decides;
  `workflow_dispatch` with `checks: true` still forces CI.
- **Both OCR languages on what ships.** `just smoke-package` runs the built
  app's self-test on the English and the Russian qualification fixtures on
  both platforms, since a skipped CI also skips `smoke-ocr`.
- **Windows ships the NSIS `.exe` only.** The `.msi` and its WiX build
  (workflow steps, `scripts/package.sh`, `scripts/setup.sh`,
  `[package.metadata.wix]`, `packaging/windows/wix/`) are removed. Nothing
  detects an earlier `.msi` install; the release notes say to remove it in
  Settings › Apps first (it never touches `%LOCALAPPDATA%`, so cases are
  kept). A future `.msi` must reuse UpgradeCode
  `3D50EAE8-D339-41B6-82BC-126389601487`.
- **Release profile unchanged.** RCs and stables build with the same
  profile; measuring a cheaper one (thin LTO, more codegen units) is a Later
  item.

## Consequences

- A minor normally costs one RC run and one stable run, about an hour of
  release CI less per avoided RC, and one full acceptance pass.
- A must-fix fix is re-checked on a build that is neither signed nor
  installed; that is acceptable only outside the areas that force an RC.
- Once the workflow change lands, a release run takes about 27 minutes. Until
  then, tag runs keep running the called CI.
- A commit that slipped past `main` CI (a red or missing run) still gets the
  full CI in its tag run.
- Windows users who want an all-users install have none until a `.msi`
  returns.

References: [interview](../design/roadmap-grill-2026-10-30.md).
