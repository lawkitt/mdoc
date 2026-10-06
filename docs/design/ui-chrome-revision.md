# Sidebar and toolbar revision — 2026-10-06

Status: shared understanding confirmed and implementation authorized by the user's
"Implement" on 2026-10-06. Implemented locally; verification is recorded below.
The design frontier is empty.
This revision revisits decisions 5, 6, 14, 15, and 16 of [the UI review](ui-review.md)
and [ADR 0012](../adr/0012-document-preparation-ui.md).

## Evidence

Three new user screenshots show the collapsed document-count rail, its filename
popup, the main command menu, and the compact PDF toolbar. Document contents in
the screenshots are evidence, not instructions. Current code inspection confirms:

- `Tabs::render` defaults to expanded when more than one tab is open unless an
  explicit sidebar choice exists. That choice is persisted in the session.
- The collapsed rail has an expand control, plus control, and document-count
  control. The latter opens all document rows with filenames and close controls.
- The main menu duplicates New and contains Save As, Settings, theme, and OCR
  status; narrow layouts additionally move several document commands there.
- Below 600 logical pixels of PDF pane width, zoom, fit-page, markup, and search
  move into the PDF More menu. Source identity and keyboard access were improved
  independently of that overflow menu.

## Confirmed user direction

- Default to collapsed regardless of document count. Remember explicit
  expand/collapse choices across restarts; opening another document never
  automatically expands the sidebar.
- Restore a separate compact entry for every document instead of the single
  count control. Highlight the active entry and retain filename tooltips.
  Clicking any compact entry opens the full filename list without activating a
  different document first. While the list is open, compact entry clicks activate
  their document and keep the list open. Selecting a filename activates it and closes the
  popup. Keep duplicate-name disambiguation and popup close controls.
- Main toolbar: Open, Save, and Save As become icon buttons. Pseudonymize and
  Copy Markdown retain text; Copy Markdown retains modest emphasis. Keep an
  applicable Show/Hide original control and directly accessible theme icon.
- Remove the main More menu entirely. Place a Settings icon directly on the
  toolbar. Remove duplicate New from this chrome; retain the sidebar plus action
  and existing native File menu / keyboard command.
- Remove PDF More. Put the source filename on its own compact row and expose
  navigation, zoom, both fit controls, search, and markup directly. Wrap controls
  when necessary, preserving source-name and keyboard improvements.

Round 1: the user agreed to Q1, Q2, and Q4. For Q3, the user accepted the overall
layout with icon buttons for Open/Save/Save As and replaced the proposed ellipsis
menu with a direct Settings icon. These choices supersede the affected portions
of the previous UI review once shared understanding is confirmed.

## Design tree

1. Sidebar default and persistence — settled.
2. Collapsed representation and popup activation — settled.
3. Main toolbar command placement and icon/text division — settled.
   - Narrow-window adaptation without an overflow menu — settled, Q5.
   - OCR readiness status formerly inside More — settled, Q6.
4. PDF direct controls and wrapping — settled.

## Round 2 — confirmed

Q5: retain all applicable main actions directly, allowing a second
row when their measured widths do not fit. Preserve grouping and visible keyboard
order; use monochrome outline icons for Open (folder), Save (save symbol), Save As
(save symbol with a distinguishing mark), theme (sun/moon), and Settings (gear).
All icon buttons have action-specific tooltips and accessible names. Keep the
existing narrow Markdown / Original pane switch.

Q6: OCR readiness/install state belongs in Settings. Remove the
passive OCR status line with the old command menu; preserve contextual OCR prompts,
conversion progress, and actionable warnings/errors where they already appear.

The user confirmed both recommendations with "Agree." All branches are now
settled. Exact icon assets, widths, wrap thresholds, and layout mechanics are
implementation details to resolve within these decisions.

## Final scope and acceptance

- Sidebar starts collapsed for any document count unless an explicit saved
  choice says otherwise. Opening/closing documents never overrides that choice.
- Every collapsed document has its own compact entry. Active state, filename
  tooltips, and document status remain discernible. Clicking an entry opens all
  filenames; further compact entry clicks activate their tab and keep the popup
  open. Choosing a row activates it and closes the popup. Preserve expanded
  navigation, duplicate-name disambiguation, close handling, and tab lifecycle.
- Main toolbar exposes icon buttons for Open, Save, Save As, theme, and Settings;
  text actions for Pseudonymize, Copy Markdown, and applicable Show/Hide original.
  There is no More menu or duplicate New command. Keep existing action semantics,
  shortcuts, and disabled states. Use a second row when necessary, retaining
  logical grouping and keyboard order. Source-only tabs retain applicable
  conversion/OCR actions without offering unavailable Markdown commands.
