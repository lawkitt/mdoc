# Selected Zorite ports — verification

Date: 2026-10-05. Accepted scope is recorded in
[ADR 0009](../../../docs/adr/0009-selected-zorite-upstream-fixes.md).

## Upstream provenance

Reviewed Zorite mainline boundary `3f14fafba120386b058e3cde24ced55f23dd653e`
through `4aad847f57a5ac3e8ec0b6220e25d9db7e5264b4`.

- Linux renderer source was introduced upstream by
  `5140b901349108b569accaf6c6fa157e29d00f5c`; glyph correction:
  `9bb9ed10d9e8285e39deccc5ccfcb46fdbc4c3ab`.
- AppImage permissions: `f022066792ebac25c91bcd169f1f9736b0ba77e0`.
- Golden-ratio constant: `e7bb3ca5f21e7837a975391d3f07b0be54b782a5`.
- Highlight/color rendering portion of merge
  `594e8e629aaf179ab585a844f12d21ff775f95ea`, adapted to mdoc.

Vendored GPUI files were compared byte-for-byte with published 0.3.2 crate source:
only `src/cosmic_text_system.rs` differs. Apache-2.0 license is identical;
the adjacent provenance note records mdoc adoption and the removal condition.
Cargo.lock changes only the renderer's source/checksum for the path override.

## Automated results

Host: Apple Silicon macOS; Rust 1.98.1; development/test profiles, 2026-10-05.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed; Cargo emits existing global-min-publish-age and `block` future-compatibility notices |
| `cargo test --workspace` | 374 passed, 0 failed, 11 ignored across unit/integration/doc-test suites |
| `cargo build` | Passed |
| cargo-deny 0.20.2 `check` | Advisories, bans, licenses and sources passed; existing duplicate/unused-ignore warnings remain |
| actionlint 1.7.12 on `release.yml` | Passed; local shellcheck was unavailable |
| AppDir permission fixtures, running the exact new workflow block under `bash -eu` | 0744 launcher becomes 0755; 0644 data and 0755 executable pass; extra 0700 executable fails with exit 1 |
| `git diff --check` | Passed |

The lint tools were downloaded from their official GitHub release assets into
an isolated temporary directory and verified against the corresponding checksums.
No dependency/toolchain/action version update was added to mdoc.

## Rendering and lifecycle regression evidence

Imported upstream tests cover ordinary highlight pairing and colored mark/span
styles in both renderers. Added mdoc regressions establish:

- Hidden markers stay out of reader search and table measurement.
- Cyrillic highlighted text maps to exact UTF-8 source ranges; HTML attributes
  stay out of editor visible-text search.
- Entity/backslash decoding before highlights does not shift marker recognition
  or click-to-source checkpoints. Plain text avoids a decoded-offset allocation.
- Comparisons, longer equals runs, unmatched/cross-line highlights and escaped
  markers remain literal. Backtick runs and closing-tag text inside code remain
  code, rather than ending a style.
- Nested colors restore the parent style; unsupported span styling and nested
  unsupported tags remain literal. Quoted attributes, actual `style` boundaries
  and invalid numeric colors are covered.
- Headless editor painting retains source annotation/search geometry inside
  highlighted and colored Russian mentions. Grouped replacement preserves
  surrounding formatting, rejects a stale revision, and undoes in one operation.
- The application Copy Markdown test now includes these formats and still
  verifies exact CRLF source/unsaved content, selection, undo, document identity,
  warnings and an unchanged original file.

## Native and platform boundaries

A standalone native macOS editor demo was launched in an isolated temporary app
bundle using the newly built example binary, without opening mdoc's persisted
session or user documents. Visual inspection confirmed the Russian
`==Важное условие==` highlight, a colored mark background and blue span text;
`a == b == c` stayed literal. The smoke app was terminated afterwards.
The permanent demo includes this reproducible example.

This is a narrow dark-theme native painting smoke, not full keyboard/IME,
accessibility, table layout, light-theme or cross-platform acceptance.
Native Linux combining-mark placement and actual AppImage launch were unavailable
on this host and remain pending. The fixture check does not establish FUSE,
direct-mount or firejail startup. The vendored backend's own upstream tests
remain unrun; they depend on Zed's development repository/assets. macOS workspace
checks do not compile that Linux-only backend.

## Review delivery

[mdoc PR #10](https://github.com/lawkitt/mdoc/pull/10), implementation commit
`7a895e3b8580e6604881d869ebe56f1d21a6b59c`. Main merge and release are separate.
