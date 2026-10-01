# Retain file-based document sessions and lazy paired tabs

Status: accepted current behavior, consolidated 2026-10-01 from the earlier
sidebar, bulk-opening, and opening-UI contracts.

Keep documents as ordinary UTF-8 files and retain one live editor/source-preview
pair per tab. Lazy admission and initialization bound multi-file opening without
rebuilding editors on tab switches. A paths-and-view-metadata session manifest
restores reading state without becoming a notebook database or draft store.

## Invariants

- Open accepts files, not folders, appends in picker order, and activates the
  first selection. Existing path identities are reused without replacing edits.
  Admission runs off the UI thread; unsupported files are summarized and failed
  tabs retain Retry/Close.
- PDF/DOCX converts on first activation when OCR is unnecessary. Hidden unvisited
  tabs stay lazy; other supported formats retain explicit conversion. OCR-required
  PDFs wait for an inline choice even when the runtime is ready.
- One Markdown conversion runs globally; DOCX preview workers are separately
  serialized. Completion stays in its originating tab and never steals focus.
  Closing rejects results immediately; an active synchronous converter keeps its
  slot until it returns. Stable tab identity and operation generations reject
  stale callbacks.
- Conversion creates unsaved Markdown while preserving the original. Untouched
  automatic output closes without prompting and restores from the source path.
  Editing enables normal save protection; successful conversion cannot overwrite
  later edits by running again.
- Live switching preserves text, undo, selection, search, and reading positions.
  Only the active view renders; initialized resources remain until tab close.
  No eviction policy is currently introduced.
- Preview identity and visibility are separate: hiding retains the attachment.
  Failed replacement preserves the loaded preview/comments; accepted replacement
  releases the old PDF view before dropping its DOCX temporary PDF.
- Save uses atomic replacement and external-change detection. Save As cannot
  overwrite an imported source or another open tab's file. Canceling a save
  cancels its pending close/quit operation.
- Quit resolves dirty tabs before teardown and waits for the final session
  checkpoint. Cancel retains live tabs; completed saves remain saved.
- Session JSON contains ordered paths, active tab, attachment/visibility and
  reading metadata, never unsaved text, undo, cached pages, or temporary DOCX
  paths. Restore reads current disk contents lazily and clamps positions. Missing
  files remain unavailable with Retry/Close; corrupt metadata produces a notice.
  Writes are coalesced, serialized, atomic, and off the UI thread.
- Only an automatic untouched placeholder is disposable. Explicit New protects
  both blanks; editing permanently protects a placeholder even if emptied later.
  Open removes it only after successful admission; persist this provenance.
- Local images/links resolve from the Markdown directory, or the original source
  directory before saving converted content. Rendering and saving preserve source
  Markdown rather than normalizing it.
