# Dark Original pages

Status: Accepted and implemented, 2026-10-10. Interview record:
[original page tone](../design/original-page-tone.md).

## Problem

In the dark theme the Original viewer's chrome is dark but PDF and DOCX pages
stay paper-white, producing glare beside the dark Markdown pane.

## Decision

- **Page tone** is a property of the rasterized page: *original* (composite
  over white, today) or *themed* (duotone remap).
- **Themed remap**: luminance interpolates between the theme's page background
  (paper) and page text colour (ink); coloured pixels keep their hue. Applied to
  the whole bitmap, images included — same model as Zathura `recolor`.
- **Control**: tone follows the app theme (Dark → themed, Light → original);
  a header toggle in the Original viewer overrides it to original paper.
- **Location**: `gpui-pdf` owns it. The host supplies themed colours through
  `PdfStyle::themed_pages`, already read at paint time via `PdfStyleFn`, so every
  viewer follows live theme changes. `render_page` takes a `PageTone`. Covers
  DOCX because its preview is a `PdfView`.

## Performance

The remap runs in the existing composite pass as integer arithmetic (luma and a
linear paper↔ink blend, no lookup tables) so the loop stays vectorizable:
about 2.6 ms versus 0.7 ms for the plain composite on a 1640×2120 Retina page
(release build), small next to hayro rasterization (tens of ms on real pages).
No second bitmap is kept; a tone change re-renders only the visible window.

## Consequences

- Photos, signatures and stamps are recoloured in themed tone; the override
  restores fidelity when colour matters.
- Selective (image-preserving) recolouring would need interpreter-level work in
  hayro and is out of scope.
