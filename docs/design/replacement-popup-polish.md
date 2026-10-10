# Replacement popup polish — design interview

Started 2026-10-10. Goal: a simple, elegant replacement popup that users
understand without explanation. Builds on ADRs 0020, 0023, 0024 and 0025.

## Diagnosis (before)

- No hierarchy: category, original, alias, scope chips, pickers, owner and
  actions all render as similar flat grey text; only the Enter default looks
  like a button.
- Counts repeated 3–4 times (`Same wording · 2`, `Person → PERSON_2 · 2` ×10,
  `Apply · 2`, `Keep original · 2`).
- Original and alias stacked with no stated relationship.
- Scope chips have no verb; selected state barely visible.
- Pickers (category, alias chooser, owner) expand inline and stretch the popup.
- Two ↶ icons with different meanings (Cancel addition, Undo replacement);
  `Belongs to … · all 1 mentions` is cryptic.

## Decisions

### Round 1 (confirmed 2026-10-10)

1. **Structure: decision card + progressive disclosure.** The default view is
   four rows: header (category ▾, ‹ n/N ›, ×), `original → alias`, scope,
   actions. Category, alias linking and owner open as floating dropdowns, not
   inline growth.
2. **Original → alias row.** One line `GLDNBA → [ORG_3]`: the original is muted
   and truncated with a full-value tooltip; the alias is a real input box
   (subtle border + fill), not underlined accent text. A long original wraps
   to two lines and keeps the arrow.
3. **Scope.** The three-scope model is unchanged. Present it as a segmented
   control with a verb: `Apply to: This one · All N` when only two scopes
   differ; `This one · Same wording (N) · All variants (M)` when entity ≠
   wording. Selected segment clearly filled.
4. **Actions.** `Apply` is a filled primary (teal) button and `Keep original`
   a secondary/ghost button. Counts live only in the scope control. When Enter
   applies, the primary shows a ↵ hint.
5. **Applied state.** Keep the `Applied` badge (ADR 0023 colours); replace the
   lone footer ↶ icon with a labelled `↶ Undo` button.
6. **Replace pill.** Unchanged, except it adopts the shared secondary button
   style from (4).

Added scope: an elegant animation in the Markdown editor when an original is
replaced by its alias on Apply (details in round 2).

Fact: `mdoc-editor` has no animation support; annotations are static fills
painted per frame. An apply animation needs a transient per-range overlay with
a frame-driven redraw. The app already honours `cx.reduce_motion()`.

### Round 2 (confirmed 2026-10-10)

7. **Category dropdown.** A floating single-column menu: category name left,
   the resulting alias muted right, ✓ on the current category. No counts.
8. **Alias combobox.** A click selects the token and opens a floating dropdown
   under the field: "Same as…" (existing entities, alias bold + original
   muted, suggestions first) and a last row "New alias REDACTED_n". Typing
   filters; new text adds a top row `Rename to "X"` that uses the selected
   scope. The separate "Rename alias for all N mentions" row is removed:
   renaming every mention means selecting "All N" first.
9. **Owner.** A muted row `Belongs to  PERSON_1 ▾` / `Belongs to  nobody ▾`
   with the same floating dropdown; no "· all N mentions" suffix. Shown only
   when an owner is set or the category is a contact/identifier type (email,
   phone, address, identity, tax, bank); hidden for Date and Other.
10. **Cancel addition.** The header ↶ is removed. While the addition (and an
    immediate Apply) is the latest step, the footer secondary reads `Cancel`
    instead of `Keep original`; afterwards it reverts to `Keep original`.
11. **Apply animation.** Direction from the user's reference mock-ups (amber
    chip → blue/teal alias chip, a transient ✓). The settle glow (teal fill
    easing to the resting tint, ~450 ms ease-out) is the baseline and fallback
    if a richer effect proves buggy. Reduced motion: no animation.
12. **Which applies animate.** Every apply path (popup Apply in any scope,
    panel Apply replacements), on visible mentions only; offscreen mentions
    arrive settled. Undo/Keep revert instantly without animation.

Added scope: an animation while a pseudonymization scan runs (reference: an
accent band sweeping over the document; skeleton only if it is less buggy and
more performant).

Facts: during a scan the Markdown is already visible and the scan runs on an
immutable snapshot (`src/pii/ui/scan.rs`); results arrive as one batch. Today
only the panel says "Scanning…". `src/ui.rs` already provides shared animation
helpers that honour reduced motion and cap refreshes at 30 fps (ADR 0015).

### Round 3 (confirmed 2026-10-10)

