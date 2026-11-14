# Vendored: gpui-pre-macos 0.3.12

Source: crates.io `gpui-pre-macos` 0.3.12 (Zed `gpui_macos`,
zed@d2a4fb6ded7c470be25d1541281da3882495b65b; Apache-2.0,
see LICENSE-APACHE). The workspace patches this version without updating the
GPUI family. Registry checksum:
`2de7d3f4ad51ac387e822495db363fcc1188d349793365731d054444c4cb06ee`.
Re-ported from 0.3.2 for [ADR 0117](../../docs/adr/0117-gpui-0-3-12-and-a-quicker-sidebar-reveal.md):
upstream moved this crate from `cocoa`/`objc` to `objc2`, so the patches
below are written against `objc2` and the drag is in `src/text_drag.rs`,
included into `window.rs` to share its window state.

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
  shows one `NSStatusItem` whose button draws a `StatusImage` (the app's
  PNGs at 1x and 2x, loaded as one template `NSImage`) and calls back on
  click, or removes it; `status_item_bounds` gives
  the button's display and bounds (top-left origin) so a
  `WindowKind::PopUp` window can open under it. `window.rs`'s
  `display_id_for_screen` is `pub(crate)` for it. mdoc uses them in
  `src/menu_bar.rs` for the menu bar shelf (ADR 0084).
- `text_drag.rs` (included into `window.rs`): `start_text_and_file_drag` starts an outbound drag from a
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
  `set_window_height_keeping_top` gives a window a new content height with
  its top edge kept and no animation, clamped to its screen's visible frame
  (`setFrame:display:`; GPUI's `Window::resize` keeps the bottom edge), so
  the shelf popup fits its content under the menu bar item. GPUI's view sits
  out that change (its autoresizing is off for it) and is then sized
  through its own `setFrameSize:`, which resizes the drawable; then the
  window re-reads its size with `Window::bounds_changed`, since GPUI's own
  resize callback cannot update a window from inside that window's update.
  AppKit's own redraw of the resized view (`displayLayer:`, presented in
  its transaction) never reaches the screen after such a resize, so the
  layer's contents are pinned to the top (`kCAGravityTop`: an old picture
  is never stretched or squashed) and the window refreshes 20 ms later,
  once that redraw is past, drawing the new size on GPUI's display tick.
  Debug builds assert they run on the main thread and that AppKit accepts
  the policy. mdoc's tests cover the rules that call them
  (`src/menu_bar.rs`); the native effect is a GUI check.
- `pasteboard.rs`, exported from `gpui_macos.rs`: a string written with the
  metadata `CONCEALED_CLIPBOARD_METADATA` is written for this Mac only
  (`prepareForNewContentsWithOptions:` with
  `NSPasteboardContentsCurrentHostOnly`, in place of `clearContents`), so
  Universal Clipboard does not carry it, and carries the empty marker types
  `org.nspasteboard.ConcealedType` and `org.nspasteboard.TransientType`
  (nspasteboard.org), so clipboard managers skip it.
  Every other copy is unchanged. mdoc tags restored text with it
  (`src/restored_clipboard.rs`; recognition grill 2026-11-01, round 3).
- `presence.rs`'s `set_dock_visible` calls AppKit directly, not GPUI
  0.3.12's `App::set_activation_policy`: GPUI treats an Accessory app as
  headless (`graphical_environment` is then `None`), which mdoc's hidden
  window must not be.

Drop this directory, the patch entry and the `gpui_macos` dependency once the
adopted GPUI family's `prompt_for_new_path` takes a message, the app can
answer `applicationShouldTerminate:`, and GPUI offers a status item, a
text drag, window hiding, the activation policy and concealed clipboard text. When updating GPUI, re-vendor the
adopted version and reapply only these changes.
