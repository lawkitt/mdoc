# Pseudonymization: replace a custom selection

Design interview started 2026-10-09. Status: confirmed and implemented 2026-10-09. ADR: [0024](../adr/0024-replace-custom-selection.md).

## Goal

After Pseudonymize has run, the lawyer selects any text in the Markdown editor
that detection missed and turns it into a replacement — simply, intuitively and
elegantly.

## Facts (current code)

- The model already supports it: `Review::add_manual` / `validate_manual`
  (`crates/mdoc-pii/src/lib.rs`) seed a **variant** for the exact selected
  wording, so every exact repeat becomes a proposed mention with one alias.
  Normalized matches reuse an existing identity's alias.
- The UI path is `PiiAddCandidate` (`src/pii/ui.rs`), bound to ⌘⌥P, and two
  entries buried in the Replacements panel's ⋯ menu: "Add selected text" and a
  cycling "Selection type: Person ▾" button (`src/pii/ui/mapping/panel.rs`).
  After adding, the popup opens on the new mention.
- Invalid selections (crossing Markdown syntax, unsafe spans) only report an
  error in the panel after the click.
- The editor's right-click menu (`crates/mdoc-editor`, `DiagMenu`) is internal;
  hosts have no hook to add items yet. `EditorEvent::SelectionChanged` exists for
  caret/selection-anchored host affordances.

## Decisions

Round 1, confirmed 2026-10-09:

1. **Entry points**: a floating Replace pill beside a non-empty selection while
   review is open; a "Replace with placeholder" item in the editor right-click
   menu (needs a host menu-item hook in `mdoc-editor`); ⌘⌥P stays. The ⋯-menu
   "Add selected text" and "Selection type" entries are removed.
2. **Category after adding**: one action adds with a guessed category (pattern
   evidence, otherwise Person); the popup opens on the new mention and its
   existing category chips correct it. No pre-add chooser.
3. **Scope**: every exact repeat of the wording is proposed (same-wording
   variant semantics); popup scope chips narrow it.
4. **State**: added mentions are proposed (amber); the popup opens with
   `Apply · N` as the default so Enter applies.
5. **Availability**: whenever the tab has had a review, open or closed; using it
   after Close review reopens review. Not offered before the first Pseudonymize.
6. **Untidy selections**: silently trim edge whitespace/punctuation and
   unambiguous edge Markdown delimiters. Remaining invalid selections (syntax
   crossed inside, overlap with a mention or placeholder) show the pill disabled
   with a one-line reason — no post-click error.
7. **Linking**: no new UI; normalized-identity reuse plus the popup alias
   chooser's suggestions cover linking to an existing entity.

Round 2, confirmed 2026-10-09:

8. **Category guess**, first match wins: (1) a structured-rule detection
   (`src/pii/detector/structured.rs`) covering the whole selection → its
   category; (2) phone shape (only digits, `+`, `()`, `-`, spaces; ≥ 7 digits) →
   Phone; (3) a company legal form (ООО, АО, ПАО, ИП, LLC, Ltd, Inc, GmbH…) →
   Organization; (4) Person. No model call. Address and Bank are never guessed.
9. **Pill**: compact `Replace`, tooltip ⌘⌥P. Anchored above the selection end
   (below if no room); appears once the selection settles (mouse-up or keyboard
   selection), not while dragging; follows scroll; hides on typing, Escape,
   collapsed selection or when the popup opens. Neutral at rest, amber on hover.
   Disabled with the reason in its tooltip when invalid.
10. **Widening a proposal**: a selection that fully contains only proposed
    mentions at that location supersedes them in the same undo step. Overlap with
    applied text or placeholders, and partial overlap, stays disabled. Other
    mentions of the superseded variant elsewhere stay proposed.
11. **Right-click item**: first, "Replace with placeholder ⌘⌥P", then a
    separator; only with a non-empty selection in a tab that has a review;
    disabled with the reason when invalid; absent without a selection.
12. **No provenance marking**: manual additions look like any replacement and
    survive Rescan. Glossary term **Manual addition**; UI label "Replace".

## Invariants and defaults (not separately asked)

- Adding is one undo step (existing `checkpoint_review`); redo restores it.
- Keyboard path is ⌘⌥P; the pill is a pointer affordance, not a focus stop.
- Originals stay live-tab only, as for detected mentions (ADRs 0018–0023).
- Acceptance: model tests for trimming, guessing and superseding; live QA of
  pill, right-click item, disabled reasons and Enter-to-apply in both themes.