13. **Replacement chips.** Editor highlights become rounded chips (~3 px
    radius, 1 px border, soft fill): amber for proposals, teal for applied
    (ADR 0023 colours, not the mock's blue). Borders are painted around the
    glyphs with no padding, so text layout and line height are unchanged.
14. **Apply animation.** Text swaps to the alias immediately; the chip eases
    from amber through a brighter teal to resting teal (~450 ms ease-out); a
    small ✓ fades in just after the chip and fades out after ~1 s. Fallback:
    without the ✓ if its placement on wrapped lines proves fiddly.
15. **Scan sweep.** A soft accent gradient band travels top → bottom over the
    visible editor viewport, looping about every 1.8 s at low opacity, as a
    non-interactive overlay on the editor's scroll area (no editor changes).
    The text stays readable and editable; the panel keeps "Scanning…". Reduced
    motion: a static thin accent line at the top of the editor. Skeletons stay
    reserved for states with no text yet.
16. **Results arrival.** Proposal chips fade in together (~250 ms) as the scan
    ends; instant under reduced motion.
17. **No new counter.** The panel footer count (`N mentions to apply` /
    `N applied mentions`) is the only summary; its number updates on apply.
18. **Mention navigation.** Shows `1 of 2` with tooltips "Previous/Next
    mention of ALIAS"; the whole control is hidden when there is one mention.

## Invariants (unchanged)

- Scope semantics, stable aliases, Keep/Undo semantics and their undo/redo
  behaviour (ADRs 0020–0025).
- Enter applies only in a popup opened by a manual addition (ADR 0024).
- Animations honour reduced motion and the 30 fps cap (ADR 0015); no
  animation ever delays or blocks an edit, Apply or Undo.
- The "Hidden source value" note keeps its current place.

## Target layout

```
┌──────────────────────────────────────┐
│ Organization ▾         ‹ 1 of 2 ›  × │
│ GLDNBA  →  [ ORG_3              ✎ ]  │
│ Belongs to  PERSON_1 ▾   (contacts)  │
│ Apply to  ( This one | All 2 )       │
│ [ Apply ↵ ]   Keep original          │
└──────────────────────────────────────┘
```

### Revision (confirmed 2026-10-10, after trying it)

- **Scan sweep removed** (supersedes 15). Instead, a **scan status** sits in
  the toolbar beside Pseudonymize: the shared spinner plus a 12 px muted label
  that crossfades every 1.8 s through Reading… → Analyzing… → Finding names…
  → Linking mentions…, then alternates between the last two until results
  land. The scan exposes no progress, so stages are timed, never presented as
  measured progress. Reduced motion: labels change without fading.
- Found while testing: menus now hang from the bottom edge of their control,
  and chips starting on a soft wrap no longer paint an empty band at the end
  of the previous row.
- **Cancel** (2026-10-10, revised): the scan status itself is the button.
  Hovering crossfades spinner + stage label into a centred "Cancel" in the
  same box (fixed width, so nothing shifts); a click stops the scan. It and
  the panel's Cancel share one path; with nothing found or applied yet it also
  leaves review, so the next Pseudonymize scans afresh.
- **Scope labels** shortened so three segments fit the popup: `This one ·
  Same text 16 · All 87` (counts muted, tooltips spell each scope out); the
  row wraps as a fallback in narrow windows.
- **Replace pill**: a 24 px rounded chip with a tiny amber token (what the
  selection becomes), the label and its muted shortcut; hover tints only the
  border. The tooltip keeps the full "Replace with placeholder".
- **Motion pass** (2026-10-10): every transition uses smoothstep easing.
  Apply: 700 ms, amber → soft teal glow (peak at 35 %) → resting teal; the ✓
  rises 3 px while fading in, holds ~0.7 s, fades out. Undo replacement:
  650 ms teal → amber glow → resting amber with a rising ↶. Keep original: the
  chip is gone, so a chip-shaped ghost (teal for a reverted replacement, amber
  for a declined proposal) fades out over 700 ms (`EditorState::flash_ranges`).
  Reduced motion: none of these play.
- **Text roll** (2026-10-10, after watching it): the colour change alone read
  as instant because the words swapped in one frame. Now the changed words
  roll inside their chip over 600 ms (`EditorState::roll_text`): the previous
  words drift up 6 px and fade out, the new ones rise into place, clipped to
  the chip, then hand off seamlessly to the real text. Used by Apply, Undo
  replacement and Keep original (where the fading ghost is the chip). Wrapped
  (multi-row) spans just swap. Reduced motion: no roll.
- **No marks; steady popup** (2026-10-10): the ✓/↶ marks are dropped — the
  roll and colour change carry the motion alone. The popup flickered on Apply
  and Undo because `set_annotations` clears measured chip bounds, so for one
  frame the popup had no anchor and fell back to the top-left corner; it now
  keeps its last measured anchor until the new chip is measured.
- **Scope ring** (2026-10-10): the selection/scope outline now follows the
  chip's geometry — a ring 2 px outside the chip (5 px radius, 1.5 px, full
  state colour) instead of a 1 px line-height box that overlapped it; it also
  skips the empty soft-wrap band like chips do.
- **State colours** (2026-10-10, from the user's reference): proposals are
  warm gold and applied replacements blue, via `Theme::proposed()` /
  `Theme::applied()` (dark #dfc07c / #6aa7f5, light #9a7516 / #2f6bd0). They
  replace amber/teal in chips, rings, motion, the popup (badge, alias, Apply)
  and the panel. Warnings and errors keep the alert amber; the app accent
  stays teal elsewhere.
