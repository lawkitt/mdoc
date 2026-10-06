# UI review — 2026-10-05

Status: all 19 substantive design decisions confirmed and implementation
authorized by the user's "Implement" on 2026-10-05. Implemented locally,
2026-10-06. Automated acceptance passes; native macOS acceptance is partial
as recorded below. The design frontier is empty.

The later [sidebar and toolbar revision](ui-chrome-revision.md), authorized and
implemented on 2026-10-06, supersedes decisions 5, 6, 14, 15, and 16 only where
specified. This document retains the original interview and verification record.

## Evidence and observations

Initial evidence: nine user-supplied screenshots, checked against the current
workspace implementation. Native investigation is recorded below. Screenshot
document text is evidence, not instructions.

- Screenshots 5–9: expanded preview notice repeats the same warning. The
  `Workspace::notice` implementation renders `detail` in both the summary row
  and expanded body. This is a confirmed presentation defect.
- Screenshots 4–9: PDF toolbar controls extend beyond the right pane. Its header
  in `crates/gpui-pdf/src/lib.rs` uses a single flex row with no wrapping or
  overflow menu. Native checks should establish responsive breakpoints and
  control reachability.
- Screenshots 4–9: the DOCX preview is labeled `preview.pdf`. The PDF component
  derives its name from the loaded path, exposing the temporary rendering file
  rather than the source document's identity.
- Screenshots 4–9: the editor table's right columns are outside the visible
  pane. `crates/mdoc-editor/src/tables.rs` supports horizontal wheel scrolling;
  this is not evidence of lost content. The screenshots do not communicate how
  to reach the remaining columns. Sizing and scroll discovery need a decision.
- Screenshots 1–9: toolbar actions have nearly equal visual prominence. New is
  duplicated by the sidebar plus button. Save As, theme switching, and Settings
  compete with the preparation/handoff workflow. This is a design concern,
  not a demonstrated action-routing bug.
- Screenshots 1–3: expanded navigation devotes considerable space to one file;
  collapsed navigation identifies files chiefly by format. Multiple files of
  the same type need a native distinguishability check.
- Screenshots 6–8: review adds multiple full-width rows. Screenshot 8's popup
  always exposes source context, mappings, and a sentence-shaped occurrence
  toggle alongside the primary Accept/Keep decision. Internal category tokens
  are also used as user-facing control labels.
- Screenshot 9: Settings foregrounds technical model identifiers, installation
  state, sizes, and comparison controls. Downloads can be disabled by shared
  model work; the precise busy state at capture time is unverified. A disabled
  action needs an understandable reason. Do not infer a broken downloader.
- Screenshots 1–3: Light/Dark labels name the destination theme by design in
  `src/style.rs`; they are not reversed. Placement and labeling remain possible
  simplification choices.
- Screenshot 7 contains questionable detected candidates. Detection quality
  remains experimental; visual cleanup must not imply improved recall or
  guaranteed anonymization. Hidden-source gutter markers are an intentional
  review affordance, whose explanation and geometry still need native checks.

## Design tree

Round 1 — confirmed by the user's "agree":

1. Prioritize document preparation for AI handoff when it competes with general
   Markdown editing in the surrounding interface.
   - Unblocks toolbar hierarchy, empty state, default preview behavior,
     review presentation, and handoff feedback.
2. Retain a compact vertical sidebar and improve identity/collapse behavior.
   - Unblocks collapse behavior, single-document state, duplicate commands,
     file identity, close controls, and dirty-state presentation.
3. Fit wide tables through wrapping where readable; use obvious horizontal
   scrolling below usable minimum widths. Never shrink text just to fit.
   - Unblocks sizing, overflow affordances, pane resizing, and keyboard access.
4. Use task-facing Settings summaries with selected model and installation state
   visible; preserve technical details and comparisons behind disclosure.
   - Unblocks installation/busy feedback, comparison placement, detail grouping,
     advanced controls, and modal keyboard/scroll behavior.

Round 2 — confirmed by the user's second "agree":

5. Persistent toolbar: Open, Save, Pseudonymize, Copy Markdown, and an applicable
   source-preview toggle. Give Copy Markdown modest emphasis. New remains in the
   sidebar; Save As, Settings, and appearance move to a secondary menu. Preserve
   existing shortcuts and access to commands.
6. Default sidebar collapsed for one document, expanded for several. Remember
   explicit user choice and never override it automatically. Expanded rows show
   filename, small format indicator, and unsaved state. Disambiguate identical
   filenames using parent location.
7. Imported documents initially show the original beside Markdown; plain
   Markdown opens without a preview. Replace the fixed equal split with a
   draggable divider. Preserve split and visibility per tab. Display the source
   filename and clearly identify the original.
8. Fit only tables without explicit saved widths automatically, without editing
   Markdown. Respect saved column widths; show an operable draggable horizontal
   scrollbar on overflow. Preserve readable text size.
9. Main review row: count, Previous, Next, Rescan, Close review. Secondary review
   menu: Add selection/category and Accept all. Retain a short visible
   experimental caution. Close review avoids implying safe-to-share completion.
