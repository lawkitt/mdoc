# Consistent loading feedback and action-oriented dialogs

Status: accepted and implementation authorized by the user's "implement",
2026-10-07. Implemented locally. Design frontier empty.

## Problem

Plain static loading labels fail to communicate activity visually. Replacement
popups devote excessive space to labels, secondary explanations and separated
quiet actions, obscuring the transformation and immediate choice.

## Accepted direction

- Use one restrained activity style across app-owned loading states: accent
  spinner and short task label, with real progress bars where measurable.
- Apply consistent padding, borders, fields and button hierarchy to app-owned
  popups/dialogs; keep native operating-system dialogs.
- Make original text → replacement token the dominant visual content of
  replacement review and inspection popups.
- Reduce nonessential popup information without adding Details disclosures.
  Retain short experimental caution near review/handoff.
- Show task labels immediately and delay animation about 150 ms. Keep usable
  content visible, preserve existing cancellation and use measured progress only.
- Preserve editable pseudonym tokens, wrapping/stacking long transitions and
  explicit exact-match scope. Use primary Replace / secondary Keep in review;
  primary Restore this / secondary Restore all N matches for applied replacements.
- Remove the permanent restoration footer and redundant labels; retain actionable
  errors, essential context and existing Settings Advanced/model tools.

The [interview](../design/loading-and-dialogs.md) records all confirmed decisions
and acceptance criteria. These presentation decisions preserve
processing behavior, explicit activation, cancellation/stale-result guards,
replacement/Keep/undo semantics, source state and live-memory provenance.

## Implementation and verification

Shared presentation helpers in `src/ui.rs` provide panel surfaces, action buttons,
original → replacement chips, delayed accent activity spinners and byte-backed
progress bars. GPUI animation support honors reduced motion and caps animation
refreshes at 30 fps. The delay applies on mount; usable content stays visible.

The presentation covers Markdown/source loading, conversion/OCR, Markdown/PDF
search, PII scans, Settings checks/application, model work/downloads, comparisons
and active tab work. PDF hosts can supply the same activity renderer for file
loading and active page preparation; unscheduled pages and queued jobs do not
animate. Background OCR readiness checking does not add a workspace status row.
Download progress is explicitly per current file and disappears for verification
or runtime checking, where retained byte counts would be misleading.

Replacement popups use the shared transition, wrapping, accent token and clear
Replace/Keep or scoped restoration buttons. The restoration footer is removed;
no Details controls were added. Existing context/linking and Settings tools remain
available. Choosers, document lists and action menus share the panel surface.
Outside-click closure of replacement/chooser popups now restores focus through
its existing close handler.

Formatting, diff checks, workspace Clippy with warnings denied, workspace tests
(416 passed; 15 existing probes ignored), and the development build pass. Cargo
still reports its existing unstable registry-setting and upstream `block` future-
incompatibility notices; these are not new application lint warnings.

New automated checks exercise actual restore-button clicks, same-original scope,
single-match action visibility, both themes at 1100×760 and 640×480, long Cyrillic
transition wrapping, keyboard replacement, focus return and undo. Existing suites
also cover cancellation/stale results, model/Settings access and document lifecycle.

An isolated temporary native macOS component preview verified spinner rendering,
progress/transition contrast, long-value wrapping and action hierarchy in both
themes, and removal of activity feedback on completion. It held no document,
model or saved-session state and was closed after inspection. Its temporary source
was removed. The existing mdoc process was not interrupted.

This native check covers shared visual components, not every live application
workflow. Full native popup/Settings/PDF/loading/cancellation integration, timing
and reduced-motion acceptance, IME/comprehensive accessibility and Windows/Linux
checks remain unverified. Headless checks do not establish those properties.

## Follow-up: editable token selection — 2026-10-07

A user screenshot exposed unreadable selected replacement text in light theme.
The shared single-line input painted an opaque hard-coded green selection over
text inheriting that same accent. `src/markdown_search.rs` now paints selection
with a translucent foreground tint and uses the inherited foreground for its
caret, preserving editing, selection and IME logic.

An isolated native macOS preview using the actual `SearchInput` inside the
replacement transition verified selected token text in both themes and typing
with a visible caret. This corrects the earlier preview's omission of the
editable input. The temporary preview and source were removed; no saved document
or session was used. The focused input and PII popup tests pass.
