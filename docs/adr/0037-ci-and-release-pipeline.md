# CI and release pipeline

Status: Accepted and implemented, 2026-10-10; first release run outstanding. See the
[historical design interview](https://github.com/lawkitt/mdoc/blob/01ab890db29febc5d9920de64aa494480a5d5407/docs/design/ci-release-pipeline.md).

## Problem

The workflows were ported almost verbatim from Zorite while GitHub Actions were
disabled. They build six targets in about fourteen formats, although OCR and
pseudonymization qualify only on Apple Silicon macOS and Windows x64
([ADR 0004](0004-explicit-offline-ocr-with-qualified-models.md)). They also
carry a Nix flake, winget automation and a version taken from the tag rather
than the repository. Nothing lets local development run the same checks as CI.

## Decision

- **Shipped targets**: Apple Silicon macOS (`.dmg`) and Windows x64 (NSIS
  `.exe` always, `.msi` on stable tags only). No `.zip`s. Intel macOS, Windows
  arm64 and Linux lose their release jobs, packaging and CI test runners; one
  Ubuntu job keeps `fmt`, `cargo-deny` and clippy so Linux code still compiles.
  The README calls those platforms "builds from source, unsupported".
- **No Nix**: the flake and its workflow are removed.
- **CI** runs on pushes to `main` and on PRs: fmt, deny, clippy `-D warnings`
  and `cargo test --workspace` on macos-26 and windows-latest, plus the Ubuntu
  lint job. Direct pushes to `main` remain; no branch protection yet.
- **Release** runs on `v*` tags (manual dispatch builds without publishing): CI,
  then the OCR smoke test on macos-26 and Windows x64, then packaging, then a
  **draft** GitHub Release with CHANGELOG highlights, a commit list,
  `SHA256SUMS` and build-provenance attestations. The release fails when the
  tag differs from the version in `Cargo.toml`.
- **Versioning**: the repository is the source of truth. A **release commit**
  bumps `Cargo.toml`, `Cargo.lock` and `resources/Info.plist`. `-beta.N` /
  `-rc.N` tags are pre-releases; highlights come only from a matching
  `## [x.y.z]` CHANGELOG section. The first tag is `v0.1.0-beta.1`.
- **Signing**: `0.x` pre-releases ship unsigned. The owner obtains an
  individual Apple Developer ID; notarized macOS and signed Windows builds are
  required for `1.0`. Signing steps land only once the certificate exists.
- **VC++ Redistributable**: the NSIS `.exe` bundles `vc_redist.x64.exe` and
  runs it (elevated) only when the runtime is missing; CI checks its Microsoft
  Authenticode signature instead of a hash. The `.msi` refuses to install with
  a clear message when the runtime is missing.
- **Channels**: GitHub Releases only. winget automation is deleted;
  winget, Homebrew and Scoop return with signed `1.0` builds.
- **Automation**: a root `justfile` of thin recipes with logic in
  `scripts/*.sh`, run by Git Bash at its fixed path on Windows. CI calls the
  same recipes. Recipes: `setup`, `doctor`, `check`, `fmt`, `lint`, `test`,
  `perf`, `smoke-ocr`, `smoke-models` (`--download` to allow downloads),
  `check-all`, `package`, `release <version>`, `icon`, `actionlint`. Tool
  versions live in `scripts/tools.env`, read by both CI and `just setup`.
- **`just release <version>`** requires a clean, up-to-date `main`, runs `just
  check` (offering `check-all`), makes the release commit, renames
  `## Unreleased` for stable versions only, tags, and pushes after a y/N
  confirmation (`--no-push` available).
- **Toolchain**: `rust-toolchain.toml` pins an exact Rust version, bumped by
  hand.
- **Repository settings**: GitHub-owned plus listed actions only, full-SHA
  pinning required, read-only default `GITHUB_TOKEN`, Actions cannot approve
  PRs, external fork PRs need approval. No personal access token is needed.

## Invariants

- Every action is pinned to a full commit SHA; only the release-publishing job
  has `contents: write`.
- Local recipes and CI run the same commands; everything must also run natively
  on the owner's Apple Silicon Mac and Windows x64 PC.
- Heavy and machine-dependent tests (performance, installed models, local
  files) stay `#[ignore]` and run only through local recipes; ordinary CI never
  needs network access beyond fetching dependencies.

## Consequences

- Linux, Intel and Windows arm64 users must build from source.
- Unsigned betas need right-click → Open on macOS and "Run anyway" on Windows.
- The Windows installer grows by about 25 MB.
