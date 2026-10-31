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
- `status_item.rs` (new), exported from `gpui_macos.rs`: `set_status_item`
  shows one `NSStatusItem` whose button draws an SF Symbol as a template
  image and calls back on click, or removes it; `status_item_bounds` gives
  the button's display and bounds (top-left origin) so a
  `WindowKind::PopUp` window can open under it. `window.rs`'s
  `display_id_for_screen` is `pub(crate)` for it. mdoc uses them in
  `src/menu_bar.rs` for the menu bar shelf (ADR 0084).
- `window.rs`: `start_text_and_file_drag` starts an outbound drag from a
  window's last left mouse down carrying one `NSPasteboardItem` with both
  plain text and a file URL (upstream's `start_external_drag` carries files
  only). The shelf drags a document with it (ADR 0084). It shows an optional
  `DragCard` (title and first line, light or dark, drawn as a rounded
  `NSImage`) in place of the `.md` icon, and calls `on_end(dropped)` exactly
  once: from `draggingSession:endedAtPoint:operation:` (dropped when the
  operation is not `NSDragOperationNone`), with `false` when a later drag
  starts first, or at once when the drag cannot start. The source window and
  the image are retained until the drag ends, so the end still arrives after
  the window closes. `DragCard` is exported from `gpui_macos.rs`.
- `presence.rs` (new), exported from `gpui_macos.rs`: `hide_window`
  (`orderOut:`, which GPUI lacks) and `set_dock_visible` (the activation
  policy, Regular or Accessory), so with the menu bar item on, closing
  mdoc's window hides it and mdoc leaves the Dock until it shows again
  (ADR 0084); GPUI's `Window::activate_window` shows the window again.
  Debug builds assert both run on the main thread and that AppKit accepts
  the policy. mdoc's tests cover the rules that call them
  (`src/menu_bar.rs`); the native effect is a GUI check.
- `Cargo.toml`: `[lints.rust.deprecated]` is set to `allow`. Upstream
  still calls the deprecated `cocoa` crate (716 warnings). Cargo hides
  warnings from registry crates, but prints them for this one because it is
  patched in from a local path.

Drop this directory, the patch entry and the `gpui_macos` dependency once the
adopted GPUI family's `prompt_for_new_path` takes a message, the app can
answer `applicationShouldTerminate:`, and GPUI offers a status item, a
text drag, window hiding and the activation policy. When updating GPUI, re-vendor the
adopted version and reapply only these changes.
