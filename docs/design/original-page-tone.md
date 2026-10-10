# Original page tone — interview record

Grilling session, 2026-10-10. Decisions feed [ADR 0035](../adr/0035-dark-original-pages.md).

## Facts established

- The app theme (`style::Theme`, default Dark) is shared as `Rc<Cell<Theme>>`
  and is not persisted. `PdfView` chrome already follows it via `PdfStyleFn`.
- Page bitmaps are always composited onto white in `gpui_pdf::render_page`.
- DOCX Original is rendered to a temporary PDF and shown in the same `PdfView`
  (`src/preview.rs`), so one crate change covers PDF and DOCX.
- Search-match, link-hover and form-hover overlays use hard-coded hues in
  `gpui-pdf` tuned for white paper.

## Prior art (how viewers solve dark PDFs)

| Approach | Examples | Notes |
| --- | --- | --- |
| Chrome only, pages stay white | Firefox pdf.js default, PDFium `PageColor` | Current mdoc behavior. |
| Plain invert of the bitmap | MuPDF, CSS `filter: invert()` web tools | Hues flip (red→cyan). |
| Invert lightness / luma, hue kept | Okular "Invert Lightness/Luma" | Images inverted too. |
| Duotone remap paper→dark, ink→light, interpolate by luminance | Zathura `recolor` (+ `recolor-keephue`), Okular "Change Dark & Light Colors", Lector `colorScheme="dark"` (#141210 / #eae6e0), Acrobat "Replace Document Colors" | The common dark-reading answer. |
| Selective (skip images) | Chroma heuristic attempt in Okular (misfires on grey images); renderer-level text/line-art only (Acrobat, PDF Studio) | Hard; Okular devs call semantic dark mode "very tricky". |

Known pitfalls from Zathura's tracker: white flash while a page loads and
colour flashes on zoom when the old/new bitmaps have different tones.

## Round 1 decisions

1. **Look**: remap paper to the theme's page background and ink to the theme's
   text colour, interpolating by luminance and keeping the hue of coloured ink
   (Zathura `recolor` + `keephue` model).
2. **Images**: recolour the whole rasterized page in v1; no image detection.
   Scanned PDFs (one full-page image) are recoloured, which is desired.
3. **Control**: pages follow the app theme by default; a header toggle in the
   Original viewer overrides it to show the original paper.
4. **Location**: in `gpui-pdf`, as a live closure read at render time (like
   `PdfQualityFn`), applied in `render_page`'s compositing step.

## Round 2 decisions

5. **Availability and lifetime**: the toggle appears only in the Dark theme.
   The override belongs to one document view, lives in memory only and resets
   when the tab closes. It survives app-theme switches while the tab is open.
6. **Overlays**: search-match, link-hover and form-hover colours move into
   `PdfStyle` so the host sets them per tone; same hues, higher alpha on themed pages.
7. **Transition**: a tone change bumps the render generation (the zoom/quality
   path). Old bitmaps stay until replacements land; a brief mixed-tone moment is
   accepted over doubling page memory. Unrendered slots paint in the paper
   colour so themed pages never flash white.
8. **Colours (Dark)**: paper `#26262b` (distinct from viewer bg `#1e1e22`, border
   kept), ink `#d8d8dc`. New `PdfStyle` fields `page_paper`, `page_ink`.
9. **Toggle**: Original header beside zoom; page icon with half-filled circle;
   tooltip "Show original paper" (themed) / "Show dark pages" (overridden);
   accent while overridden. No keyboard shortcut.
