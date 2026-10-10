# CI and release pipeline — design interview

Status: interview complete 2026-10-10, awaiting confirmation. Decisions graduate to ADRs once the
interview reaches shared understanding.

## Baseline facts

- GitHub Actions are disabled on `lawkitt/mdoc` (public repo); no tags or
  releases exist yet. `Cargo.toml` is at `0.1.0`; `CHANGELOG.md` has only
  `## Unreleased`.
- The workflows are a near-verbatim port of Zorite's (`packetThrower/zorite`):
  - `ci.yml`: fmt + cargo-deny (Ubuntu only), clippy `-D warnings` and
    `cargo test --workspace` on macos-26 arm64, Windows x64/arm64, Ubuntu
    x64/arm64; plus an mdoc-only Windows x64 OCR setup smoke test that downloads
    real model components. Also callable from `release.yml`.
  - `release.yml`: on `v*` tags (or manual dispatch for unpublished test builds)
    runs CI, then builds macOS arm64 + cross-compiled x86_64 (`.dmg`, `.zip`),
    Windows x64/arm64 (NSIS `.exe`, `.msi` for stable tags only, portable
    `.zip`), Linux x64/arm64 (`.deb`, `.rpm`, `.pkg.tar.zst`, `.AppImage` with
    embedded zsync update information), then publishes a GitHub Release with
    CHANGELOG highlights, a commit list and `SHA256SUMS`. Pre-release tags
    (`-beta.1`) are marked pre-release. Nothing is signed or notarized.
  - `after_release.yml`: winget submission, gated off by
    `vars.ENABLE_WINGET_PUBLISHING` and `secrets.WINGET_TOKEN`.
  - `actionlint.yml`: lints workflow changes. All actions are SHA-pinned;
    Dependabot bumps Cargo and Actions weekly.
- Zorite additionally has a docs-site workflow, a Homebrew tap and Scoop bucket
  (separate repos), and a Nix flake.
- **Zorite's update feature** (`src/updater.rs`) is *detection only*: at boot
  (opt-out toggle, optional pre-releases) it queries the GitHub Releases API,
  compares semver with `CARGO_PKG_VERSION`, and shows an amber dot plus
  Settings → Updates with notes and a "View release" link. It never downloads
  or installs, explicitly because builds are unsigned. The AppImage zsync
  information is the only in-place update path (via external AppImageUpdate).
  mdoc did not port `updater.rs`.
- OCR and pseudonymization qualify only on Apple Silicon macOS and Windows x64
  (ADR 0004); other targets get native-text import only.

## Decisions

1. **No Nix.** `flake.nix`, `flake.lock` and `.github/workflows/nix.yml` are
   removed; mdoc is not distributed or built through Nix.
2. **Updates: detection only**, ported from Zorite's `updater.rs`. Enabled by
   default with a Settings toggle; the README states what the check sends (an
   unauthenticated GET to the GitHub Releases API with an mdoc User-Agent, no
   document data). No download/install until builds are signed.
3. **Shipped targets: Apple Silicon macOS and Windows x64 only** — the targets
   where OCR and pseudonymization qualify. Others return when they qualify.
4. **Signing:** `0.x` pre-releases ship unsigned; obtain an Apple Developer ID
   now (notarization is the bigger hurdle for legal users); signing on macOS and
   Windows is a requirement for `1.0`.
5. **Direct pushes to `main`** stay; CI runs on push (and on PRs, e.g.
   Dependabot). No branch protection until collaborators join.
6. **Windows OCR smoke test** (real model download) leaves everyday CI; it runs
   as a gate of the release workflow and on manual dispatch.
7. **Versioning:** first tag `v0.1.0-beta.1`; `-beta.N`/`-rc.N` mark
   pre-releases; release-note highlights come only from a matching
   `## [x.y.z]` CHANGELOG section (Zorite convention).
8. **Draft releases** at first: the tag builds and uploads to a draft that is
   reviewed and published by hand; revisit after a few clean releases.
9. **Repository Actions settings:** allow GitHub-owned plus explicitly listed
   actions, require full-SHA pinning, read-only default `GITHUB_TOKEN`, Actions
   may not approve PRs. The exact allow-list follows the final job set.
