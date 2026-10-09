# Pseudonymization review refinements

Design interview started 2026-10-09. Status: confirmed and implemented 2026-10-09. ADR: [0025](../adr/0025-review-refinements.md).

## Scope

1. Manual additions default to Person (the popup screenshot shows
   "Планируемая дата" → PERSON_8); whether to widen the default categories.
2. A direct way to cancel a mistaken manual addition besides ⌘Z.
3. Panel: collapse/expand groups by click, scope highlighting of a group's
   mentions in the editor (panel selection and popup scope chips), and
   drag-and-drop of mentions between groups.

## Facts (current code)

- Categories are fixed: Person, Organization, Email, Phone, Address, Identity,
  Tax, Bank (`mdoc_pii::Category`); tokens PERSON_n, ORG_n, … The popup's
  category picker re-categorizes and replaces generated wrong-category tokens.
- `detector::guess_category` falls back to Person when no rule, phone shape or
  legal form matches (ADR 0024 decision 8).
- Popup scope chips: This mention / Same wording · N / Entire entity · N
  (`popup.rs` `Scope`). "Keep original · N" reverts and records a Keep.
- The panel is an accordion: the selected entity's mentions are listed under
  it (`replacement_entries`); there is no explicit collapse.
- The editor highlights all proposals (amber) and applied (teal) and one
  active annotation; there is no group/scope highlight.
- gpui drag-and-drop (`on_drag`/`on_drop`) is already used in `src/tabs.rs`.
- Linking a mention to another entity exists through the popup alias chooser
  (assignment of an occurrence; merge of entities), one undo step each.

## Decisions

Round 1, confirmed 2026-10-09:

1. **Guess fallback**: Person only for name-shaped text; everything else is a
   new neutral **Other** category, token `REDACTED_n`. Still one click; chips
   correct it.
2. **New categories**: Other and **Date** (`DATE_n`), for manual additions only;
   the detection model's category list is unchanged. Amount/contract numbers
   wait for demand.
3. **Cancel a mistaken addition**: a ↶ "Cancel addition" icon in the popup
   header while the addition is the latest history step (exactly that undo),
   plus an "Added · ↶" footer notice in the panel, cleared by the next review
   action or text edit (as "Kept N ↶").
4. **Collapse**: click toggles a group row; ▸/▾ chevron; one group open at a
   time (accordion).
5. **Scope highlight style**: a thin outline in the state colour (amber/teal)
   around in-scope mentions, over the existing fill; the active mention keeps
   its stronger fill.
6. **Highlight trigger**: the selected scope chip sets the outline; hovering a
   chip previews its scope; a selected panel group outlines its whole entity;
   cleared when the popup closes and the group is deselected.
7. **Drag sources/targets**: mention rows and group rows. Mention → group
   assigns that mention; group → group merges; mention → empty space splits a
   new entity. One undo step each; cross-category adopts the target category.
8. **Drag feedback**: whole row draggable after a small threshold; ghost chip
   with the original; target row gets an accent outline and "Link to ORG_1"
   (empty space: "New entity"); dropping on the source is a no-op; no
   confirmation.

Round 2, confirmed 2026-10-09:

9. **Name-shaped**: 1–4 words each starting with a capital (hyphenated
   surnames and initials like "М.С."/"J.R." count) → Person; otherwise Other. A
   lone capitalized word guesses Person. The user added: the suggested alias
   needs an elegant focus cue so it reads as clickable/changeable (round 3).
10. **Date shapes**: `12.03.2026`, `12/03/2026`, `2026-03-12`; RU/EN month-name
    dates; blank-fill templates `«__» ____ 2026 г.`, `«12» марта 2026`
    (including escaped Markdown). Year-only/month-only stay Other.
11. **Cancel after Enter**: ↶ Cancel addition undoes both the apply and the
    addition while those are the latest two history steps.
12. **Drag moves one mention**; same-wording moves stay in the popup.
13. **No scrollbar marks** for off-screen in-scope mentions this round.
14. **Picker order**: Person, Organization, Email, Phone, Address, Date,
    Identity, Tax identifier, Bank details, Other. Tokens `DATE_n`,
    `REDACTED_n`.

Round 3, confirmed 2026-10-09:

15. **Cue targets**: the alias field, then (~150 ms later) the category chip
    when the guess was a fallback (Person/Other); only the alias after a
    confident guess (rule, phone, Date, legal form).
16. **Cue style**: one soft accent glow behind the field, fading in/out over
    ~700 ms with a briefly thicker underline; reduced motion shows a static
    glow for the same time. On hover the alias field always shows a pencil icon
    and an I-beam.
