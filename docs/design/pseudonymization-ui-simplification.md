# Pseudonymization UI simplification — 2026-10-08

Status: implemented and locally verified, 2026-10-08, following the user's
"implement". See the [verification record](../../tests/fixtures/pseudonymization/results/2026-10-08-ui/README.md).

## Purpose and evidence

The user requests a substantially simpler, more elegant pseudonymization UI,
using the supplied screenshot as the starting point. Inspected the current
review bar and mode controls in `src/pseudonymization_ui/render.rs`, identity
panel and correction actions in `src/pseudonymization_ui/mapping.rs`, ADRs
0012/0014/0018, and the existing loading/dialog design agreement.

The screenshot presents parallel review and identity controls, repeated pending
counts and experimental cautions, two copy actions, and identity maintenance
controls alongside the main replacement task. The selected identity's details
expand inside the scrolling identity list. The review bar still exposes Details,
although the previous dialog agreement called for concise content without extra
Details controls. These are observed presentation issues, not new decisions.

Existing contracts include explicit proposed-map review before alias application,
stable document-local aliases, optional manually confirmed ownership, explicit
occurrence provenance, Keep, metadata-aware undo/redo, revision guards, and
live-tab-only originals. Copy for AI requires resolved detected pending mentions
and can include an alias-only relationship legend; Copy Markdown copies current
source. Detection remains experimental. This interview may explicitly amend
workflow/presentation choices; it does not qualify detection.

## Design tree and current frontier

Root decisions from Round 1:

1. Q1 — confirmed: compact review before applying, with simple original-to-alias
   rows and one prominent batch action.
   - Review unit, ordering, navigation and document/list synchronization.
   - Row/popup contents, action scope and application feedback.
   - Keep/manual additions and handling unresolved mentions.
2. Q2 — confirmed: retain identity correction tools with contextual
   disclosure. Retain rename,
   category correction, link/separate and owner assignment, exposing them only
   when relevant to a selected item.
   - Main workspace, selected-item editing, search and terminology.
   - Variant/occurrence assignment, merge/split scope and owner selection.
   - Discoverability and correction of applied replacements.
3. Q3 — confirmed: converge on one copy action for now; future revision is allowed.
   Q6 selects current Markdown exactly. Pending-review availability remains open.
   - Action placement, relationship-legend choice and pending/stale states.
   - Completion wording and one concise experimental caution.

Cross-cutting branches after the root decisions:

- Entry/exit and coexistence with Anonymize, Original and conversion notices.
- Wide/narrow layout, themes, long values and large lists.
- Keyboard traversal, focus return and accessible names.
- Busy, cancelled, setup, empty, failed and edited-after-scan states.
- Verification criteria and final shared-understanding confirmation.

Questions downstream of unsettled prerequisites wait for later rounds. Exact
spacing and implementation mechanics can follow an agreed visual contract.

## Confirmed decisions

Round 1: the user answered "Q1-Q2 agree. Q3 keep one copy action for now, maybe
we will change it later".

- Preserve review before applying and make the proposal list compact, with one
  prominent apply action. Rationale: retain visibility of incorrect mappings
  while reducing effort and competing controls.
- Preserve correction capabilities, disclosing them contextually for the selected
  item. Rationale: simplify the routine view without losing identity corrections.
- Present one copy action for now. This rejects the proposed two-action design;
  it does not yet choose raw text versus a relationship appendix or a copy gate.

Durable direction is recorded in [ADR 0019](../adr/0019-simpler-pseudonymization-ui.md).

## Round 2 — confirmed

The user answered "Q4 agree, and make panel UI simple and elegant, Q5-Q6 agree".

- Q4: One compact Replacements panel combines the review bar and identity panel,
  opens after scanning, keeps the document visible and contains review/application
  controls. Closing it preserves clickable applied highlights. Make it simple
  and elegant; remove the parallel review bar.
- Q5: One original-to-alias row per identity, with mention count. Selecting it
  reveals variants/occurrences and points to the document. Clicking a highlight
  selects the corresponding item.
- Q6: Copy Markdown copies current source exactly. Remove the second copy action
  and relationship-legend option for now; owner assignments remain local.

Rationale: give the task one workspace, avoid repeated-name clutter and make
clipboard contents predictable.

## Round 3 — confirmed

The user answered "agree" to Q7–Q10.

- Q7: Application and Keep scope. Apply replacements applies
  all remaining proposals as one undo step without mandatory per-row approvals;
  Keep on an identity row excludes its pending mentions, with single-mention
  Keep available during occurrence inspection.
- Q8: Selected-item presentation. Open a compact detail view
  inside the same panel, with Back preserving list position; show the transition,
  alias editing and mention navigation, with identity corrections under an
  item-specific action menu. Applied document popups retain scoped Restore and
  a route to this detail view.
- Q9: Copy availability. Ordinary Copy Markdown stays available
  with pending proposals, copies current text without applying anything, and
  does not claim completion. Show pending status and one concise review caution.
- Q10: Narrow layout. Dock beside the document when both fit;
  use a dismissible overlay drawer at narrow widths, preserving selection and
  scroll on return to the document. Keep application controls reachable.

Rationale: avoid repetitive approval work, isolate detailed corrections from the
list, preserve predictable raw copying and protect document readability.

## Confirmed final scope

### Main workflow and panel

- Preserve Scan → review proposed aliases → Apply replacements. Pseudonymization
  scanning alone does not edit text. Open the Replacements panel after scanning.
- Combine the pseudonymization review bar and Identities panel into this one
  workspace. Keep a compact reopen control when dismissed. Close hides the
  workspace without discarding accepted edits or applied-highlight provenance.
- Show one compact original → alias row per identity, with a mention count and
  clear pending/applied distinction. Search finds aliases and original variants.
  Preserve stable alias numbering; sorting/filtering does not change identity.
