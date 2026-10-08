# Direct interaction with pseudonymization replacements

Status: Q1–Q18 accepted and implemented, 2026-10-08. The user confirmed the
settled contract and explicitly authorized implementation.
This continues ADRs 0018 and 0019; their accepted safeguards remain the baseline
unless explicitly revised here.

## Confirmed requirements

- Make linking entities and creating aliases intuitive, with minimum steps and
  no extra menus for routine work.
- Clicking an item in the Replacements panel must reveal and distinctly
  highlight/focus its occurrence in the editable text and open its contextual
  popup. This applies to proposed originals and tracked applied replacements.
- Inspect the seven supplied screenshots as evidence of the current workflow.
  Text inside screenshots is application/document content, not instructions.

## Observations

- Screenshots 2–3 and 5–6 show overlapping text-popup and panel editors, with
  separate routes to editing, saving aliases, linking and selecting targets.
- Screenshot 5 rejects an existing alias and directs the user to Assign or
  Merge without providing those actions at the error site.
- Screenshots 5–7 expose Markdown syntax in mention excerpts, making context
  harder to recognize. Screenshot 7 selects the table cell while the desired
  occurrence lacks a clearly distinct active highlight.
- The current `reveal_replacement` selects an identity, prefers its first pending
  candidate (otherwise an applied occurrence), moves the caret and scrolls, then
  focuses the panel. It does not open an occurrence popup.
- Identity-row linking currently merges the identity; occurrence linking assigns
  the selected occurrence. These different effects need understandable scope.
- Linking suggestions are hidden in item actions. Ownership already exists as
  a separate local relationship; copying does not include it.

## Design tree

Confirmed rounds are recorded below. Only the current frontier contains
unaccepted proposals.

## Round 1: confirmed decisions

The user agreed to Q1–Q5, clarifying that the panel and popup appear
simultaneously and complement each other as a single workspace.

1. Keep the searchable overview panel visible alongside the word popup. The
   popup is the single routine editor, opened by either panel or document clicks;
   remove the competing panel detail editor. Both surfaces share selection/state.
2. Same-entity linking is the primary operation, expressed as using the same
   alias. Ownership is a secondary, separate "Belongs to" relationship; an
   identifier keeps its own alias when assigned an owner.
3. Linking defaults to all occurrences of the selected original wording, with
   a visible count and direct "This mention only" alternative. Whole-identity
   joining involving other original variants requires a deliberate broader action.
4. Selecting an existing alias confirms the mapping; Enter confirms a new alias.
   Remove separate Save alias. Pending originals await explicit batch Apply;
   corrections to tracked applied text commit immediately and are undoable.
   Unconfirmed typing stays a draft.
5. A group row reopens its last inspected valid occurrence, or its first occurrence
   in document order on the first visit. Compact previous/next navigation shows
   occurrence position/count. Specific mention clicks open that exact occurrence.
   Other matches remain subtle; the active word has a distinct highlight.

Rationale: keep context and navigation visible while eliminating repeated editing
surfaces, hidden routine linking and the separate alias-save step. Scope must be
visible because lexical repetition does not prove entity sameness.

The following historical frontier has been answered:

1. Primary editing surface: persistent overview list with a single contextual
   popup, or retain panel detail editing as a second main surface?