17. **Cue timing**: every popup opened by a manual addition, plus the first
    popup opened on a detected mention in each tab.
18. **Alias click**: selects the whole token and opens suggestions; typing
    replaces it, ↑/↓ picks an existing entity, Enter confirms (ADR 0020). In a
    manual-addition popup, Enter applies only while the alias field is not
    focused.

## Invariants and defaults (not separately asked)

- Other and Date are ordinary categories for aliases, category correction,
  token reservation and Copy for AI; the detector never emits them.
- Collapsing the group that holds the popup's mention closes the popup;
  the document position is kept.
- Every drop, merge, split, addition and cancel is one undo step; the popup
  alias chooser remains the keyboard alternative to drag-and-drop.
- Scope outlines are painted by the editor as a separate annotation layer and
  never change text, history or the active annotation.
- Acceptance: model tests (name/Date/Other guessing, new tokens, drop
  semantics), GPUI tests (collapse, scope outlines, chip hover preview,
  cancel addition incl. after Enter, drag link/merge/split, cue once per tab),
  and live QA in both themes, including reduced motion.

## Implementation notes (2026-10-09)

- **Model**: `Category::Date` (`DATE`) and `Category::Other` (`REDACTED`);
  `Category::ALL` is the picker order. Discovery reserves the new tokens like
  the others.
- **Guess** (`detector::guess_category`): structured rule → `date_shaped`
  (numeric, RU/EN month names, blank-fill incl. `\_`) → phone shape → legal
  form → `name_shaped` (Person) → Other.
- **Cancel addition**: `MappingUi::{record_added, record_apply, added_at}`;
  `apply_scope` records an Apply right after the addition. `cancel_addition`
  closes the popup and dispatches one or two editor Undos. Shown as the popup
  header ↶ (when the active original is the addition) and the panel footer
  `Added “…” ↶`.
- **Alias cue**: `MappingUi::request_cue(manual, chip)`; the popup wraps the
  alias field (and the category chip, delayed 150 ms) in a one-shot 850 ms
  gpui animation keyed by a cue generation. Hover shows ✎ and an I-beam; a
  click selects the token and opens suggestions.
- **Panel**: ▸/▾ chevrons; clicking the open group calls `collapse_replacement`.
  Rows are drag sources (`PanelDrag::{Mention, Entity}`, `DragGhost`) and drop
  targets (`drag_over` accent border, `on_drag_move` → "Link to ALIAS");
  a dashed "New entity" zone appears while dragging a mention of a
  multi-mention entity. Drops call `MappingAction::Scoped`/`Merge`, and a drop
  never opens the popup (`after_drop`).
- **Scope outlines**: `EditorState::set_outlines` paints display-only 1 px
  rounded outlines over annotation fills (`range_quads`, so tables and wraps
  work). `Workspace::sync_scope_outlines` runs each render: popup scope (or
  hovered chip), else the selected group's mentions; amber/teal by state.

## Verification

- `cargo test --workspace`: 451 passed, 16 ignored; clippy `-D warnings`,
  `cargo fmt --check`, `git diff --check` clean.
- New tests: Date/Other tokens (`mdoc-pii`); Date/name/Other guessing
  (`detector`); Other + cue + cancel after Enter (`pii::ui::tests`); group
  toggle and outlines for group, chip and hover; drops link/no-op/split/merge
  without opening the popup (`mapping_tests`).
- Live QA (temporary bundle, synthetic fixture, session restored):
  "Планируемая дата" → Other/REDACTED_1 with the alias glow and ↶ in the popup
  header; Cancel addition restored the text; footer `Added “…” ↶`; outlines on
  both "Павлова М.С." mentions; mention drag with ghost, accent border and
  "Link to ORG_1"; "New entity" split; group merge "Link to PERSON_1"; click
  collapse; outlines in the light theme.
- Not verified by eye: the category-chip glow (too brief for the capture),
  reduced-motion behaviour (relies on gpui), Windows/Linux, accessibility of
  drag-and-drop (the popup chooser remains the keyboard path).

## Follow-up: new-alias drop area (2026-10-09)

User request: dragging a mention out of its group into free panel space offers
a new alias. While a mention of a multi-mention entity is dragged over the
panel (`MappingUi::drag_in_panel`), the list keeps its rows' height and the
free space below becomes a dashed, rounded area reading "Drop to give it a
new alias → ORG_2" — the next free token of the entity's category
(`Review::next_alias`, a non-reserving preview). It highlights under the
pointer and disappears when the drag leaves the panel or ends. Dropping
separates the mention exactly as before (`MappingAction::Scoped`, one undo
step, no popup). Covered by a GPUI test that drags with real pointer events.
