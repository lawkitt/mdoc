# Theme contrast and type ramp

Status: Accepted, 2026-10-23.
Amended by [ADR 0074](0074-one-focus-ring.md): the one outside focus ring.

## Problem

Theme colours and text sizes were chosen by eye. Nothing caught a token
falling under WCAG contrast after a change. The 0.1.4 theme had four text
pairs under 4.5:1 (light muted text on the selected sidebar row, the light
applied alias over a hovered row, dark destructive menu rows, white on the
dark notices badge), and category-coloured mention text fell to 2.8:1 in
light. Two labels also used 9 px text.

## Decision

- **Text contrast (WCAG 2.2, 1.4.3).** Text reaches 4.5:1 on every ground
  it is drawn on, in light and dark. Chrome text is checked on the card, the
  sidebar, a hovered row and the selected row; panel and menu text on the
  card and a hovered row; category colours, which mention words and popup
  aliases are drawn in, on the card and a hovered row; applied aliases in the
  editor on their own tint, at rest and active; Markdown styles (links, code, quotes, alerts) on
  the card. Alpha colours are blended onto their ground first.
- **Non-text contrast (1.4.11).** What a control needs to be seen reaches
  3:1: ticked checkboxes and their tick, category dots and underlines, the
  proposed and applied rings, and the focus accent.
- **Exemptions.**
  - Syntax markers (`**`, `#`, `[ ]( )`) stay dimmed. They repeat what the
    rendered style already shows, and full-strength markers would compete
    with the text.
  - Borders and hairlines are decorative: the card edge, dividers and the
    mention bar's edge. Content and shadow identify those surfaces.
  - The Keychain explainer's drawing of the macOS dialog keeps that
    dialog's sizes. It is an image (`Role::Image`), marked
    `type-ramp: illustration` in the source.
- **Type ramp.** UI text uses 10, 11, 12, 13, 14, 15, 16, 18 and 26 px.
  Nothing is smaller than 10 px.
- **Fixes in this change.**
  - Light muted and placeholder text: `#666670` → `#5f5f69`.
  - Light applied alias: `#2f6bd0` → `#2a62c0`.
  - Dark destructive rows: `#e5484d` → `#f47c7f`.
  - Category colours: lightness 0.66 → 0.70 in dark and 0.42 → 0.32 in light.
  - Applied aliases in the editor are drawn in `applied_text` (`#1c4a94`
    light, `#9cc6fa` dark), which keeps 4.5:1 on their blue tint at rest and
    when active.
  - Notices badge text: dark on the dark theme's amber.
  - The sidebar format label and the badge count: 9 → 10 px.

## Invariants

- `style::contrast_tests` checks every pair above in both themes and fails
  with the pair, ground and ratio.
- The same module scans the sources: every `text_size(px(…))` literal is a
  ramp step unless it carries the illustration marker.

## Consequences

New colours or sizes are added to the test tables or the ramp in the same
change. A new exemption needs a line here.
