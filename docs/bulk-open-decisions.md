# Bulk Open and explicit Markdown conversion

Status: implementation authorized and implemented; verification recorded below.

## Confirmed — round 1

- **Open…** is the single file-selection entry point and accepts one or more
  files. Folder selection is out of scope.
- Each selected supported file opens using its normal behavior in its own tab:
  Markdown is editable; PDF/DOCX opens as a preview without automatic Markdown
  extraction. DOCX preview rendering is distinct from Markdown conversion.
- Markdown conversion is an explicit action after opening a source file.
- When conversion needs OCR for all or some pages, ask for confirmation before
  running OCR, including when the local OCR runtime is ready.
- Append new tabs; reuse already-open file identities rather than duplicate
  them. Preserve existing tabs and their unsaved edits.

Rationale: one predictable file-opening flow, with conversion and OCR under
explicit user control.

## Relationship to existing decisions

This design supersedes the earlier sidebar contract where Open attaches a
PDF/DOCX to the active Markdown tab, and the OCR contract where a ready runtime
automatically runs recognition during import.

## Confirmed — round 2

- Convert to Markdown transforms the source tab into an unsaved Markdown editor
  alongside the unchanged original preview. Save suggests `source-name.md` and
  never overwrites the source implicitly.
- Supported formats without native previews open a source tab with the filename,
  a Preview unavailable message, and Convert to Markdown. Preserve existing
  conversion format support.
- When OCR is ready, confirmation offers Run OCR, Skip OCR, and Cancel. When
  components are missing, offer Set up and run OCR, Skip OCR, and Cancel.
- Skip OCR extracts available native text with page-specific omission warnings
  outside the Markdown. If no usable content is extracted, retain the source
  preview unchanged. Cancel leaves the source tab unchanged.
- Unconverted source tabs restore across restart using their source paths and
  close without save prompts. Converted content or edits require normal unsaved
  document protection.

Rationale: conversion creates the editable document only when requested;
opening and reading source files does not create a save obligation.

## Confirmed — round 3

- Add selected files as tabs immediately in picker order, activating the first
  selected file. Initialize other tabs only when selected. Reuse existing file
  identities without moving or resetting their tabs.
- Report unsupported files in one summary. Keep load failures on their affected
  tab with Retry and Close; other selected files remain usable.
- Allow one Markdown conversion at a time globally. Disable Convert elsewhere
  while busy; tab switching remains available. Completion never steals focus.
  Closing the source tab cancels its pending result, with existing cooperative
  cancellation limits applying to underlying synchronous conversion work.
- Hide Convert after successful conversion. Reopening the source activates its
  existing paired tab rather than replacing Markdown or creating a duplicate.
- If OCR is unavailable on the platform or fails, explain the reason and offer
  native text only or Cancel. Offer Retry for recoverable failures. Never
  silently omit scanned pages; the native-text-only path follows round 2's
  warnings and no-usable-content behavior.

Rationale: lazy loading bounds initial bulk-open work, tab-local errors preserve
partial success, and explicit conversion admission protects existing edits.

## Final confirmation

The user agreed to all three rounds and requested implementation. Existing tab lifecycle, source preservation, save protection,
and platform verification requirements continue to apply except where this
document explicitly changes the earlier contracts.

## Implementation and verification

- Open uses one multi-file picker; command-line files, OS file-open events, and
  local links use the same tab-opening path. Format admission and path identity
  checks run off the UI thread. Unknown extensions retain content-based detection
  with a bounded 256 MiB read; known formats do not need content reads at admission.
- Source-only tab records persist source paths and preview positions without
  inventing an unsaved Markdown document. Inactive tabs allocate no document view.
  Source-only PDF/DOCX previews occupy the full content width.
- Convert acts on the current source, preserves an already-loaded preview, and
  creates unsaved Markdown in place. Initial extraction does not load OCR models;
  recognition follows explicit confirmation. A shared conversion permit spans
  workers, confirmation dialogs, and setup. Closing the tab rejects its results;
  its worker retains the permit until it exits.
- Automated coverage includes picker batch ordering, deduplication, lazy
  initialization, existing edits, source-only restoration/close, missing-source
  retry, same-tab conversion, retained preview identity, ready-runtime consent,
  explicit fallback after OCR failure, busy admission, and closed-tab results.
- Native picker, visual layout, keyboard interaction, and Windows/Linux execution
  still need platform checks; headless tests do not establish visual acceptance.