10. **Unshipped platforms** (Intel macOS, Windows arm64, Linux): their release
    jobs, packaging (`.deb`/`.rpm`/pacman/AppImage + zsync, Intel cross-build,
    `packaging/linux/`) and CI test runners go. One Ubuntu job keeps `fmt`,
    `cargo-deny` and clippy so Linux `cfg` code still compiles. README: "builds
    from source, unsupported".
11. **Formats:** macOS `.dmg`; Windows NSIS `.exe` always, `.msi` on stable tags.
    No `.zip`s.
12. **Version source of truth is the repo.** A release commit bumps
    `Cargo.toml` and `resources/Info.plist`; `release.yml` fails when the tag
    differs. Manual dispatch builds still patch the version.
13. **Update check behaviour:** pre-release builds include pre-releases
    automatically; stable builds offer "Include pre-releases" (off). Check at
    launch and on "Check now"; no polling. New Settings → Updates section after
    Spelling; a dot on the Settings entry. Silent on failure. Reuses `ureq` 3.
14. **Channels:** GitHub Releases only. `after_release.yml` and the winget
    templates are deleted; winget/Homebrew/Scoop are rebuilt for `1.0`.
15. **Apple Developer ID: individual account** (publisher is the owner's name).
    Signing/notarization steps are added only once the certificate exists.
16. **VC++ Redistributable:** the Windows installers ensure the x64 VC++
    Redistributable is present instead of relying only on in-app instructions.
    NSIS `.exe` bundles `vc_redist.x64.exe` and runs it (elevated) only when
    missing; CI verifies its Microsoft Authenticode signature, not a hash (the
    upstream link is unversioned). The `.msi` blocks install with a clear
    message when the runtime is missing (no Burn bootstrapper).
17. **Automation:** root `justfile` of thin recipes; non-trivial logic in
    `scripts/*.sh` (bash; Git Bash on Windows). CI invokes the same recipes, so
    local and CI checks cannot drift. Recipes: `check`, `fmt`, `lint`, `test`,
    `smoke-ocr`, `package`, `release <version>`, `icon` (replaces
    `build/make-icon.sh`), `actionlint`.
18. **`just release <version>`:** requires clean, up-to-date `main`; runs
    `just check`; bumps `Cargo.toml`, `Info.plist`, `Cargo.lock`; for stable
    versions turns `## Unreleased` into `## [x.y.z] - <date>` and opens a new
    Unreleased (betas leave the CHANGELOG); commits `release: vX.Y.Z`, tags,
    and pushes only after a y/N confirmation (`--no-push` available).
19. **Pinned toolchain:** `rust-toolchain.toml` pins the exact local Rust
    version for development, CI and releases; bumped by hand.
20. **OCR smoke on both shipped targets** (macos-26 and Windows x64) as a
    release gate.
21. **Delivery order** (direct commits to `main`): trim workflows → toolchain +
    `just`/scripts + CI on `just` → VC++ runtime in installers → update check →
    manual dispatch dry run → tag `v0.1.0-beta.1`. ADR 0037 (pipeline), ADR 0038
    (update check), CONTEXT.md terms.
22. **Heavy checks are local `just` recipes, CI stays light.** `just perf`
    runs the `#[ignore]` performance tests serially; `just smoke-models` runs
    the installed-model tests (downloads only with `--download`); `just
    smoke-ocr`; `just check-all` = `check` + `smoke-ocr` + `smoke-models`.
    CI keeps fmt/deny/clippy/test, plus the OCR smoke as a release gate on the
    built targets. `just release` offers, but does not require, `check-all`.
23. **Build provenance:** the release job publishes
    `actions/attest-build-provenance` attestations alongside `SHA256SUMS`.
24. **Local platforms:** the owner develops on Apple Silicon macOS and has a
    Windows x64 PC, so every recipe (including `package` and the smoke tests)
    must run natively on both shipped targets.
25. **Windows shell for recipes:** Git Bash at its fixed path
    (`C:/Program Files/Git/bin/bash.exe`), never a bare `bash` (WSL). Release
    packaging steps currently in PowerShell move to bash.
26. **`just setup` / `just doctor`:** install and report the pinned tools
    (`cargo-deny`, `cargo-packager`, `actionlint`; on Windows `cargo-wix` and
    WiX). Rust tools via `cargo binstall --locked`; WiX only after
    confirmation. Versions live in `scripts/tools.env`, shared with CI.

Outcome: [ADR 0037](../adr/0037-ci-and-release-pipeline.md) and
[ADR 0038](../adr/0038-update-check.md).
