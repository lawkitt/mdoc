# CI and release pipeline

Status: Accepted and implemented, 2026-10-10; first release v0.1.0-beta.2, 2026-10-11. See the
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
  lint job. Direct pushes to `main` remain; no branch protection yet. The
  `release: v…` commit pushed with a tag is skipped on `main`: the tag's
  Release run tests that commit.
- **Release** runs on `v*` tags (manual dispatch builds without publishing): CI
  with the OCR smoke test as a step of the macos-26 and Windows x64 test jobs
  (reusing their build), then packaging and the
  **packaged-app gate** (`just smoke-package`: the shipped binary, on macOS from
  the `.dmg` under its hardened-runtime signature, downloads the OCR runtime and
  model and recognizes the English fixture via `mdoc --mdoc-self-test`), then a
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
  `check-all`, `package`, `smoke-package`, `release <version>`, `icon`,
  `actionlint`. Tool
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
- Release gates test the shipped artifact, not only test builds: the macOS
  bundle keeps the hardened runtime with `disable-library-validation`
  (`resources/entitlements.plist`), checked by `just package`, and must load
  the downloaded PDFium and ONNX Runtime in `just smoke-package`.
- Heavy and machine-dependent tests (performance, installed models, local
  files) stay `#[ignore]` and run only through local recipes; ordinary CI never
  needs network access beyond fetching dependencies.

## Consequences

- Linux, Intel and Windows arm64 users must build from source.
- Unsigned betas need System Settings → Privacy & Security → Open Anyway on
  macOS (right-click → Open no longer works from macOS 15) and "Run anyway" on
  Windows.
- The Windows installer grows by about 25 MB.
- v0.1.0-beta.1 was withdrawn unpublished: its hardened-runtime signature
  refused the downloaded runtimes, which only a packaged build shows; that
  gap is why `smoke-package` exists.
