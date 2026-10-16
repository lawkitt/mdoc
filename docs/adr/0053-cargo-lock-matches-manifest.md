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
- **Dependabot changes the manifest when it must.** `versioning-strategy:
  increase-if-necessary` on the cargo ecosystem: a bump the requirement does not
  allow raises the requirement, and a bump that needs code changes fails CI
  visibly instead of being re-resolved. `docxide-pdf` is ignored there and
  bumped by hand.
- **Exact pins carry their reason** in a `Cargo.toml` comment.
- **Sensitive bumps are deliberate work**, one PR per crate, each with its own
  check: `dirs` (the same case, key, update and crash-report paths on macOS,
  Windows and Linux), `aes-gcm` (existing case files still decrypt) and
  `docxide-pdf` (preview corpus unchanged; only when a rendering fix needs it).

## Consequences

Dependabot PRs for breaking bumps arrive red until someone does the code work.
If Dependabot's cargo support for `increase-if-necessary` falls short, the
`--locked` check still stops lockfile-only bumps from merging green.
