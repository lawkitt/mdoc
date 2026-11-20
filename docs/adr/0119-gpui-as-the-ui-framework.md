# GPUI as the UI framework

Status: Accepted, 2026-11-15.

## Why

mdoc's editor, PDF viewer and offscreen render harness came from Zorite,
which is built on GPUI, Zed's GPU-rendered Rust UI framework. Leaving it is
a frontend migration that needs equivalents for editor layout and input,
the preview, accessibility, window lifecycle and the harness. mdoc uses the
published `gpui-pre` packages, owns forks of Zorite's editor, PDF viewer and
BiDi crates, and vendors the macOS, Windows and WGPU platform crates in
`vendor/`, each patch listed in that crate's `MDOC-PATCH.md` and re-ported
on every upgrade. One GPUI type family runs across every desktop crate. The
macOS minimum, 12.0, is the target of GPUI's Metal shader build.

## Rejected

- **Iced, Slint or Tauri**: the migration above.
- **GPUI Fast, GPUI Kit, GPUI CE or a Zed Git pin**: a second GPUI type
  family beside the published packages.

## Revisit when

An upgrade can't keep behaviour on macOS and Windows, the headless renders
and the harness working, or GPUI stops publishing packages.
