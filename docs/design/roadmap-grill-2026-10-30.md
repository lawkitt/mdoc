# Grill: 0.3.0 grooming and fewer RCs (2026-10-30)

Interview record for grooming ROADMAP.md, adding Later items to 0.3.0 and
cutting fewer release candidates. ADRs written from it hold the rules; this
file keeps the reasoning.

## State at the start

- 0.3.0 had one item left, the selection bar; the other six from the
  2026-10-29 grill had merged.
- The Local CI runner item had met its Done-when line (#237 and #238 carry
  `local/macos` and `local/windows` statuses) but was still listed.
- Open bugs #179, #180, #181 had no milestone.
- Release run for v0.2.1: 40 min. Called CI 13 min (Windows tests longest),
  `just package` 17 min on macOS (with notarization) and 24 min on Windows
  (release profile: `lto = true`, `codegen-units = 1`), smoke and draft 1 min.
- 0.2.0 needed rc.1–rc.4 in three days: `docs/release-run.md`'s default gate
  cuts the next RC after every acceptance run with must-fix Fails.

## Round 1 decisions

1. **New 0.3.0 items**, from Later: Save mapping as case explains Save As
   ([Cases]); F6 into the sidebar lands on the active tab ([A11y]); a Mac
   shortcut for a tab's actions besides ⇧F10 ([A11y]); one install per
   machine ([Windows]). Model detection/memory and macOS `AXFocused` stay in
   Later.
2. **#179, #180, #181** join the 0.3.0 milestone as issues, not roadmap items.
3. **Housekeeping:** delete the finished Local CI runner item (its fragment
   `changes/local-ci-runner.md` already exists), date line 2026-10-30, and one
   0.3.0 list numbered from 1 with the selection bar first.
4. **rc.1 gate:** Engineering items don't block rc.1. Release CI is slow, so
   RCs should come less often; grilled from round 2.

## Round 2 decisions

5. **One RC per minor by default.** After rc.1's acceptance runs, must-fix
   fixes merge to `main` and are re-checked on a local unsigned
   `just package` build on the acceptance machines, only the rows each fix
   touches; then the stable is tagged and gets a smoke run like a patch. A
   further RC only when a fix touches what only a signed, installed build
   exercises: installer, signing or notarization, updater, keychain. (A
   signed `workflow_dispatch` build was rejected: still about 25 minutes.)
6. **A tag run skips the called CI** when `main` already tested the code
   (13 of 40 minutes). The exact rule is in round 3.
7. **Release profile** (`lto = true`, `codegen-units = 1`; most of the
   17–24 minute package step): measure thin LTO or more codegen units
   against binary size and the perf budgets; a Later item, not now. RCs and
   stables keep one profile.

## Facts found for round 3

- `main` CI skips the `release: v…` commit that `just release` pushes; the
  tag's run is the only test of it. That commit changes only version lines
  (Cargo.toml, Cargo.lock, Info.plist), CHANGELOG.md and `changes/`.
- `smoke-ocr` (English and Russian, via `cargo test`) runs only in the
  called CI of a release. `smoke-package` runs the built app's
  `--mdoc-self-test` on the English fixture only; `russian.pdf` and
  `russian.txt` are in the same fixture folder.

## Round 3 decisions

8. **Skip rule.** A tag run skips the called CI when the release commit
   changes only the version lines, `resources/Info.plist`, `CHANGELOG.md` and
   `changes/`, and its parent has a successful hosted `ci.yml` push run on
   `main`. If that run is still going, the tag run waits up to about 20
   minutes for it rather than starting its own; otherwise CI runs as today.
   The `version` job decides; `workflow_dispatch` `checks: true` still forces
   CI.
9. **Russian OCR in releases:** `smoke-package` also runs the built app's
   self-test on `russian.pdf`, on both platforms.
10. **Where it lives:** new ADR 0083 (fewer release candidates) amends ADR
    0059's acceptance line and `docs/release-run.md`'s default gate; the
    Engineering item Release-run rules is folded in and written now. The
    workflow and `smoke-package` change is one Engineering item to land
    before rc.1; the release-profile measurement goes to the end of Later.

## Round 4 decisions

11. **F6 into the sidebar** always lands on the active tab's row (Home's row
    on Home), in the expanded list or the collapsed rail; amends ADR 0062's
    landing rule for the sidebar only.
12. **⌃↩ on a focused sidebar tab** opens its menu on both platforms; ⇧F10
    and the Windows Menu key stay; the shortcuts panel lists it.
13. **Save mapping as case** on an unsaved conversion opens Save As whose
    macOS message line and Windows title read "Save the document first so
    the case can find it". No extra notice.
14. **Windows ships only the NSIS `.exe`.** The `.msi` goes to Later.
    Windows has no in-app updater yet (ADR 0040), so there is no updater
    wording to change. The `.msi` (per machine) never touches
    `%LOCALAPPDATA%`, so removing it keeps the case key.
15. **Order:** selection bar, then the rest as asked; settled after round 5.

## Round 5 decisions

16. **Existing `.msi` installs:** no detection code; mdoc has no real users
    yet. The 0.3.0 release notes say to remove an all-users mdoc in
    Settings › Apps before running the `.exe` (cases are kept).
17. **The `.msi` build code is deleted** (WiX steps in `release.yml`,
    `scripts/package.sh` and `scripts/setup.sh`, `[package.metadata.wix]`,
    `packaging/windows/wix/`); ADR 0037 amended to `.exe` only. Git history
    keeps the files; the Later line records the UpgradeCode a future `.msi`
    must reuse.
18. **Order in 0.3.0:** selection bar, `.exe` only, Save As explains, F6
    lands on the active tab, ⌃↩ tab actions. `.exe` only is second because a
    late installer fix would force another RC.
