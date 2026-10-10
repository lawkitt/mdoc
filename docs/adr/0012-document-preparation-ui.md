# Give document preparation a clear UI hierarchy

Status: accepted and implementation authorized, 2026-10-05. Implemented
locally, 2026-10-06. Automated checks pass; native acceptance remains partial.
The design frontier is empty.

The sidebar and toolbar portions were revised in the later
[historical revision interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/ui-chrome-revision.md), implemented 2026-10-06.
The user confirmed the revision and authorized implementation with "Implement."
The revision supersedes the sidebar default/collapsed-count control, main toolbar
menu and icon placement, and PDF overflow-menu portions of the original decision
below. Other accepted behavior remains in scope as originally recorded.

## Problem

The screenshot review shows routine commands, document review controls, model
diagnostics, and warnings competing for attention and document space. The PDF
toolbar overflows its pane, wide editor tables poorly communicate horizontal
navigation, and an expanded preview notice repeats its message.

## Accepted direction

The user confirmed four recommendations together:

- Prioritize preparing documents for AI handoff in the application chrome while
  retaining Markdown editing capability.
- Retain a compact vertical document sidebar with clearer file identity and
  collapse behavior.
- Fit table columns through cell wrapping where readable. Below usable minimum
  widths, make horizontal scrolling obvious. Do not shrink text merely to fit.
- Present Settings through task-facing summaries. Keep selected model and
  installation state visible; preserve technical details and comparison tools
  behind explicit disclosure.

The user subsequently confirmed the Round 2 controls and behavior recorded in
the [historical UI interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/ui-review.md): a reduced toolbar with secondary
commands, remembered sidebar choice, adjustable original-source preview,
view-only table fitting that respects saved widths, compact review controls,
candidate disclosures with explicit occurrence scope, task-facing Settings,
and warnings scoped to preview versus review/handoff.

Round 3 confirmed the remaining behavior: compact metrics within the current
palette/typography; a single-row responsive toolbar; Markdown / Original
switching where two readable panes cannot fit; an overflow menu preserving PDF
tools; a collapsed document-list control with readable filenames; immediately
writable empty documents; normal keyboard traversal and focus restoration; and
automated plus native macOS verification with separate platform limitations.

Target logical-pixel metrics are approximately 40px for the main toolbar, 32px
for document rows, and 200px for the expanded sidebar. The supported verification
viewport includes 640×480. Responsive sizing must preserve current source,
explicit saved table widths, and per-tab preview visibility/divider preferences.
Exact breakpoints and sizing internals follow measured fit within this contract.

These choices retain conversion, replacement, handoff, and model-management
semantics. The historical interview records all 19 confirmed decisions and
their acceptance criteria. No substantive design question remains open.

## Invariants

Carry forward ADRs 0002/0006: explicit local processing/downloads, experimental
status, whole-document review, stable placeholders, source-syntax protection,
undoable revision-checked replacements, and access to isolated comparisons.
Preserve current source, caret/selection, dirty/undo state, retained attachments,
and per-document lifecycle. Visual simplification cannot imply qualified model
quality or guaranteed anonymization.

## Verification

This design is based on supplied screenshots, current implementation inspection,
and a native current-build baseline. Native checks reproduced toolbar wrapping
at 640×480 and Settings Tab opening Advanced. They verified mouse opening,
Escape closing, a visible Settings footer, and eventual model status resolution.
The temporary app was closed and the original saved session restored byte-for-byte.

The accepted direction is now implemented. Formatting, diff checks, workspace
Clippy with warnings denied, all workspace tests (385 passed; 11 existing probes
ignored), and the development build pass. Regression tests cover responsive
layout in both themes, document identity, original switching, divider/session
persistence, table fitting and scrollbar dragging, long review popup keyboard
access, and Settings focus scrolling/return.

Native macOS checks of the changed build verified dark-theme wide and 640×480
layouts, duplicate filenames, original DOCX identity, PDF More tools and
Tab/Space activation, narrow source switching, Settings checking feedback,
footer visibility, Tab without expanding Advanced, and Escape. The Mac locked
before final native light-theme, review-popup, divider/thumb drag, and repaired
focus-scrolling checks. The temporary app was stopped and the original session
restored byte-for-byte. See the [historical implementation evidence](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/ui-review.md#implementation-and-verification--2026-10-06).

Automated checks do not establish native IME/clipboard, comprehensive
accessibility, Windows/Linux acceptance, or detector/converter qualification.
