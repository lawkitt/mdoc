# Vendored: gpui-pre-macos 0.3.2

Source: crates.io `gpui-pre-macos` 0.3.2 (Zed `gpui_macos`,
zed@801c087af22dd189dc1aa49e2f370b4f04190b19; Apache-2.0,
see LICENSE-APACHE). The workspace patches this version without updating the
GPUI family. Registry checksum:
`66a1e7a41b11c83b121b7bf729537c5b46dad91ef0cf221851cfb287cbe0fe33`.

Local changes, marked `mdoc patch`:

- `platform.rs`: `set_next_save_prompt_message` stores a message for the next
  save panel on this thread; `prompt_for_new_path` takes it and sets it as the
  `NSSavePanel`'s message line. Without one the panel is unchanged.
- `gpui_macos.rs`: exports `set_next_save_prompt_message`. mdoc calls it from
  `src/document_view/save_prompt.rs`, so a Save As on the way to a new case
  says why it opens.
- `platform.rs`: the app delegate answers `applicationShouldTerminate:`
  with the `TerminateReply` of the handler from
  `set_should_terminate_handler` (Now, Cancel, or Later, answered by
  `reply_to_should_terminate`); all three are exported from `gpui_macos.rs`.
  Without a handler it terminates now, as upstream. mdoc sets it in
  `src/system_quit.rs`, so a quit from the Dock, an Apple Event or logout
  asks about unsaved work like ⌘Q (ADR 0071).
- `Cargo.toml`: `[lints.rust.deprecated]` is set to `allow`. Upstream
  still calls the deprecated `cocoa` crate (716 warnings). Cargo hides
  warnings from registry crates, but prints them for this one because it is
  patched in from a local path.

Drop this directory, the patch entry and the `gpui_macos` dependency once the
adopted GPUI family's `prompt_for_new_path` takes a message and the app can
answer `applicationShouldTerminate:`. When updating GPUI, re-vendor the
adopted version and reapply only these changes.