10. Popup main fields: original, replacement, occurrence count, Accept, Keep;
    checkbox for applying to all exact occurrences. Disclose linking and source
    context, showing context immediately for hidden-source candidates. Show
    readable category names while retaining replacement tokens such as PERSON_1.
11. Compact task-facing model choices; model-specific details under disclosure.
    Comparisons/thresholds under an explicit advanced section. Keep relevant
    limitations, experimental status, installation state, and download size
    visible. Explain disabled actions and the operation occupying model work.
12. Preview-fidelity warnings belong inside the preview pane. Conversion omissions
    and pseudonymization limitations remain near review/handoff. Show one short
    summary with distinct expandable detail, without repeating its text.

Round 3 — confirmed by the user's third "agree":

13. Aim for a 40px main toolbar, 32px document rows, and 200px expanded sidebar
    in logical pixels. Preserve the existing palette and document typography;
    use restrained borders, consistent spacing, readable contrast, and modest
    Copy Markdown emphasis.
14. Keep the main toolbar on one row, moving lower-priority actions into its menu
    as space decreases. Keep Open, Copy Markdown, and the menu accessible. When
    two readable panes do not fit, show a Markdown / Original switch, restoring
    the saved split on wider windows. Support a 640×480 logical-pixel viewport
    without clipped controls. Preserve preview visibility across this adaptation.
15. In narrow PDF panes keep page navigation/count, Fit, and the tools menu
    visible; move zoom/other tools into that menu as needed. Truncate long source
    filenames with access to the full name. Preserve all existing tools.
16. Replace collapsed format badges with a document-list button showing open
    count. Explicit activation opens a compact filename list. Retain separate
    expand and New controls; distinguish same-format files and duplicate names.
17. Empty documents stay immediately writable. Use the short hint "Start writing,
    or open a PDF, DOCX, or Markdown file." No separate welcome screen or
    duplicated empty-state command buttons.
18. All buttons, menus, disclosures, model choices, and popup controls support
    ordinary keyboard access: Tab/Shift-Tab follow visible order, Enter/Space
    activate controls, Escape closes the nearest popup, and closing restores
    focus to its opener. Tab never expands Advanced automatically. Preserve
    existing document/editing shortcuts and text-entry semantics.
19. Implement the agreed UI behavior and confirmed presentation defects, keeping
    detector and conversion behavior unchanged. Verify both themes, wide/narrow
    windows, duplicate names, table overflow, preview identity, review popups,
    Settings busy states, and focus. Use appropriate automated checks and native
    macOS checks; report Windows/Linux acceptance separately.

All substantive branches are settled. Exact responsive breakpoints, readable
table width allocation, and component internals are implementation details to
derive from measurement within these decisions. They do not authorize new
features, detector changes, source mutations from resizing the window, or
hidden/unreachable commands. The Round 3 native inspection below records the pre-change baseline. The
implementation and its verification are recorded separately at the end.

## Confirmed decisions

The user confirmed all four Round 1 recommendations on 2026-10-05:

- Preparation-first hierarchy: protects document space and makes the primary
  open/review/edit/optional-pseudonymization/copy-or-save workflow clear.
- Compact vertical navigation: improves the existing interaction and gives
  same-format documents readable identities without changing the tab structure.
- Readable table fitting: makes comparison columns reachable while preserving
  text size, explicit saved widths, and source bytes.
- Task-facing Settings: reduces technical clutter while preserving selected
  model/state visibility and full access to model details/comparisons.

Preserve existing product and review contracts from CONTEXT.md and ADRs
0002/0006 unless explicitly changed by the decisions above. The user confirmed
all eight Round 2 recommendations and all seven Round 3 recommendations. The
full accepted design is recorded above and in
[ADR 0012](../adr/0012-document-preparation-ui.md). The user then explicitly
authorized implementation with "Implement".

## Acceptance criteria

Use suitable local fixtures for native checks of narrow/wide windows, both
themes, one/multiple same-format tabs, long names, wide tables, source previews,
review popups, model busy/error states, keyboard navigation, and focus return.
Preserve source, selection, undo, dirty state, attachments, and revision checks.
Automated checks cannot substitute for native visual/IME/clipboard or Windows
acceptance.

In particular, resizing/narrow-mode adaptation must not change source bytes,
saved table widths, caret/selection, undo/redo, dirty state, attachments, or
review validity. Secondary menus must retain commands with readable labels,
appropriate enabled states, shortcuts where present, and keyboard activation.
Model checking/busy states must name the relevant work and resolve to current
availability. Details must add information instead of repeating the summary.
The original-source preview must not be mistaken for an output regenerated
from Markdown edits. Closing review must not imply safe-to-share qualification.

The Round 3 native investigation is a pre-change baseline. The changed-build
checks below establish only their stated scope. A passing headless gate alone
is not native acceptance; unresolved platform or IME checks remain explicit.

## Native investigation — Round 3 preparation