## Implementation notes (2026-10-09)

- **Model** (`mdoc-pii`): `manual.rs` adds `trim_selection` and
  `Review::manual_target`, which returns the trimmed span or a one-line reason
  ("Select identifying text.", "Selection crosses Markdown formatting.",
  "Select whole words.", "Already replaced.", "Already part of a proposed
  replacement.", "Select all of the proposed replacement, or none of it.").
  `Variant` gains a discovery `priority`: manual additions get increasing
  priorities and claim overlapping hits first, which supersedes a contained
  pending mention without touching its other mentions.
- **Editor** (`mdoc-editor`): `SelectionAction` + `set_selection_action` let a
  host offer one action for an exact selection. The editor renders the pill
  (deferred, above the selection end; below near the window top; hidden while
  dragging, unfocused, with a menu open, or after Escape for that selection) and
  the first right-click item, and emits `EditorEvent::SelectionAction`. Mouse-up
  after a drag and double/triple-click now emit `SelectionChanged`. Added
  `set_selection` and `is_selecting`.
- **App**: `detector::guess_category`; `Workspace::sync_selection_action` runs
  on `SelectionChanged` and after every annotation sync; `add_pii_candidate`
  validates with `manual_target`, opens the panel if closed, collapses the
  caret and opens the popup with `ReviewUi::enter_applies`. The ⋯-menu entries
  and `ReviewUi::category` are removed.
- **Refinement during implementation**: an existing test pinned ADR 0020's
  "neutral Enter never applies". Enter therefore applies only in a popup opened
  by a manual addition (flag cleared whenever the popup changes or closes), and
  only then is `Apply · N` drawn in the applied teal as the default.
- **Note**: ИП is in the legal-form list per decision 8, so "ИП Павлова …"
  guesses Organization; the popup corrects it.

## Verification

- `cargo test --workspace`: 446 passed, 16 ignored. `cargo clippy --workspace
  --all-targets -D warnings`, `cargo fmt` and `git diff --check` pass.
- New tests: trimming, reasons, superseding and pending counts (`mdoc-pii`);
  category guessing (`detector`); app flow (not offered before a scan, disabled
  reason, trimmed add with guessed category, popup focus, Enter applies only the
  new wording, two separate undo steps).
- Live macOS QA (temporary `mdocQA.app` bundle on the synthetic fixture,
  session state backed up and restored): pill on double-click and drag
  selections, amber hover and `⌘⌥P` tooltip; click adds and opens the popup;
  Enter applies; the right-click "Replace with placeholder ⌘⌥P" item supersedes
  a contained proposal; ⌘⌥P works; a mid-word drag shows the disabled pill with
  "Select whole words."; Escape hides it. Dark and light themes inspected.
- Not verified: Windows/Linux, IME and screen-reader access to the pill.

## Fixes after user QA (2026-10-09)

- **Pill misplaced in tables / after resizes.** The pill was anchored with
  `bounds_for_offset`, which follows the hidden source row, not table cell
  layout; table widths also follow the window until a column is resized. The
  editor now records the painted selection quads (`selection_bounds`) during
  paint and anchors the pill at the top-right of the last quad, re-rendering on
  the next frame when that geometry moves. A selection scrolled out of view has
  no quads, so the pill hides.
- **Escaped placeholders refused.** `«\_\__» __________ 2026 г.` failed because
  manual spans rejected every `\`, `*`, `_` and `~`. Manual spans now allow them
  and instead require that replacing the span with a token leaves the GFM
  structure of its blank-line-bounded block unchanged (`syntax::plain_text_span`,
  using the `markdown` crate already in the lockfile), and that the span does not
  follow a backslash. Escaped and non-flanking delimiters are text; a span that
  opens or closes emphasis is still "Selection crosses Markdown formatting."
  Discovery applies the same check to every repeat. `[]<>`|#$` and newlines stay
  refused (links, HTML, code, tables, tags, math).
- Verified live in the QA bundle on a synthetic table document: the pill sits
  above the selected cell word before and after zooming the window, and the
  escaped date placeholder is added and applied. Workspace: 447 passed,
  16 ignored; clippy and fmt clean.
