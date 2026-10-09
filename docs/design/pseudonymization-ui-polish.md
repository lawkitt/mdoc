# Pseudonymization UI/UX polish

Status: Design interview in progress, started 2026-10-09. Continues ADRs
[0020](../adr/0020-direct-replacement-workspace.md) and
[0022](../adr/0022-remove-anonymization-mode.md); their contracts remain the
baseline unless revised here.

## Requested by the user

- Place the Replacements panel directly to the right of the Markdown editor.
- Apply a single mention or a matching group from the popup, with the applied
  state visible in both popup and panel.
- An applied mention offers undo of its replacement in the popup.
- Grouped values in the panel have no hover/selected styling.

## Observed baseline (code, 2026-10-09)

- Apply exists only as the panel-wide "Apply replacements" (one undo step).
- The popup offers "Keep this mention" / "Keep N matching values" for proposals
  and "Restore this mention" / "Restore N matching values" for applied mentions.
  Restore reverts text and records a Keep decision.
- Scope chips "This mention / Same wording · N / Entire entity · M"
  (`popup_render.rs`) set `mapping.scope`, which governs only alias and
  category changes. Keep/Restore carry their own fixed scope (ADR 0020 Q10).
- Only the identity row receives the active background; expanded mention rows
  have no hover or selected style.

## Round 1: confirmed decisions

1. **Layout** is Markdown | Replacements | Original. Width remains the adaptive
   260–340 px at full height. With Original hidden: Markdown | Replacements.
   Narrow windows keep the existing Markdown/Original pane switching; the panel
   stays beside the editor. The Original preview header gets its own close (×)
   button, equivalent to "Hide original". Supersedes ADR 0020's right-edge placement.
2. **The popup gets scoped Apply actions**, replacing the separate
   "Keep N matching values"-style buttons; scope is resolved by Q6.
