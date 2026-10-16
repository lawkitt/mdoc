# Cargo.lock always matches Cargo.toml

Status: Accepted, 2026-10-16.

## Problem

Dependabot PRs #48, #49 and #50 bumped `dirs` to 7.0.0, `docxide-pdf` to 0.18.4
and `aes-gcm` to 0.11.1 in `Cargo.lock` only, while `Cargo.toml` still required
`dirs = "6"`, `docxide-pdf = "=0.17.1"` and `aes-gcm = "0.10"`. CI builds without
`--locked`, so cargo re-resolved silently and the PRs passed. Afterwards every
cargo run rewrote the lockfile, every checkout showed it modified, and
`cargo fetch --locked` in `scripts/package.sh` refused the committed file.

## Decision

- **Restore first.** `main` takes the lockfile cargo resolves from the current
  manifest (lawkitt/mdoc#57); the three bumps are reverted.
- **Drift fails the gate.** The lint recipe runs
  `cargo metadata --locked --format-version 1`, so `just check` locally and CI
  on every runner reject a lockfile the manifest does not produce (ADR 0037).
- **Dependabot keeps its default strategy.** For cargo it accepts only `auto`
  or `lockfile-only` (`increase-if-necessary` is rejected as invalid config), so
  a bump the manifest does not allow can still arrive lockfile-only; the
  `--locked` check fails it, and someone raises `Cargo.toml` and fixes the code
  in its place. `docxide-pdf` is ignored there and bumped by hand.
- **Exact pins carry their reason** in a `Cargo.toml` comment.
- **Sensitive bumps are deliberate work**, one PR per crate, each with its own
  check: `dirs` (the same case, key, update and crash-report paths on macOS,
  Windows and Linux), `aes-gcm` (existing case files still decrypt) and
  `docxide-pdf` (preview corpus unchanged; only when a rendering fix needs it).

## Consequences

Dependabot PRs for bumps the manifest does not allow arrive red; they are
replaced by a deliberate PR that raises the requirement and does the code work.
