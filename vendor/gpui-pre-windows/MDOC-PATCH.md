# Vendored: gpui-pre-windows 0.3.12

Source: crates.io `gpui-pre-windows` 0.3.12 (Zed `gpui_windows`,
zed@d2a4fb6ded7c470be25d1541281da3882495b65b; Apache-2.0,
see LICENSE-APACHE). The workspace patches this version without updating the
GPUI family. Registry checksum:
`7201762420222d5b5ce5bdf0056996199212a3349bfb2457a3a0238c688681f3`.
Re-ported from 0.3.2 for [ADR 0117](../../docs/adr/0117-gpui-0-3-12-and-a-quicker-sidebar-reveal.md).
The save dialog now opens through upstream's `dialog::show_dialog`; the
title is taken before it.

Local changes:

- `directx_devices.rs`: feature `test-support` adds a WARP D3D11 device.
- `directx_renderer.rs`: the swap chain is optional to permit offscreen-only
  resources. Native construction still always supplies its original swap
  chain; presentation and native resize require it. Rendering/shaders, atlas,
  path intermediates and BGRA staging readback are shared unchanged.
- `headless.rs`: feature `test-support` exports a standalone
  `PlatformHeadlessRenderer` using offscreen textures on WARP, and the
  DirectWrite text system (which needs no device in 0.3.12). It never constructs WindowsPlatform, HWND,
  DirectComposition, a swap chain or an event loop; it never presents.
- `gpui_windows.rs`: exports these two test-only entry points. mdoc supplies
  the renderer factory to HeadlessAppContext directly, without patching the
  platform convenience crate.
- `platform.rs` (`mdoc patch`): `set_next_save_prompt_message` stores a
  message for the next save dialog on this thread; `prompt_for_new_path`
  takes it and sets it as the `IFileSaveDialog`'s title. Without one the
  dialog is unchanged. `gpui_windows.rs` exports it; mdoc calls it from
  `src/document_view/save_prompt.rs`, so a Save As on the way to a new case
  says why it opens.
- `clipboard.rs` (`mdoc patch`), exported from `gpui_windows.rs`: a string
  written with the metadata `CONCEALED_CLIPBOARD_METADATA` also carries the
  registered formats `ExcludeClipboardContentFromMonitorProcessing`,
  `CanIncludeInClipboardHistory` (DWORD 0) and `CanUploadToCloudClipboard`
  (DWORD 0), so Win+V history, cloud clipboard sync and clipboard monitors
  skip it. Every other copy is unchanged. mdoc tags restored text with it
  (`src/restored_clipboard.rs`; recognition grill 2026-11-01, round 3).

Only the save dialog title and concealed clipboard text run in the normal product application. The
headless app entry point and runner also require debug assertions and the
headless-renders feature.
Validation: the app's Windows `just lint`, `just test` and
`just headless-renders <new-directory>` exercise this adapter, including
DirectWrite text, paths, PNG readback, both themes, and actual Tab events.

Drop this directory and the patch/dependency once the adopted GPUI family
provides a standalone Windows headless renderer and text-system factory,
its `prompt_for_new_path` takes a message, and its clipboard can mark text
concealed.
When updating GPUI, re-vendor the adopted version and reapply only these
changes; compare against the published source before updating expectations.