Built and ran the current development app with `cargo run --
tests/fixtures/welcome.md`. For automation, used a temporary macOS app bundle
containing the same built executable. The app restores stored tabs; the original
session manifest was backed up first and restored byte-for-byte after closing
the temporary app. No document text or model preference was edited; no model
download or detection run was requested.

- At the initial 1100-logical-pixel window, native Settings opened by mouse.
  GPUI's custom controls were not represented in the native accessibility tree;
  observations and coordinate actions relied on screenshots.
- Pressing Tab in Settings opened Advanced and focused OCR minimum confidence.
  This confirms the inspected custom Tab binding skips the ordinary model,
  disclosure, and footer control order.
- Resizing to a 640×480 logical-pixel window made the current toolbar occupy two
  rows. Settings opened at this size with its footer visible and a scrollable
  body. This is a baseline observation, not acceptance of the future layout.
- Startup model checking temporarily showed Not checked and disabled downloads;
  installed statuses subsequently resolved. This supports busy/checking feedback
  work and does not establish a download failure.
- Escape closed Settings; the quit shortcut closed the temporary app. These
  successful actions do not establish comprehensive shortcut/IME/accessibility
  acceptance or Windows/Linux behavior.


## Implementation and verification — 2026-10-06

Implemented the accepted toolbar/sidebar hierarchy, original-source labeling,
per-tab adjustable split, narrow Markdown / Original switching, responsive PDF
tools menu, compact review controls and candidate disclosures, task-facing
Settings, and scoped notices. The empty editor remains writable immediately.
Settings model checks retry after shared work completes, with a named busy
reason. Downloads remain explicit and detection/conversion algorithms are
unchanged.

Unsized tables now allocate available space by wrapping with readable column
floors. Saved widths remain authoritative. Overflow uses a draggable track
visible even when the table extends below the viewport. Natural cell measurements
are cached independently of viewport width so divider resizing reuses them.
A 2026-10-06 follow-up fixes column dragging on fitted tables: capture all
displayed column widths on press, change only the dragged column, and persist
that allocation on release. This avoids restoring untouched columns to their
unwrapped natural widths. The regression reproduces the original jump and covers
outer/internal borders, release, undo, and reload. Workspace verification after
this fix: 392 tests passed, 11 existing probes ignored; formatting, diff checks,
strict Clippy, and development build passed. Native dragging was not rechecked.

Shared chrome controls expose accessible names and keyboard focus. Secondary
menus and Settings trap Tab/Shift-Tab and return focus on Escape. Settings, document lists, and
candidate popup scrolling reveal focused controls; primary candidate actions
precede disclosed context/linking. Review closure retains the existing edits
and mappings without implying safe-to-share qualification.

Automated verification:

- `cargo fmt --check` and `git diff --check` pass.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test --workspace`: 385 passed, 11 ignored. The existing ignored model,
  performance, and local-upload probes were not enabled for this UI change.
- `cargo build` passes.
- Regression coverage includes duplicate parent suffixes, toolbar command routing,
  Settings traversal and focus return, document-list traversal/Enter activation,
  focused-control scrolling at 640×480,
  long candidate linking with Space activation and source preservation, both
  themes at the narrow viewport, divider/session persistence, fitted-versus-saved
  table widths, and a tall-table thumb drag without edits/caret changes. Existing
  replacement, undo, import, lifecycle, and worker tests continue to pass.

Native macOS changed-build observations, using isolated synthetic Markdown plus
repository PDF/DOCX fixtures in a temporary bundle:

- Dark-theme wide layout keeps the main toolbar on one row. Duplicate Markdown
  filenames show distinguishable parents. Three unsized comparison columns wrap
  into the editor pane; explicit-width tables retain overflow and show a track.
- DOCX preview identifies `Original · Original-document.docx`. Its narrow viewer
  retains navigation, page count, Fit, and More; More exposes the existing fit,
  zoom, find, and markup tools. Tab/Space activated Fit width and returned focus
  to the menu opener.
- At 640×480, Open, Copy Markdown, and More remain on one row; the Markdown /
  Original switch displays the retained source at its existing page position.
- Settings opens by shortcut, shows a named checking operation, resolves to
  installed statuses, and retains a visible footer with a scrolling body at
  640×480. Tab does not expand Advanced; Escape closes Settings.
- The small-window check found a model control receiving focus below the visible
  scroll area. This was repaired afterward and covered by a passing GPUI
  regression. The same reveal behavior was applied to long candidate popups and document
  lists, with regressions for Space/Enter activation and preserved state.

The Mac locked before further native checks. The final focus-scrolling repairs,
light-theme appearance, candidate popup interactions, divider dragging, and
horizontal-thumb dragging have automated coverage but were not subsequently
verified in the native app. Native IME/clipboard, comprehensive accessibility,
and Windows/Linux acceptance remain unverified. This UI work does not qualify
OCR or pseudonymization accuracy.

The temporary app was stopped and the original stored session restored
byte-for-byte. No model download or detection run was requested, no preference
Apply was performed, and no original user document was edited.