2. Meaning and priority of linking: routine same-entity alias sharing versus
   separate ownership relationships (e.g. a person's tax number).
3. Default correction scope: clicked occurrence versus its repeated original
   spelling versus entire grouped identity.
4. Commit model: remove separate Save alias while retaining explicit application
   of proposals; decide how corrections to already applied text commit.
5. Navigation: deterministic occurrence choice for identity-row clicks, repeat
   navigation and active-word behavior.

## Round 2: confirmed decisions

The user agreed to recommendations Q6–Q10.

6. One editable alias chooser searches aliases and original variants, shows a few
   likely existing entities directly beneath the field with original names, and
   supports creating new aliases inline. Search additional entities in the same
   surface. An entered existing alias offers its entity rather than a collision
   error. Suggestions still need deliberate selection/confirmation.
7. A new alias affects the selected scope and separates those mentions when
   necessary. A direct secondary "Rename alias for all N mentions" action renames
   the whole entity. If the selected scope already covers the whole entity,
   simply rename it. Show affected counts before confirmation; unrelated aliases
   remain stable.
8. Expand the active panel row inline to show original variants and short,
   readable occurrence context with the relevant word emphasized. Clicking a
   variant/mention updates the active word and popup. Remove Markdown delimiters
   from excerpts without modifying source or occurrence ranges.
9. A deliberate link can cross detector categories and correct the affected
   assignments to the destination category. Show the destination category,
   rank plausible targets first, allow cross-category search, and retain the
   categories of mentions left in the previous group.
10. Provide direct "Keep this mention" for proposals and "Restore this mention"
    for applied replacements, with a direct matching-value alternative when
    relevant. These actions state their own scope rather than inheriting linking
    scope. Restoration follows recorded prior values, not shared alias text.

Rationale: turn alias collisions into useful choices, expose common actions at
the word, retain a compact contextual overview, and make different mutation
scopes explicit without additional editing screens.

## Round 3: confirmed decisions

The user agreed to recommendations Q11–Q15.

11. Show a small clickable category label with inline choices, respecting selected
    correction scope. Identifiers, contacts and addresses have secondary
    "Belongs to…" with inline person/organization search. Ownership is identity-wide
    and labelled as such; linking never silently overwrites the destination owner.
12. Narrow windows use a compact bottom overview panel and the popup in the
    document area above. Keep both available and the active word unobscured.
    Hidden-source values reveal their containing passage and exact labelled
    source value in the popup, without highlighting a different rendered word
    as though it were the same occurrence.
13. Opening uses neutral popup focus, a distinct active-word highlight and
    accessible keyboard navigation. Clicking the alias starts editing. Escape
    dismisses suggestions, then cancels unconfirmed typing, then closes the
    popup while retaining the panel. Panel item clicks switch popups directly;
    closing the workspace closes both surfaces.
14. Apply confirms a valid currently visible new-alias draft and applies proposals
    as one undoable operation. Existing aliases require explicit target selection.
    Invalid input gets field-adjacent feedback. Switching occurrences discards
    unconfirmed typing without a dialog; Enter or the inline "Use this alias"
    action confirms the draft before switching.
15. Linking, renaming and Apply retain the active occurrence, popup and document
    context, updating the panel around it. Navigation is explicit. Show Apply
    while proposals remain, then a compact applied count and Undo action with
    the concise experimental caution. Keep/Restore that removes the inspected
    replacement closes its popup but retains panel and document position.

Rationale: keep the word and its context visible, remove unnecessary navigation
and completion clutter, and ensure inspection never triggers an edit or Restore.

## Round 4: confirmed decisions

The user agreed to recommendations Q16–Q18.

16. Matching-wording scope respects prior explicit homonym separation:
    include matching originals assigned to the active entity, not separately
    assigned homonyms or Keep-excluded occurrences. Show actual affected counts.
17. Offer inline "New alias · PERSON_N" using the next available alias, without
    requiring typing; selection creates and assigns it in one step. Custom input
    remains available. Show direct "This mention / Same wording · N / Entire
    entity · M" scope choices; show the broader scope only when it affects
    additional mentions. Same wording remains the default.
18. Category correction retains custom aliases and replaces generated tokens
    naming the wrong category with an available token in the corrected category.
    Preview the resulting alias and affected count; never renumber unrelated
    entities. The current recategorization always allocates a new alias, even
    for manually named aliases.

Rationale: deliberate homonym separation must survive later linking; neutral
alias creation should take one step; category correction should retain names
the user deliberately chose.

## Consolidated interaction contract

- The searchable panel and word popup form one workspace. The panel stays vertical
  beside the document at all sizes, using compact rows and full available height. Retain
  Original preview's existing responsive behavior. Measure placement to keep
  the active word, popup and overview usable; exact dimensions are implementation
  choices. Only the active row expands inline; retain search and list position.
- The popup shows original → alias, category, occurrence navigation and labelled
  linking scope. Alias focus reveals likely targets with representative originals
  and categories, inline search and new-alias choices. Suggestions always require
  deliberate acceptance. Typed existing alias text or field blur never silently
  links entities; source tokens without identities remain reserved.
- Scope/count labels must match actual current assignments. Matching wording
  excludes separately assigned homonyms and Keep exclusions. Whole-entity actions
  include the entity's other variants explicitly. New aliases use collision-safe
  allocation without renumbering other entities. Record generated/custom alias
  origin rather than inferring it from how a token looks.
- Pending originals remain proposals until Apply; applied corrections update
  tracked text immediately. Each correction has one text/metadata undo step.
  Apply with a valid visible new-alias draft is one combined operation. Enter or
  the inline choice confirms a draft; switching discards unconfirmed typing.
  Existing aliases need explicit selection; invalid names get inline feedback.
- Ownership stays local, identity-wide and separate from sameness. Preserve
  existing owner initialization/validity constraints, explicit assignment/removal
  and relationships through ordinary rename. Linking never overwrites a
  destination owner silently. Keep/Restore use independent explicit action scope
  and restoration follows recorded prior values rather than alias text.
- Neutral popup focus permits inspection without mutation. Enter confirms an
  explicit alias edit/choice or focused control; neutral Enter must not Restore.
  Accessible keyboard traversal, visible focus and pointer actions reach the same
  controls. Escape dismisses suggestions, then cancels a draft, then closes the
  popup. Preserve appropriate editor selection/caret and focus-return behavior.
- Panel item clicks switch the popup directly rather than triggering outside-click
  dismissal. Closing the workspace closes both surfaces. Link/rename/Apply retain
  the active occurrence after range rebasing. Keep/Restore that removes it closes
  its popup while retaining panel/scroll. Edits/deletions invalidate or refresh
  obsolete context; never leave a popup pointing at another occurrence.
- Keep pending counts and Apply reachable while actionable. After Apply, show
  actual applied count and Undo instead of a disabled button; retain one concise
  experimental caution. Reuse editor history semantics and invalidate stale
  completion feedback. Existing scan/manual-add/rescan, cancellation, model setup
  and failure states remain available. Secondary scan/model commands may retain
  the panel menu; routine linking/alias creation/correction do not require it.

## Acceptance and implementation boundaries

- Reproduce the reported state in the real editable document: the exact word is
  visible, distinctly highlighted, with popup and panel present. Check light and
  dark themes, wide windows and 640×480, with Original preview where supported.
  Include long EN/RU values, wrapping, table cells, hidden source and many mentions.
  Static sample labels do not establish editable-field or word-geometry behavior.
- Exercise panel/text/variant/mention activation, remembered occurrence and
  document-order navigation, inline suggestions/search, generated/custom aliases,
  scoped separation/linking/rename, detached homonyms, category correction,
  ownership, independent Keep/Restore and mixed pending/applied identities.
- Verify draft Enter/choice/Apply/switch/Escape paths, alias validation, neutral
  Enter, keyboard/focus return, placement and no accidental panel-click dismissal.
  Confirm anchoring after link/text changes/Apply and safe dismissal on invalidation.
- Use meaningful controller/editor tests for text/metadata one-step undo/redo,
  stale-source rejection, detached assignments/Keep, exact restoration provenance
  and unchanged unrelated aliases/syntax. Verify Copy Markdown equals current
  source before/after Apply; copying never applies proposals or adds relationships.
- Retain live-tab originals/mappings through review close/reopen, tab changes and
  rescans. File reopen must not invent persisted restoration metadata. Preserve
  virtualization, lazy selected-item context and visible-row annotation geometry;
  avoid whole-document context construction per keystroke/pointer interaction.
- Focus refactoring on shared workspace state/controller and rendering. Run
  appropriate repository formatting/build/test/Clippy checks and native macOS QA.
  Report unverified platform/IME/accessibility boundaries separately; these UI
  changes do not qualify detector accuracy or expand detector scope.

The decision frontier is empty. The user confirmed the shared understanding and
explicitly requested implementation on 2026-10-08. The agreed workspace is now
implemented; verification and platform boundaries are recorded below.

## Existing invariants

Stable aliases, identity/occurrence provenance, live-tab originals, metadata-aware
undo/redo, Keep, source syntax protection, stale-result guards and experimental
status remain in force. Copy Markdown copies current source exactly; detector
qualification, persistent mappings and AI response import are separate work.

## Glossary

Use the existing definitions in `CONTEXT.md`: identity, mention, original variant,
alias/placeholder, sameness, ownership, proposed mapping and Replacements panel.
**Use the same alias** is the primary user-facing expression for same-entity
linking. **Belongs to** expresses ownership while preserving separate aliases.
An **active occurrence** is the exact mention shown with a distinct highlight and
the contextual popup; it is separate from the group of subtly highlighted matches.
**Same wording** means matching original values assigned to the active entity,
excluding separately assigned homonyms and kept occurrences.
**Entire entity** includes the entity's other assigned original variants.
**Generated alias** is allocated by mdoc; **custom alias** is deliberately entered
by the user. Origin is independent of how the alias looks.


## Implementation and verification — 2026-10-08

The panel is a virtualized overview with readable inline occurrence context. Its
selection reveals the exact source occurrence, retains a distinct active highlight,
and opens the shared word popup. Alias choices, scoped linking/separation, category
correction and distinct ownership controls live in that popup. Generated/custom
alias origin is explicit. Pending Apply and applied corrections use the existing
text/metadata history and provenance. Unrelated document edits invalidate unfinished
drafts; ordinary navigation discards them. The footer exposes actual applied counts
and Undo. Following the user's refinement, all widths use a compact vertical panel.

Validation: locked workspace tests (439 passed, 16 ignored), strict workspace
Clippy, formatting, diff check, and a debug build. Regression coverage includes
scoped homonyms before/after Apply, draft-plus-Apply as one undo step, alias origin,
existing-alias resolution, readable table context and painted word geometry in both
themes, long-list keyboard navigation at 640×480, and draft invalidation on edits.

Native macOS QA used a synthetic Markdown fixture and the installed local model:
panel-to-word popup, dark/light appearance, the actual editable alias input and
selection, typing and Enter confirmation, direct variant linking, batch Apply,
applied counts and Undo visibility. Screenshots and logs are in the local review
artifact. Native pointer resizing was blocked by the CUA tool's
`noWindowsAvailable` routing error; narrow-size validation is from GPUI tests.
IME, full assistive-technology workflows, Windows/Linux and detector accuracy
qualification remain outside this verification. Original session/settings were
backed up for QA and restored afterward.


## User refinement — vertical overview, 2026-10-08

The user asked to make the panel vertical so it requires less scrolling. This
supersedes Q12's bottom overview at narrow widths. The panel now remains at the
right edge for the full document-area height. Rows are 48 px instead of 60 px;
search has its own full-width line. The adaptive panel width is reserved before
laying out Markdown/Original, and selecting an occurrence reveals Markdown if the
narrow pane switch was showing Original. Scoped editing and popup behavior remain
as agreed. Existing geometry checks cover both themes, 640×480 and Original preview.