3. **Two reversal actions for an applied mention**: *Undo replacement* returns
   the text to the original and the mention to **proposed** with the same alias
   (the inverse of popup Apply); a secondary action reverts and records a Keep
   decision (today's Restore).
4. **Applied state is visible**: popup shows an "Applied" badge beside the
   category; the panel group row reads `N · applied`, `N · proposed` or
   `K of N applied`; mention rows carry a small ✓/applied marker; editor keeps
   the existing ADR 0014 applied/proposed highlight distinction. *(Colour revised by
   Q9: applied values get their own background colour.)*
5. **Panel row styling**: all rows (group and mention) get hover backgrounds;
   the expanded group keeps a subtle container background; the mention row
   matching the popup's active occurrence gets a stronger selected background
   and a left accent bar. Popup ‹ › navigation moves that selection and scrolls
   it into view.

## Round 2: confirmed decisions

6. **One scope for every popup action.** The scope chips (This mention / Same
   wording · N / Entire entity · M, the last only when broader) govern alias,
   category, Apply, Keep and Undo alike. Each action shows its count for the
   selected scope. Same wording stays the default. Proposed mention: `Apply · N`
   and `Keep · N`. Applied mention: `Undo · N` (back to proposed) and a Keep
   action that reverts and records Keep. This revises ADR 0020 Q10's
   independent Keep/Restore scope; the visible count guards against broad
   accidental actions.
7. **Keep stays the term** in UI, glossary and code; no Dismiss/Skip rename.
8. **Short-lived recovery after Keep**: the panel footer briefly shows
   "Kept N · Undo", which undoes that Keep step. A dedicated Kept list/filter is
   out of scope.
9. **Apply in the popup keeps the popup on the same mention**, now showing the
   Applied badge, so Undo is immediately reachable. Moving to other mentions
   stays explicit. **Applied values get a distinct background colour**, not only
   the lower opacity of the proposal colour used today (`src/pii/ui.rs`
   annotations: proposal and applied both use `alert_warning`, alpha 0.14 vs 0.10).

## Round 3: confirmed decisions

10. **Applied colour is teal** (`search_accent`, #82b9a7 dark / #336b5c light),
    already used for aliases in the panel. Proposed stays amber
    (`alert_warning`). Teal is used for applied editor highlights (with a stronger
    active variant), the panel ✓ marker and the popup "Applied" badge. Contrast
    is checked in both themes.
11. **Keep label is `Keep original · N`** for both proposed and applied
    mentions. **Undo is an icon button**, not a text button (placement and
    tooltip in round 4).
12. **A scope with both states**: Apply and Undo follow the active mention's
    state and count only matching-state mentions in scope. `Keep original · N`
    covers both states. The chip count still shows the whole scope.
13. **"Kept N · Undo" stays until the next review action or text edit**, with
    no timer. It uses the ordinary undo step, so Cmd+Z behaves the same.

## Round 4: confirmed decisions

14. **Popup Undo icon** (↶, new `resources/ui/undo.svg` in the existing
    hand-drawn style) sits in the action row before `Keep original · N`, with a
    32 px hit area. Tooltip and accessibility label: "Undo replacement · N (back
    to proposed)". The header shows only state (category, Applied badge, navigation).
15. **Panel applied mention rows** show ↶ on hover or when selected. It undoes
    only that mention. Group rows get no undo; the popup's Entire entity scope
    covers that case.
16. **Footer undo controls use the icon too**: `N applied mentions ↶` after a
    full Apply and `Kept N ↶` after Keep, each with a descriptive tooltip.
17. **Record as ADR 0023** with a CONTEXT.md update.

## Consolidated contract

- Layout: Markdown | Replacements | Original; Original has its own × close.
- Popup: one scope chip row governs every action. Proposed: `Apply · N`,
  `Keep original · N`. Applied: ↶ Undo (back to proposed), `Keep original · N`.
  Apply/Undo count only mentions in scope that share the active mention's state.
  Each action is one undo step; the popup stays on the active mention.
- Applied state: teal editor highlight, teal "Applied" popup badge, teal ✓ on
  panel mention rows, group status `N · applied` / `N · proposed` / `K of N applied`.
  Proposed stays amber.
- Panel: hover on all rows; subtle background on the expanded group; a stronger
  selected background plus left accent bar on the active mention row, following
  popup ‹ › navigation and scrolled into view; ↶ on applied mention rows.
- Recovery: `Kept N ↶` in the footer until the next review action or text edit.

The user confirmed the empty frontier on 2026-10-09 and requested implementation.

## Implementation notes (2026-10-09)

- Model (`mdoc-pii`): `Review::reversion_edits` reverts an exact set of applied
  occurrences; `commit_unapply` records the reversion without Keep and pins a
  separated homonym to its identity through an occurrence assignment;
  `keep_mentions` keeps a set of pending mentions with one refresh.
  `Tracking::commit_restore` takes a `keep` flag.
- Popup (`popup.rs`, `popup_render.rs`): `scoped_mentions` splits the selected
  scope into proposals and applied occurrences. `apply_scope`, `undo_replacements`
  and `keep_originals` replace the fixed-scope Keep/Restore handlers and the
  unbound `PiiRestore`/`PiiRestoreAll` actions. A visible alias draft is confirmed
  together with a scoped Apply (`Applying::Ranges`), still one undo step. Keep of
  mixed states reverts applied text and keeps proposals in the same history step.
- Panel (`panel.rs`): rows track applied counts; the expanded group uses a
  half-strength `placeholder_bg` so control hover stays visible; the active
  mention uses `sidebar_selected` plus a 3 px teal bar (gpui allows one hover
  style per element, so hover replaces the selected fill and the bar remains).
  The ↶ on applied mention rows appears on hover (tracked in `MappingUi::hovered`)
  or when current. `reveal_active_row` scrolls the list after navigation.
  "Kept N ↶" is shown while the Keep's history state is current.
- Layout (`main.rs`): the panel follows the Markdown column; split dragging
  subtracts the panel width. `gpui-pdf` gained `PdfView::set_on_close` (✕ in the
  source-name row); the host hides Original through `toggle_preview`.
- Editor highlights: applied uses `search_accent` at 0.16 / 0.34 active.
- Verification: `cargo test --workspace`, `cargo clippy --workspace --all-targets`
  and `cargo fmt --check` pass. New tests cover scoped Apply/Undo/Keep as single
  undo steps with the popup retained, separated-homonym Undo, the panel row ↶
  click in both themes, and panel-before-Original placement. Live visual QA in
  the running app was not performed in this session (the unbundled debug binary
  could not be granted to screen control); light/dark contrast is unverified by eye.
