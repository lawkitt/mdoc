# First-use model setup and English-first defaults

Status: Accepted and implemented, 2026-10-09; Windows native check outstanding. See the
[historical design interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/model-setup-ux.md). Amends
[ADR 0006](0006-local-model-settings.md) defaults and Settings controls.

## Problem

First use of OCR or Pseudonymize starts a download without showing its size,
pops the Settings dialog open, and (for Pseudonymize) first runs a scan that
fails. Settings blurs "selected" with "installed", shows per-file progress and
leads with engine names and Russian caveats. Defaults favour Russian, which is
no longer a priority.

## Decision

- **Inline consent card** at the point of use states what downloads, the total
  size and that processing then stays offline; **Download & continue**
  downloads and resumes the task, **Choose model…** opens Settings. Settings
  never opens itself. Pseudonymize with a missing model shows the card instead
  of a failing scan.
- **Progress** reports total bundle bytes, then Verifying / Checking runtime,
  with Cancel, in the card and in the Settings row.
- **Settings rows** show one status label, plain-language titles and
  "Downloads on first use" for a selected missing model; technical details move
  under Details. Model choices save immediately; footer is Reset defaults and
  Done.
- **Defaults**: OCR PP-OCRv6 Small (English); pseudonymization GLiNER2 FP16.
  Cyrillic OCR and FP32 remain optional. Stored preferences are not migrated.

- **Markdown pane**: every not-yet-converted state uses one centred state
  card with a single primary action, a quiet secondary and a link; it changes
  in place through download, verification, recognition and failure. Page
  lists use ranges. The duplicate top notice appears only when the Markdown
  pane is hidden.
- **Leaving and failure**: downloads continue globally; the resume belongs to
  the requesting tab. Cancel or failure returns to the card with Retry,
  keeping verified files. Settings downloads only enable one-click Run.

## Unchanged

Explicit network consent, offline inference, one model job at a time,
generation-scoped continuations, Settings downloads never starting document
processing, Repair/Remove, comparisons and advanced numeric settings.
