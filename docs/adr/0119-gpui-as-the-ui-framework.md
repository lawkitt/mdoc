# GPUI as the UI framework

Status: Accepted, 2026-11-15.

## Decision

- The desktop app is built on GPUI, Zed's GPU-rendered Rust UI framework, from
  the published `gpui-pre` packages (0.3.12), because mdoc's editor, PDF
  viewer and offscreen render harness came from Zorite on GPUI. Leaving it is
  a frontend migration that needs equivalents for editor layout and input,
  the preview, accessibility, window lifecycle and the harness.
- The editor (`mdoc-editor`), PDF viewer (`gpui-pdf`) and BiDi adapter
  (`gpui-bidi`) are forks of Zorite's GPUI crates, owned by mdoc.
- The macOS, Windows and WGPU platform crates are vendored in `vendor/` and
  patched through `[patch.crates-io]`; each patch is listed in that crate's
  `MDOC-PATCH.md` (or `ZORITE-PATCH.md`) and re-ported on every upgrade.
- The macOS minimum is 12.0, the target of GPUI 0.3.12's Metal shader build.
- GPUI Fast, GPUI Kit, GPUI CE, a Zed Git pin and other frontends (Iced,
  Slint, Tauri) are not adopted.

## Invariants

- One GPUI type family across every desktop crate.
- An upgrade keeps behaviour on macOS and Windows and the windowless
  headless renders working on both.

References: [GPUI grill](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/design/gpui-grill-2026-11-14.md).
