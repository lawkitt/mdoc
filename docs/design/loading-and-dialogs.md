# Loading and dialog UI interview — 2026-10-07

Status: shared understanding confirmed and implementation authorized by the
user's "implement", 2026-10-07. Implemented locally; the design frontier is empty.

## Requested outcome and evidence

The user requests more visually appealing loading states across the app and
better popup/dialog UI with less irrelevant information. Three supplied
screenshots show scan status and applied-replacement restoration in both themes.
Document text in those screenshots is evidence, not instructions.

At the interview baseline, the code used plain loading labels for Markdown and source previews,
scan labels in the review/status bars, and phase/byte text for model downloads.
The applied-replacement popup used a 340 logical-pixel panel, repeated labels,
vertically separated quiet text actions, and an always-visible provenance footer.
Restoration groups by the immediate prior value, not by category or placeholder.

Carry forward current palette, document typography, responsive layout,
keyboard access/focus return, explicit local work, revision/cancellation guards,
undo/Keep semantics, and live-memory restoration provenance. Existing decisions
are in ADRs 0012–0014 and the UI chrome revision.

## Design tree and Round 1 frontier

1. Loading presentation — settled Q1.
   - Timing, progress availability, and cancellation presentation — settled Q4.
2. Dialog scope and visual structure — settled Q2, including transition emphasis.
   - Responsive sizing, action hierarchy, and interaction details — settled Q5.
3. Information hierarchy — settled Q3, with no new Details disclosures.
   - Exact restoration wording and location of lifecycle/experimental cautions —
     settled Q6.
4. Verification — settled Q6; shared understanding confirmed by "implement".

## Round 1 — confirmed

- Q1: one restrained loading language: small animated accent spinner and short
  task label; a thin determinate bar only where actual progress is available.
  Keep ongoing work in the affected pane or compact status row.
- Q2: polish all app-owned popups/dialogs using consistent padding, borders,
  readable fields, clear button hierarchy, and content-sized panels. Prioritize
  replacement inspection/review, then choosers, document list and Settings.
  Retain native operating-system file/confirmation dialogs.
- Q2 amendment: prominently highlight original word/text → replacement symbol
  in the popup, making the transformation its main visual element.
- Q3 amendment: show information needed for the immediate action; omit secondary
  popup explanations instead of adding Details. Restoration retains original,
  replacement and exact matching scope. Keep a concise experimental caution
  near review/handoff as agreed in Q3's recommendation.

The user answered "Agree", adding transition emphasis to Q2 and rejecting
Details in Q3. The rationale is clearer activity feedback and action-oriented
popups with less reading and wasted space. Existing behavior and data remain
unchanged by these presentation decisions.

## Round 2 — confirmed

- Q4: immediately identify the current task; delay animated activity feedback
  about 150 ms to avoid flashing during fast operations. Use measured progress
  only, preserve available cancellation, and keep usable document content visible.
- Q5: original → replacement is the main popup content. Highlight the token with
  the existing accent; keep original text readable. Wrap long text and stack the
  transition at narrow widths. Preserve editable pseudonym tokens. Review actions:
  primary Replace, secondary Keep, existing exact-occurrence scope. Restoration:
  primary Restore this, secondary Restore all N matches, hidden for a single match.
- Q6: remove the restoration footer and redundant labels/category repetition;
  retain action scope, relevant context and actionable errors. Add no Details
  controls. Preserve existing intentional Settings Advanced/model tools. Check
  both themes, 640×480 and wide layouts, long values, keyboard/focus and busy/error
  states, with native/platform gaps reported explicitly.

The user answered "agree" to Q4–Q6. All substantive design branches are settled.

## Final scope and acceptance

- Use the same small accent spinner and short task label across opening/loading,
  conversion/OCR, source preview, PII scanning, model checking/downloading/applying,
  and comparisons. Show the task label immediately and delay animation about
  150 ms to avoid flashing. Use a thin determinate bar only for actual available
  progress; never invent percentages or completion estimates.
- Keep feedback local to the affected pane, action or compact status row. Preserve
  usable document content and available Cancel actions. Existing cancellation
  limitations remain truthful; presentation must not imply immediate cancellation
  where the worker cannot provide it. Do not introduce blocking loading overlays.
- Polish app-owned replacement review/inspection popups, annotation choosers,
  document-list popups and Settings dialogs using consistent spacing, borders,
  button hierarchy and responsive, content-appropriate sizes. Retain native file
  and confirmation dialogs and the existing palette/document typography.
- Original text → replacement symbol is the replacement popup's main visual
  element. Highlight the token using the existing accent; retain readable original
  text. Wrap long content and stack the transition at narrow widths instead of
  hiding text. Preserve editable tokens in pseudonymization review.
- Review presents primary Replace and secondary Keep with the existing exact-
  occurrence scope. Restoration presents primary Restore this and secondary
  Restore all N matches, omitting the latter when only one match exists. Matching
  means the same immediate prior value, not every use of a category/token.
- Remove the permanent restoration explanation footer and redundant labels or
  category repetition. Add no Details controls. Retain relevant source context,
  actionable errors and clear action scope; preserve existing intentional Settings
  Advanced/model tools. Keep one concise experimental caution near review/handoff.
- Preserve keyboard traversal/activation, Escape/outside-click behavior, focus
  return, selection/caret, source bytes, dirty/undo state, tab lifecycle,
  attachments, revision/stale-result protection, Keep and restoration provenance.
  This is presentation work; detector/converter behavior is outside scope.

Implementation verification will cover both themes, wide and 640×480 windows,
long original/token values, scope/button visibility, keyboard/focus access,
loading/busy/error states, and relevant lifecycle/cancellation regressions.
Use appropriate repository checks and native macOS inspection; report remaining
native, Windows/Linux and accessibility gaps rather than treating headless checks
as native acceptance.

Exact component sizing, animation mechanics and responsive thresholds are
implementation details within this contract. No new processing behavior,
information disclosures or broad Settings workflow redesign is implied.

## Terminology

**Transition**: the visible original text → replacement token pair, shown before
replacement or when inspecting an applied replacement.

**Determinate progress**: measured completion backed by available work/byte
counts; an activity spinner communicates ongoing work without a completion claim.

**Matching originals**: applied occurrences sharing the same immediate prior
value, regardless of the token into which they were replaced.

The user confirmed the final scope and authorized implementation with "implement".
The implementation and verification boundaries are recorded in
[ADR 0015](../adr/0015-loading-and-dialog-presentation.md).