- Keep the list uncluttered: no permanent rename field, category selector,
  ownership editor, variants list, occurrence list or repeated explanatory
  footer in each row. Use the existing restrained palette, typography and accent;
  wrap long values and retain access to full originals.
- Use one prominent Apply replacements action for all remaining proposals,
  as one undo step. No mandatory per-row approval checkboxes. Keep original at
  identity scope excludes that identity's pending mentions, including assigned
  variants; single-mention Keep remains available during occurrence inspection.
  Applied values use scoped Restore rather than an ambiguous Keep action.

### Item inspection and corrections

- Selecting a row opens a compact item view within the same panel. Back restores
  search/list scroll position. Center original → editable alias and mention
  navigation; show variants/occurrences only in this selected-item context.
- Connect row/mention selection to the document and document highlights to the
  corresponding item. Keep contextual occurrence inspection available without
  requiring users to enumerate every repeated mention.
- Retain rename, category correction, identity linking/separation, occurrence
  assignment and owner assignment through item-specific actions. Distinguish
  identity-wide operations from variant/occurrence operations explicitly.
- Preserve existing correction behavior for applied tracked occurrences, alias
  collision checks, Keep and metadata-aware undo/redo. No token-string inference
  of identity or restoration provenance.
- Applied document highlights retain compact original → replacement popups with
  Restore this, Restore all matching originals where applicable, and a route to
  replacement editing. Restoration scope remains the same immediate prior value,
  not every occurrence sharing an alias/category token.

### Handoff, warnings and states

- Keep the existing main-toolbar Copy Markdown action. Remove Copy for AI and
  the relationship-legend option. Copy current full Markdown exactly; do not
  append relationship policy or apply pending proposals as a copying side effect.
  Owner assignments remain local. Future copy changes are a separate decision.
- Pending proposals do not block copying. Preserve existing source-availability
  guards. Keep one concise experimental caution near review: Experimental ·
  Review for missed identifiers before sharing. Show pending status without
  duplicate counters or completion claims. Do not call the document safe to share.
- Remove redundant review Details/Less and repeated warning text. Preserve
  intentional Settings/model-management tools and actionable errors.
- Preserve existing scan/rescan, cancellation, manual selection addition and
  model-setup access, with secondary commands in the panel's action menu. Use
  the existing local activity treatment; do not invent progress percentages.
- Distinguish scanning, cancellation, setup needed, failure, no proposals and
  applied states with concise relevant text/actions. Empty detection results do
  not establish absence of identifying information. Keep stale-result/revision
  guards and accurate rescanning behavior after edits.
- Preserve the default Anonymize flow, Original preview, conversion notices,
  source documents and other editor behavior. This change focuses on the
  pseudonymization workspace, not detector changes or qualification.

### Responsive and interaction contract

- Dock beside the document where both remain readable; use a dismissible drawer
  over the document at narrow widths. Keep Apply reachable and preserve source
  selection/caret/scroll. Exact dimensions follow measured fit, not new caps.
- Preserve keyboard navigation/activation, accessible names and visible focus,
  Escape/dismissal and focus return. Retain virtualization and build detailed
  occurrence context only for the selected item; avoid whole-document UI work.
- Preserve current text, dirty/undo state, live-tab originals, attachments,
  confirmed mappings, Keep exclusions and source-syntax protection. Closing or
  reopening files continues to follow the existing live-memory mapping contract.

### Acceptance evidence

- Verify the actual interactive review/item/popup states in light and dark themes,
  at a wide viewport and 640×480, including long EN/RU values and many mentions.
- Exercise search, Back/list restoration, both selection directions, batch Apply,
  identity/mention Keep, contextual identity corrections and scoped restoration.
- Check one-step undo/redo, edits/stale results, close/reopen review, rescans and
  copy before/after applying. Clipboard output must equal current Markdown exactly.
- Verify busy/cancelled/setup/error/empty states and keyboard/focus behavior with
  appropriate existing repository checks and native macOS interaction. Report
  unverified IME/accessibility and Windows/Linux boundaries explicitly.
- Exact metrics and reusable UI/component refactoring are implementation choices
  within this contract. Keep refactoring focused and preserve existing performance
  safeguards. Do not add detector scope or unrelated UI work.

The decision frontier is empty. The user's "implement" confirms the combined
scope and authorizes implementation.

## Terminology

Use existing `CONTEXT.md` terms: identity, alias/placeholder, mention, review
candidate, Keep and replacement mapping. **Replacements panel** is the agreed
UI name for the combined pseudonymization review workspace; identity remains
the underlying grouping concept, not its primary user-facing title.

## Delivered implementation

The combined panel uses a virtualized original-to-alias list with mention counts,
search and an item view with Back. Alias edits are staged with Save alias; an
unsaved item alias disables Apply. Contextual actions retain category correction,
linking, splitting, variants and local owners. The fixed footer contains activity,
pending/result feedback, Apply and one experimental caution. Narrow layouts use
a dismissible drawer; available document width also accounts for Original preview.

Scan, manual additions, cancellation, model setup and settings remain available.
Normal Apply retains candidate-popup drafts. Converting tracked shared markers
to aliases uses recorded occurrence provenance and leaves pasted lookalikes alone.
Keep at identity scope batches only currently assigned pending occurrences,
preserving detached homonyms and metadata undo/redo. Applied originals are reused
through shared provenance strings. Restoration retains its existing exact scope.

The renderer is isolated in `src/pseudonymization_ui/mapping/render.rs`, keeping
mapping/history behavior in its controller. The old pseudonymization review bar,
Copy for AI, relationship appendix control and unused legend builder are removed.
The existing automatic Anonymize bar and default behavior remain intact.
