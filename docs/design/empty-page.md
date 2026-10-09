# Empty page and file drop — 2026-10-09

Status: shared understanding confirmed and implementation authorized by the
user's "agree, implement" on 2026-10-09. Implemented locally; verification
below. Recorded as [ADR 0028](../adr/0028-empty-page-and-file-drop.md).

## Evidence

User screenshot: a blank "Untitled.md" tab shows only the editor placeholder
"Start writing, or open a PDF, DOCX, or Markdown file." at top-left of an
otherwise empty editor.

Current code:

- `Open` (`src/main.rs`, `Workspace::open`) already prompts with
  `multiple: true` and emits `TabEvent::Open(paths)`.
- `Tabs::open_paths` / `finish_open` (`src/tabs.rs`) skip unsupported files with
  a notice ("Unsupported files skipped: …"), focus an existing tab for an
  already-open path, append new tabs at the end, and discard a disposable blank
  tab.
- No external (OS) file drop exists; `on_drop` is used only for internal
  `TabDrag` reorder and PII panel drags.
- No recent-files storage exists.

## Confirmed decisions (round 1)

1. **Visibility**: the empty page shows on any Markdown tab whose content is
   empty (new window "Untitled", any **+** tab). It hides on the first
   keystroke and returns if the content becomes empty again.
2. **Non-blocking**: the editor keeps focus; typing works immediately. Only the
   button (and hint) are interactive; clicking elsewhere places the caret.
3. **Content**: centered both axes — document icon; primary **Open files…**
   button (dispatches the existing `Open` action); line "PDF, DOCX or Markdown ·
   select several to open each in its own tab"; shortcut hint **⌘O**
   (platform-appropriate); faint "or just start typing". Replaces the old
   top-left placeholder; a short "Type here…" ghost text stays at the caret.
4. **External file drop** (user revision): accepted on the **whole window while
   the empty page is shown**, and on the **document sidebar** (Chrome-like) in
   any state. Both reuse `TabEvent::Open` semantics.
5. **Recent files**: out of scope (no storage; privacy of legal documents).
   Deferred idea.
6. **Style**: filled primary button in the existing Copy Markdown green;
   muted theme text; follows light/dark theme.

## Confirmed decisions (round 2)

7. **Sidebar drop position**: an insertion line between documents (same accent
   line as tab reorder); dropped files open as new tabs at that position in
   selection order. Over a document, top/bottom half means before/after — a drop
   never replaces a document. Already-open files focus their existing tab and
   do not move; the disposable blank tab is still discarded. (Requires
   `open_paths` to accept an insertion index; default stays "append".)
8. **Collapsed rail**: dragging files over it triggers the ADR 0027 hover
   reveal (~200ms); it stays open during the drag and closes after the drop or
   ~300ms after leaving. The saved sidebar choice is unchanged.
9. **Drag feedback on the empty page**: dashed accent border around the editor
   area and "Drop to open". If no dragged path has a supported extension, show
   muted "No supported files" without highlight. Mixed drops open supported
   files and show the existing skipped notice.
10. **Folders**: skipped with the existing notice. Opening a folder as a
    Chrome-style **tab group** is deferred to the [Roadmap](../../ROADMAP.md).

## Invariants

- The empty page never captures typing or steals editor focus.
- Every open path (button, ⌘O, drop) goes through `Tabs::open_paths`, so
  dedup, unsupported-file notice and blank-tab disposal stay uniform.

## Implementation notes

- `Workspace::shows_empty_page` (`src/main.rs`): not loading/unavailable/
  source-only, no source or attachment, empty editor text. Converted Markdown
  beside a source never shows the empty page. The editor keeps a short ghost
  text, "Type here…", at the caret (user revision, 2026-10-09); the old long
  placeholder is gone.
- The overlay (`Workspace::empty_page`, `src/workspace_ui.rs`) has no hitbox
  except the button, so clicks reach the editor.
- Drag feedback uses `import::supported_extension` (extension only;
  `supported_source` reads file contents). Unsupported-only drops are refused by
  `can_drop`.
- GPUI reports no hover during a drag, so the rail uses `on_drag_move` to feed
  the ADR 0027 reveal (`file_drag_rail`). Drag state is cleared on render once
  no drag is active (drop or the pointer leaving the window).
- `Tabs::open_paths_at` anchors the insertion on a tab id, so tabs closing
  while paths resolve cannot misplace the new ones.

## Verification

Automated: `cargo fmt`, `cargo clippy --all-targets` clean, `cargo test --bin mdoc`
205 passed. New tests: empty page opens several files and replaces the blank
tab, hides on typing and returns when emptied; dropping on the empty page
(unsupported refused, mixed opens supported with the notice, non-empty document
not a target); sidebar drop at the upper half of a row and below the last row,
with an already-open file not moving; drag over the collapsed rail reveals the
list, drops at the end, and closes after the pointer leaves without changing the
saved choice.

Native macOS check (dark theme, collapsed rail, blank tab): the empty page is
centered with the icon, green **Open files… ⌘O**, format line and faint hint.
Not checked natively: a real Finder drag, light theme, Windows/Linux.