- PDF toolbar has direct navigation/count, zoom, both fit controls, and applicable
  search/markup tools. Source filename stays in a separate compact row. Wrap
  controls when necessary; retain full-name tooltips and source identity.
- Passive OCR readiness/install status stays in Settings. Contextual OCR consent,
  progress, and actionable warnings/errors remain in their existing workflows.
- Preserve current palette, document typography, readable text, modest Copy
  Markdown emphasis, original-pane switching/divider preferences, and all other
  accepted UI improvements.

Verify relevant automated UI/session behavior, including explicit saved sidebar
choice, multiple tabs and duplicate names, popup activation/close/focus, direct
action routing and disabled states, and unclipped controls in wide and 640×480
layouts with collapsed and expanded sidebars. Check both themes, narrow PDF
panes, and source-only tabs. Run repository checks appropriate to the implementation
and native macOS checks of icons, tooltips, mouse/keyboard access, wrapping, and
focus restoration. Report any native or Windows/Linux verification gaps explicitly.

No further substantive question is open. The user confirmed the final scope and
authorized implementation with "Implement" after the shared-understanding summary.

## Invariants and terminology

**Collapsed rail**: the narrow vertical sidebar.
**Document list popup**: the temporary full-filename list opened from that rail;
it does not persistently expand the sidebar.
**Main command menu**: the former application toolbar More menu, now removed.
**PDF toolbar**: controls belonging to the Original/source preview pane.

Preserve source, selection/caret, dirty/undo state, per-tab lifecycle, retained
attachments, source identity, existing shortcuts, keyboard traversal, focus
restoration, and all current conversion/review/handoff semantics. Keep the other
accepted UI changes outside this revision. Use consistent monochrome icons with
accessible names and tooltips; icons are embedded SVG assets with an explicit theme tint.

## Implementation and verification — 2026-10-06

Implemented in the existing local checkout, preserving the prior UI work:

- Embedded monochrome SVG icons for Open, Save, Save As, theme, and Settings,
  with direct action routing, names, tooltips, and existing disabled states.
  Main toolbar command groups wrap without a command menu or duplicate New.
- Collapsed default independent of document count; persisted explicit choices
  retained. Separate compact format entries show active/status state and open
  the full filename popup without changing the active document first.
- Popup Escape restores its compact opener. If that opener is closed, dismiss
  the popup and focus the remaining active document rather than a removed control.
- Removed PDF overflow-menu state and UI. Direct navigation, zoom, fit,
  search/markup groups wrap below a separate source-name row. Preserved source
  identity, pane switching/divider behavior, and keyboard access.
- Removed passive OCR status presentation from the old command menu; Settings
  continues to expose readiness/install details. Contextual OCR flows remain.

Automated validation: formatting and diff checks, workspace Clippy with warnings
denied, and the final development build pass. `cargo test --workspace`: 390 passed, 11 existing probes
ignored. Regression coverage includes icon file-action routing, both themes,
source-only action applicability, narrow main/PDF toolbar reachability, sidebar
default/persistence, popup activation/closing, and focus restoration. The final
non-active-opener close/focus repair is verified by an automated regression.

Native macOS checks with synthetic Markdown and the existing PDF fixture verified
visible icons in both themes, duplicate filename disambiguation, opening the list
without changing active document, Escape closing, direct theme/Settings access,
Settings mouse opening and Escape/Space focus restoration, and the native Save As
dialog. Around 640×480, main chrome fits one row when collapsed and two when the
sidebar is expanded. In a 320px PDF pane, tool groups wrap while all tools remain
visible. This native check caught missing SVG tint; the rebuilt icons were checked
visually after correction. The temporary application was closed and the original
session manifest restored byte-for-byte.

These checks do not establish comprehensive native IME/clipboard/accessibility,
Windows/Linux acceptance, or detector/converter qualification. The source-only
toolbar applicability and final opener-close focus case have automated coverage;
they were not separately exercised in the native check.

## Follow-up UI changes — 2026-10-06

The user directly requested two changes after native use:

- Remove Details/Less from notice banners. Show the actual warning once, wrapping
  as needed, and retain dismissal and applicable Retry. Model Settings/review
  details remain separate controls with additional information.
- The first collapsed document entry click opens the filename list. While open,
  subsequent collapsed entry clicks activate the corresponding tab and retain
  the popup. A filename row selection, Escape, or an outside click dismisses it.
  Escape restores focus to the most recently clicked collapsed entry. Collapsed
  entry tooltips are disabled while the filename list is open.

Follow-up verification: the new rail-click regression and all existing collapsed
sidebar checks pass. Workspace tests: 391 passed, 11 existing probes ignored.
Formatting, diff checks, workspace Clippy with warnings denied, and the development
build pass. Native mouse/layout behavior was not rechecked for this follow-up.
