//! The editor's render element: layout, prepaint geometry, and paint — split
//! from `lib.rs`.

use super::*;

/// The custom element that lays out + paints the editor's wrapped lines, cursor,
/// and selection, and wires the input handler. Height is content-driven via a
/// measured layout (it depends on the resolved width once soft-wrap is applied).
pub(crate) struct EditorElement {
    pub(crate) editor: Entity<EditorState>,
}

pub(crate) struct PrepaintState {
    /// Text rolls to paint over the text (see [`paint_roll`]).
    rolls: Vec<Roll>,
    wrapped: Vec<WrappedLine>,
    /// Per-line wrap-row count (see [`EditorState::wrap_rows`]).
    wrap_rows: Vec<usize>,
    /// The hovered line's gutter drag grip (line, rect) + its cursor hitbox.
    grip: Option<(usize, Bounds<Pixels>)>,
    grip_hb: Option<Hitbox>,
    /// Top offset of each logical line relative to the editor's top.
    line_tops: Vec<Pixels>,
    /// Per-logical-line wrap-row height (variable for headings + images).
    line_heights: Vec<Pixels>,
    /// `Some` for a line painted as an inline image instead of its source text.
    widgets: Vec<Option<Block>>,
    /// Per-line fenced-code-block background (rounded full-width box).
    backgrounds: Vec<Option<CodeBg>>,
    /// `Some` for a line painted as a table-grid row instead of source.
    tables: Vec<Option<TableRow>>,
    /// Per-line display→source byte map for marker-hidden rows (W6).
    maps: Vec<Option<std::rc::Rc<Vec<usize>>>>,
    /// Per-line gutter decoration (blockquote / list / checkbox).
    marks: Vec<Option<LineMark>>,
    /// Per-line right-to-left geometry (#66); `None` on LTR rows. Committed to
    /// [`EditorState::rtl_rows`] in paint, like `line_insets`.
    rtl: Vec<Option<RtlRow>>,
    /// Per-line inline `$…$` formulas (image + display offset + source range), painted over
    /// their spacers in the shaped text.
    inline_images: Vec<Vec<InlineImage>>,
    /// Corner-grip hitbox for each painted inline image, keyed by logical line
    /// like `checkbox_grips` — so paint can set the resize cursor over each
    /// (hitboxes must be inserted in prepaint). Parallels the images paint
    /// walks, looked up by line.
    image_grips: Vec<(usize, Hitbox)>,
    /// Pointer-cursor hitboxes (`(line, hitbox)`) for clickable gutter checkboxes
    /// and file chips, so the cursor flips to a hand over them (like the image
    /// grips' resize cursor). Set in paint via `set_cursor_style`.
    checkbox_grips: Vec<(usize, Hitbox)>,
    /// Per-code-block chrome (lang tag + Copy), laid out here so paint and the
    /// click rects agree. Hover-revealed: only the hovered card's chips.
    code_chips: Vec<CodeChip>,
    /// Every code card's full bounds (`(first body line, rect)`), committed for
    /// the hover tracking in `on_mouse_move`.
    code_card_rects: Vec<(usize, Bounds<Pixels>)>,
    chip_grips: Vec<(usize, Hitbox)>,
    /// Pointer-cursor hitboxes over foldable-callout chevrons, keyed by line.
    alert_fold_grips: Vec<(usize, Hitbox)>,
    /// Heading fold chevrons to paint: `(line, folded, x)` — x is line-local,
    /// past the heading text. Only hovered or already-folded headings get one.
    heading_chevrons: Vec<(usize, bool, Pixels)>,
    /// Pointer-cursor hitboxes over heading fold chevrons, keyed by line.
    heading_fold_grips: Vec<(usize, Hitbox)>,
    /// Window-space bounds of every heading's first visual row, for
    /// `on_mouse_move`'s hover tracking (committed to the editor in paint).
    heading_row_rects: Vec<(usize, Bounds<Pixels>)>,
    /// Pointer-cursor hitboxes over inline links (`[text](url)` and bare
    /// URLs), so hovering a clickable link shows a hand.
    link_grips: Vec<Hitbox>,
    /// Per-table hover zones (the grid plus pill reach) — repaint gating.
    table_zones: Vec<(Bounds<Pixels>, usize)>,
    table_thumbs: Vec<(TableThumb, Hitbox)>,
    /// Hovered-row / hovered-column border pills + outlines (issue #16).
    row_aff: Option<TableAffordance>,
    col_aff: Option<TableAffordance>,
    /// The caret's cell, outlined in the accent color (Cditor-style).
    caret_cell: Option<(Bounds<Pixels>, Hsla)>,
    /// Column-resize grip bands over the hovered table's vertical borders:
    /// `(band, hitbox, header row, column, current width, border x, table top,
    /// table bottom, accent)`.
    col_resize_grips: Vec<ColResizeGrip>,
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
    /// Find-match highlight quads, painted beneath the selection.
    search: Vec<PaintQuad>,
    search_bounds: Vec<Option<Bounds<Pixels>>>,
    annotations: Vec<PaintQuad>,
    /// Host outlines (review scope), painted over annotation fills.
    outlines: Vec<PaintQuad>,
    annotation_bounds: Vec<(u64, Bounds<Pixels>)>,
    hidden_annotation_hits: Vec<(Vec<u64>, Hitbox)>,
    annotation_counts: Vec<(SharedString, Bounds<Pixels>, Hsla)>,
}

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, ()) {
        // Height depends on the resolved width (soft-wrap), so measure it: shape
        // the content at the available width and count wrapped rows.
        let editor = self.editor.clone();
        // Capture the ambient text style NOW, while the host wrapper's style is
        // on the window's style stack (gpui's Text element does the same). The
        // measure closure runs later, in the layout pass, where the stack is
        // unwound and `text_style()` reverts to the root size — measuring at a
        // different size than paint leaves the element shorter than its painted
        // text (days/pages overlapped at >16px text sizes).
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let id = window.request_measured_layout(style, move |known, available, window, cx| {
            let editor = editor.read(cx);
            let base_lh = font_size * LINE_HEIGHT_RATIO;
            let wrap_width = match available.width {
                AvailableSpace::Definite(w) => Some(w),
                _ => known.width,
            };
            let height = if editor.content.is_empty() {
                // Placeholder rows at the base size.
                let rows = shape_all(
                    window,
                    &editor.placeholder,
                    font_size,
                    text_style.font(),
                    text_style.color,
                    wrap_width,
                )
                .iter()
                .map(|line| line.wrap_boundaries().len() + 1)
                .sum::<usize>()
                .max(1);
                base_lh * rows as f32
            } else {
                // Sum of per-line (variable) heights × each line's wrap rows.
                // Reveal-on-caret applies only while focused (matches prepaint).
                let focused = editor.focus_handle.is_focused(window);
                let caret_row = focused.then(|| editor.row_col(editor.cursor_offset()).0);
                let selection = if focused {
                    (editor.selected_range.start, editor.selected_range.end)
                } else {
                    (usize::MAX, usize::MAX)
                };
                let sf = window.scale_factor();
                let scan = editor.scan_data();
                let shaped = shape_document(
                    window,
                    &editor.content,
                    &text_style.font(),
                    text_style.color,
                    font_size,
                    &editor.diagnostics,
                    &editor.annotation_text_colors(),
                    editor.markdown_style.as_ref(),
                    wrap_width,
                    caret_row,
                    editor.block_image.as_ref(),
                    editor.block_chip.as_ref(),
                    sf,
                    selection,
                    editor.image_resize,
                    editor.table_col_resize.as_ref(),
                    &scan,
                    &editor.shape_caches,
                    editor.shape_band.get().map(|(a, b)| (px(a), px(b))),
                    &editor.folded_headings,
                );
                // Mirror prepaint's `line_tops` walk exactly (same `line_pads`),
                // or the element lays out shorter than it paints.
                let (heights, backgrounds, tables, rows) = (
                    &shaped.heights,
                    &shaped.backgrounds,
                    &shaped.tables,
                    &shaped.wrap_rows,
                );
                let mut y = px(0.);
                let mut needed = px(0.);
                let mut new_tops: Vec<Pixels> = Vec::with_capacity(heights.len());
                for (i, h) in heights.iter().enumerate() {
                    let tbl = tables.get(i).and_then(Option::as_ref);
                    let (top, bot) = line_pads(backgrounds[i], tbl);
                    y += top;
                    new_tops.push(y);
                    let row_h = *h * rows[i] as f32;
                    // A table's hover "+" add-row strip paints just below its
                    // last row (see `TableAdds`). Keep that space inside the
                    // element for a table near the document's end — outside
                    // the element the strip's hitbox is masked off and its
                    // clicks never reach the bounds-gated mouse handlers
                    // (the add-column strip never has this problem: it
                    // borrows horizontal space, which always exists).
                    if let Some(t) = tbl
                        && t.is_last
                        && !t.col_widths.is_empty()
                    {
                        needed = needed.max(y + row_h + (*h * 0.75).max(px(12.)));
                    }
                    y += row_h + bot;
                }
                let total = y.max(base_lh).max(needed);
                // Hand the shaping to this frame's prepaint (see ShapeMemo).
                *editor.shape_memo.borrow_mut() = Some(ShapeMemo {
                    wrap_width,
                    caret_row,
                    selection,
                    font_size,
                    shaped,
                });
                total
            };
            let width = wrap_width.or(known.width).unwrap_or(px(0.));
            size(width, height)
        });
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> PrepaintState {
        let editor = self.editor.read(cx);
        // Share one source index for this frame. Calling EditorState::line_end
        // inside the per-row hitbox loops rescans the whole document per row.
        let line_starts = editor.line_starts();
        let line_end = |row: usize| {
            line_starts
                .get(row + 1)
                .map_or(editor.content.len(), |s| s - 1)
        };
        let row_col = |offset: usize| {
            let row = line_starts
                .partition_point(|&s| s <= offset)
                .saturating_sub(1);
            (row, offset - line_starts[row])
        };

        // Reveal-on-caret (markers, raw-on-caret widgets, per-construct reveal)
        // applies only while the editor is focused. An unfocused editor — always
        // shown in WYSIWYG mode but not being edited — renders fully, like a
        // reading view. `caret_row = None` + a no-match selection do that.
        let focused = editor.focus_handle.is_focused(window);
        // The active image-resize drag (if any), so a dragged image's grip hitbox
        // tracks its live preview size (copied out — `editor` stays borrowed below).
        let image_resize = editor.image_resize;
        let style = window.text_style();
        let font = style.font();

        let font_size = style.font_size.to_pixels(window.rem_size());
        let base_lh = font_size * LINE_HEIGHT_RATIO;
        let wrap_width = Some(bounds.size.width);
        let text_color = style.color;

        // Placeholder (uniform) when empty; else shape per line so headings get
        // their own taller rows (W2) and image lines render inline (W4).
        let caret_row = focused.then(|| row_col(editor.cursor_offset()).0);
        let selection = if focused {
            (editor.selected_range.start, editor.selected_range.end)
        } else {
            (usize::MAX, usize::MAX)
        };
        let sf = window.scale_factor();
        // The measure pass already shaped these exact inputs this frame — use
        // its result instead of shaping the whole document a second time.
        let memo = editor.shape_memo.borrow_mut().take().filter(|m| {
            m.wrap_width == wrap_width
                && m.caret_row == caret_row
                && m.selection == selection
                && m.font_size == font_size
        });
        let ShapedDoc {
            wrapped,
            heights: line_heights,
            widgets,
            backgrounds,
            tables,
            maps,
            marks,
            inline_images,
            wrap_rows,
            rtl_rows: rtl_layout,
        } = if let Some(m) = memo {
            m.shaped
        } else if editor.content.is_empty() {
            let w = shape_all(
                window,
                &editor.placeholder,
                font_size,
                font.clone(),
                hsla(0., 0., 0.5, 0.5),
                wrap_width,
            );
            let n = w.len();
            let rows: Vec<usize> = w.iter().map(|l| l.wrap_boundaries().len() + 1).collect();
            ShapedDoc {
                wrapped: w,
                heights: vec![base_lh; n],
                widgets: vec![None; n],
                backgrounds: vec![None; n],
                tables: vec![None; n],
                maps: vec![None; n],
                marks: vec![None; n],
                inline_images: vec![Vec::new(); n],
                wrap_rows: rows,
                // The placeholder is the host's own string; no RTL layout.
                rtl_rows: (0..n).map(|_| None).collect(),
            }
        } else {
            shape_document(
                window,
                &editor.content,
                &font,
                text_color,
                font_size,
                &editor.diagnostics,
                &editor.annotation_text_colors(),
                editor.markdown_style.as_ref(),
                wrap_width,
                caret_row,
                editor.block_image.as_ref(),
                editor.block_chip.as_ref(),
                sf,
                selection,
                editor.image_resize,
                editor.table_col_resize.as_ref(),
                &editor.scan_data(),
                &editor.shape_caches,
                editor.shape_band.get().map(|(a, b)| (px(a), px(b))),
                &editor.folded_headings,
            )
        };

        // Publish NEXT frame's shaping window: the viewport band in
        // element-local y with a viewport of margin each side, quantized so
        // the band (and with it the measure→prepaint memo) stays stable
        // across small scrolls. Read back one frame stale — both passes of a
        // frame always shape with the same band.
        {
            let vh = f32::from(window.viewport_size().height).max(1.);
            let top = -f32::from(bounds.origin.y);
            let q = 512.0;
            let lo = (((top - vh) / q).floor() * q).max(0.);
            let hi = ((top + 2. * vh) / q).ceil() * q;
            editor.shape_band.set(Some((lo, hi)));
        }

        // Top offset of each logical line (running sum of variable wrap heights),
        // reserving a gap above/below each code block so its padded box has its
        // own space (no overlap with the adjacent line, no blank line required).
        let mut line_tops = Vec::with_capacity(wrapped.len());
        let mut y = px(0.);
        for (idx, lh) in line_heights.iter().enumerate() {
            // Code-box pads plus the table gutter rows (see `line_pads`) — baked
            // into line_tops so the caret / click / paint all shift with them,
            // and neither table affordance overlaps the adjacent line.
            let (top_pad, bot_pad) = line_pads(
                backgrounds.get(idx).copied().flatten(),
                tables.get(idx).and_then(Option::as_ref),
            );
            y += top_pad;
            line_tops.push(y);
            y += *lh * wrap_rows[idx] as f32 + bot_pad;
        }

        // Right-to-left rows (#66): direction from the SOURCE line — the same
        // `base_direction` the reader uses, so the two views can never disagree
        // about which lines are RTL (markdown markers, digits and whitespace
        // are neutral under UAX #9, so `- سلام` reads RTL). Only plain text
        // rows qualify: a code block is LTR by convention, and a table/image
        // widget row paints its own geometry (flipping those is a follow-up).
        // An all-LTR document leaves every entry `None` and pays nothing.
        // The RTL layout computed while shaping (that is where the row COUNT
        // had to be decided), turned into per-row right-align shifts. Rows are
        // shifted individually: a short last row sits further right than a full
        // one, which is what right-aligned text means.
        let rtl: Vec<Option<RtlRow>> = rtl_layout
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                let (base_rtl, rows) = entry?;
                let inset = row_inset(
                    backgrounds.get(i).copied().flatten(),
                    marks.get(i).copied().flatten(),
                );
                let shifts = rows
                    .iter()
                    .map(|r| {
                        if base_rtl {
                            rtl_shift(bounds.size.width, inset, r.width)
                        } else {
                            // Reads left-to-right; it only needed the mapping.
                            px(0.)
                        }
                    })
                    .collect();
                Some(RtlRow {
                    rows,
                    shifts,
                    base_rtl,
                })
            })
            .collect();
        // Row text origin, this frame: the row's inset plus its RTL shift.
        // Fresh-data twin of `EditorState::row_origin_x` (the committed state
        // lags a frame — see `disp_col` below).
        let row_x = |row: usize| {
            row_inset(
                backgrounds.get(row).copied().flatten(),
                marks.get(row).copied().flatten(),
            ) + rtl
                .get(row)
                .and_then(Option::as_ref)
                .and_then(|r| r.shifts.first().copied())
                .unwrap_or(px(0.))
        };
        let bidi = |row: usize| rtl.get(row).and_then(Option::as_ref);

        // Corner-grip hitboxes for each VISIBLE inline image, keyed by logical
        // row (matching paint's viewport walk, like `checkbox_grips`) — the
        // resize cursor is set during paint via these. Mirrors the paint's
        // image-bounds math (row inset + IMG_ROW_PAD, live drag size) exactly so
        // the grip pins to the painted corner (incl. list-item images, which inset
        // past their bullet). Paint culls offscreen rows, so prepaint must too:
        // a whole-document list would otherwise insert a hitbox per image while
        // only a few are visible — and the old positional `image_rects.len()`
        // lookup handed a visible image an earlier offscreen image's hitbox.
        let viewport_h = window.viewport_size().height;
        // Paint culls fully-offscreen rows (see `paint`'s windowed walk); mirror
        // it so offscreen images cost no hitbox. Slack covers pads/affordances.
        let is_visible_row = |i: usize| {
            let advance = line_heights.get(i).copied().unwrap_or(base_lh)
                * wrap_rows.get(i).copied().unwrap_or(1) as f32;
            let origin_y = bounds.origin.y + line_tops.get(i).copied().unwrap_or(px(0.));
            !(origin_y + advance + px(64.) < px(0.) || origin_y - px(64.) > viewport_h)
        };
        let mut image_grips: Vec<(usize, Hitbox)> = Vec::new();
        for (i, w) in widgets.iter().enumerate() {
            if !is_visible_row(i) {
                continue;
            }
            if let Some(Block::Image(img)) = w {
                let inset = row_inset(
                    backgrounds.get(i).copied().flatten(),
                    marks.get(i).copied().flatten(),
                );
                let (img_w, img_h) = image_display_size(img, image_resize, i);
                let img_bounds = Bounds::new(
                    point(
                        bounds.origin.x + inset,
                        bounds.origin.y + line_tops[i] + px(IMG_ROW_PAD / 2.),
                    ),
                    size(img_w, img_h),
                );
                let grip = EditorState::image_grip(img_bounds);
                image_grips.push((i, window.insert_hitbox(grip, HitboxBehavior::Normal)));
            }
        }

        // Pointer-cursor hitboxes for clickable gutter checkboxes + file chips, so
        // the cursor flips to a hand over them. Bounds mirror the paint math; keyed
        // by line so paint sets the cursor on each (see `set_cursor_style`).
        let mut checkbox_grips = Vec::new();
        let mut chip_grips = Vec::new();
        let mut alert_fold_grips = Vec::new();
        for (i, lh) in line_heights.iter().enumerate() {
            if let Some(LineMark::Check { bullet_x, .. }) = marks.get(i).copied().flatten() {
                let sz = font_size * CHECKBOX_SCALE;
                let pad = px(4.);
                let is_rtl = rtl.get(i).and_then(Option::as_ref).is_some();
                let bx = bounds.origin.x + checkbox_x(bullet_x, sz, bounds.size.width, is_rtl);
                let by = bounds.origin.y + line_tops[i] + (*lh - sz) / 2.;
                let hit = Bounds::new(
                    point(bx - pad, by - pad),
                    size(sz + pad * 2., sz + pad * 2.),
                );
                checkbox_grips.push((i, window.insert_hitbox(hit, HitboxBehavior::Normal)));
            }
            if let Some(LineMark::Alert {
                fold: Some(_),
                chevron_x,
                ..
            }) = marks.get(i).copied().flatten()
            {
                // A generous box around the chevron (its glyph is ~an em wide).
                let pad = px(4.);
                let hit = Bounds::new(
                    point(
                        bounds.origin.x + chevron_x - pad,
                        bounds.origin.y + line_tops[i],
                    ),
                    size(font_size + pad * 2., *lh),
                );
                alert_fold_grips.push((i, window.insert_hitbox(hit, HitboxBehavior::Normal)));
            }
            if matches!(
                widgets.get(i).and_then(Option::as_ref),
                Some(Block::Chip { .. })
            ) {
                let hit = Bounds::new(
                    point(bounds.origin.x, bounds.origin.y + line_tops[i]),
                    size(bounds.size.width, *lh),
                );
                chip_grips.push((i, window.insert_hitbox(hit, HitboxBehavior::Normal)));
            }
        }

        // Code-card chrome (Cditor-inspired, issue #16): a language tag + Copy
        // button at each code block's top-right, inside the padded box. Laid
        // out here (paint draws at these bounds; hitboxes flip the cursor).
        let mut code_chips = Vec::new();
        let mut code_card_rects = Vec::new();
        if let Some(st) = editor
            .markdown_style
            .as_ref()
            .filter(|_| !editor.content.is_empty())
        {
            let starts = &line_starts;
            let chip_fs = px(13.);
            let chip_h = px(20.);
            let shape = |window: &mut Window, text: &SharedString, color: Hsla| {
                let run = TextRun {
                    len: text.len(),
                    font: font.clone(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                window
                    .text_system()
                    .shape_line(text.clone(), chip_fs, &[run], None)
                    .width()
            };
            for (i, bg) in backgrounds.iter().enumerate() {
                let Some(cb) = bg.as_ref().filter(|cb| cb.top) else {
                    continue;
                };
                // The card's full bounds (for hover tracking): first body line's
                // padded top through the last block line's padded bottom.
                let mut last = i;
                while last + 1 < backgrounds.len() && backgrounds[last + 1].is_some() {
                    last += 1;
                    if backgrounds[last].as_ref().is_some_and(|b| b.bottom) {
                        break;
                    }
                }
                let (top_pad, _) = code_pads(Some(*cb));
                let (_, last_bot_pad) = code_pads(backgrounds[last]);
                let card_top = bounds.origin.y + line_tops[i] - top_pad;
                let card_bottom = bounds.origin.y
                    + line_tops[last]
                    + line_heights[last] * wrap_rows[last] as f32
                    + last_bot_pad;
                let card = Bounds::new(
                    point(bounds.origin.x, card_top),
                    size(cb.width, card_bottom - card_top),
                );
                code_card_rects.push((i, card));
                // The chrome itself is hover-revealed: only the hovered card
                // lays out (and hit-tests) its chips.
                if editor.code_chip_hover != Some(i) {
                    continue;
                }
                // The opening fence row: this row if the fence is revealed, else
                // the nearest ``` row above (hidden fences collapse to height 0).
                let Some(fence_row) = (0..=i).rev().find(|&r| {
                    starts.get(r).is_some_and(|&s| {
                        editor.content[s..line_end(r)]
                            .trim_start()
                            .starts_with("```")
                    })
                }) else {
                    continue;
                };
                let lang = editor
                    .code_block_at(fence_row)
                    .map(|(l, _)| l)
                    .unwrap_or_default();
                let lang_text: SharedString = if lang.is_empty() {
                    "text ▾".into()
                } else {
                    format!("{lang} ▾").into()
                };
                let copy_text: SharedString = SharedString::new_static("Copy");
                let lang_w = shape(window, &lang_text, st.quote) + px(12.);
                let copy_w = shape(window, &copy_text, st.quote) + px(12.);
                let right = bounds.origin.x + cb.width - px(6.);
                let y = bounds.origin.y + line_tops[i] - top_pad + px(3.);
                let copy_bounds = Bounds::new(point(right - copy_w, y), size(copy_w, chip_h));
                let lang_bounds = Bounds::new(
                    point(right - copy_w - px(4.) - lang_w, y),
                    size(lang_w, chip_h),
                );
                let lang_hb = window.insert_hitbox(lang_bounds, HitboxBehavior::Normal);
                let copy_hb = window.insert_hitbox(copy_bounds, HitboxBehavior::Normal);
                code_chips.push(CodeChip {
                    lang_text,
                    copy_text,
                    lang_bounds,
                    copy_bounds,
                    fence_row,
                    // The popover surface (opaque) — the card tint is
                    // translucent, and the labels must cover the code text.
                    bg: st.popover_bg,
                    fg: st.quote,
                    lang_hb,
                    copy_hb,
                });
            }
        }

        // Heading fold chevrons: every heading row gets a hover-tracking rect;
        // a chevron (+ its hand-cursor hitbox) only when that row is hovered
        // or already folded — one on every heading would clutter.
        let mut heading_chevrons = Vec::new();
        let mut heading_fold_grips = Vec::new();
        let mut heading_row_rects = Vec::new();
        if editor.markdown_style.is_some() && !editor.content.is_empty() {
            let starts = &line_starts;
            for (i, line_shaped) in wrapped.iter().enumerate() {
                // A fence's `# comment` line isn't a heading; folded rows
                // (height 0, inside an outer fold) can't anchor a chevron.
                if backgrounds.get(i).and_then(Option::as_ref).is_some() {
                    continue;
                }
                let (Some(&start), Some(&lh)) = (starts.get(i), line_heights.get(i)) else {
                    continue;
                };
                let line = &editor.content[start..line_end(i)];
                if markdown_syntax::line_heading_level(line).is_none() || lh == px(0.) {
                    continue;
                }
                let top = bounds.origin.y + line_tops[i];
                heading_row_rects.push((
                    i,
                    Bounds::new(point(bounds.origin.x, top), size(bounds.size.width, lh)),
                ));
                let folded = editor.folded_headings.contains(line.trim());
                if !folded && editor.heading_hover_row != Some(i) {
                    continue;
                }
                // Past the heading text (its mark inset + shaped width), capped
                // inside the right edge for a wrapped heading.
                let inset = marks
                    .get(i)
                    .copied()
                    .flatten()
                    .map_or(px(0.), LineMark::inset);
                let x = (inset + line_shaped.width() + px(10.)).min(bounds.size.width - px(20.));
                heading_chevrons.push((i, folded, x));
                let hit = Bounds::new(
                    point(bounds.origin.x + x - px(4.), top),
                    size(font_size + px(8.), lh),
                );
                heading_fold_grips.push((i, window.insert_hitbox(hit, HitboxBehavior::Normal)));
            }
        }

        // Pointer-cursor hitboxes over inline links (`[[wiki]]` / `#tag` /
        // `[text](url)`), so hovering shows a hand like the reading view.
        // Geometry from this frame's shaping: source range → display cols (the
        // row's offset map) → kerned x via position_for_index. A link that
        // crosses a wrap boundary gets a box per end (the rare middle rows of
        // a 3+-row link are skipped). Widget/code/table rows carry no inline
        // links (images and chips have their own machinery).
        let mut link_grips = Vec::new();
        if editor.markdown_style.is_some() && !editor.content.is_empty() {
            let starts = &line_starts;
            for (i, line_shaped) in wrapped.iter().enumerate() {
                if widgets.get(i).and_then(Option::as_ref).is_some()
                    || backgrounds.get(i).and_then(Option::as_ref).is_some()
                    || tables.get(i).and_then(Option::as_ref).is_some()
                {
                    continue;
                }
                let (Some(&start), Some(lh)) = (starts.get(i), line_heights.get(i)) else {
                    continue;
                };
                let line = &editor.content[start..line_end(i)];
                let inset = row_x(i);
                for (range, _) in markdown_syntax::links(line) {
                    let map = maps.get(i).and_then(Option::as_ref);
                    let d1 = display_col_in(map, range.start);
                    let d2 = display_col_in(map, range.end);
                    let (Some(p1), Some(p2)) = (
                        line_pos(line_shaped, bidi(i), d1, *lh),
                        line_pos(line_shaped, bidi(i), d2, *lh),
                    ) else {
                        continue;
                    };
                    if d1 >= d2 {
                        continue; // fully hidden (e.g. collapsed markers)
                    }
                    let origin = point(bounds.origin.x + inset, bounds.origin.y + line_tops[i]);
                    if p1.y == p2.y {
                        // An RTL link ends to the LEFT of where it starts, so
                        // the box spans min→max x rather than p1→p2.
                        let hit = Bounds::new(
                            point(origin.x + p1.x.min(p2.x), origin.y + p1.y),
                            size((p2.x - p1.x).abs(), *lh),
                        );
                        link_grips.push(window.insert_hitbox(hit, HitboxBehavior::Normal));
                    } else {
                        // Wrapped: head runs to the row's end, tail from its row's start.
                        let head = Bounds::new(
                            point(origin.x + p1.x, origin.y + p1.y),
                            size((line_shaped.width() - p1.x).max(px(0.)), *lh),
                        );
                        let tail = Bounds::new(point(origin.x, origin.y + p2.y), size(p2.x, *lh));
                        link_grips.push(window.insert_hitbox(head, HitboxBehavior::Normal));
                        link_grips.push(window.insert_hitbox(tail, HitboxBehavior::Normal));
                    }
                }
            }
        }

        // Gutter drag grip (Cditor/Notion-style block reorder): hovering a
        // line reveals a six-dot handle in the left margin; pressing it grabs
        // the line's block (see `drag_block_rows`). Skipped while a drag is
        // live — the grip would chase the pointer.
        let mouse = window.mouse_position();
        let mut grip: Option<(usize, Bounds<Pixels>)> = None;
        let mut grip_hb: Option<Hitbox> = None;
        // While a drag is live, a viewport-sized hitbox (inserted after the
        // editor's own, so it sits on top) keeps the closed-hand cursor
        // everywhere — otherwise the text I-beam takes over mid-drag.
        if editor.line_drag.is_some() {
            let all = Bounds::new(gpui::Point::default(), window.viewport_size());
            grip_hb = Some(window.insert_hitbox(all, HitboxBehavior::Normal));
        }
        let gl = grip_left(bounds.origin.x);
        if editor.markdown_style.is_some()
            && editor.line_drag.is_none()
            && mouse.x >= gl - px(4.)
            && mouse.x <= bounds.origin.x + bounds.size.width
        {
            let y = mouse.y - bounds.origin.y;
            let row = (0..line_tops.len()).find(|&i| {
                let h = line_heights[i] * wrap_rows[i] as f32;
                h > px(0.5) && y >= line_tops[i] && y < line_tops[i] + h
            });
            if let Some(row) = row {
                let sz = px(14.);
                let rect = Bounds::new(
                    point(
                        gl,
                        bounds.origin.y + line_tops[row] + (line_heights[row] - sz) / 2.,
                    ),
                    size(sz, sz),
                );
                grip_hb = Some(window.insert_hitbox(rect, HitboxBehavior::Normal));
                grip = Some((row, rect));
            }
        }

        // Table interaction (issue #16, Cditor-style): the hovered row/column
        // gets an ACCENT OUTLINE (no fill) plus a small pill ON the table border
        // — "+" inserts after it, "−" deletes it. The caret's cell is outlined
        // too. Paint shows/cursors them only while hovered; on_mouse_down
        // hit-tests the committed pill rects.
        let mut table_zones: Vec<(Bounds<Pixels>, usize)> = Vec::new();
        let mut table_thumbs: Vec<(TableThumb, Hitbox)> = Vec::new();
        let mut row_aff: Option<TableAffordance> = None;
        let mut col_aff: Option<TableAffordance> = None;
        let mut col_resize_grips: Vec<ColResizeGrip> = Vec::new();
        let dragging_col = editor.table_col_resize.as_ref();
        let mut caret_cell: Option<(Bounds<Pixels>, Hsla)> = None;
        let caret_pos = editor.caret_table_cell_pos();
        let mut tbl_top: Option<Pixels> = None;
        let mut tbl_header = 0usize;
        // Pill geometry: thickness across the border, length along it.
        const PILL_ACROSS: f32 = 16.;
        const PILL_ALONG: f32 = 36.;
        for (i, slot) in tables.iter().enumerate() {
            let Some(t) = slot else { continue };
            if t.is_header {
                tbl_top = Some(bounds.origin.y + line_tops[i]);
                tbl_header = i;
            }
            if t.is_last && !t.col_widths.is_empty() {
                let top = tbl_top.unwrap_or(bounds.origin.y + line_tops[i]);
                let bottom = bounds.origin.y + line_tops[i] + line_heights[i];
                let width: Pixels = t.col_widths.iter().copied().sum();
                // A scrolled wide table shifts every affordance with its content.
                let left = editor.table_left(t, i, &bounds);
                let accent = editor.markdown_style.as_ref().map_or(t.border, |s| s.link);
                let g = px(PILL_ACROSS);
                let zone = Bounds::new(
                    point(left - g, top - g),
                    size(width + g * 2., (bottom - top) + g * 2.),
                );
                // Clip to the viewport: a scrolled wide table's CONTENT rect
                // slides left with its scroll offset, but the visible grid
                // stays put — hover and the wheel target what's on screen.
                table_zones.push((zone.intersect(&bounds), tbl_header));

                // The scroll thumb under a wide table: geometry + a
                // hand-cursor hitbox here; paint draws it and mouse-down
                // drags it (see `on_mouse_down`).
                let avail = bounds.size.width - px(TABLE_GUTTER);
                if width > avail
                    && top < window.content_mask().bounds.bottom()
                    && bottom > window.content_mask().bounds.top()
                {
                    let th_w = (avail / f32::from(width) * f32::from(avail)).max(px(24.));
                    let range = f32::from(width - avail);
                    let sx = editor.table_sx(tbl_header, width, avail);
                    // Track left is FIXED (the gutter edge) — `left` is the
                    // scrolled content edge and would drift the thumb. An RTL
                    // table's band sits on the other side and its scroll runs
                    // the other way, so the thumb starts at the track's right.
                    let (track, _) = table_visible_band(bounds.origin.x, bounds.size.width, t.rtl);
                    let frac = f32::from(sx) / range;
                    let frac = if t.rtl { 1. - frac } else { frac };
                    let th_x = track + (avail - th_w) * frac;
                    let mut th_c = t.border;
                    th_c.a = (th_c.a * 1.5).min(0.8);
                    let visible_bottom = bottom.min(window.content_mask().bounds.bottom() - px(4.));
                    let rect =
                        Bounds::new(point(th_x, visible_bottom - px(8.)), size(th_w, px(6.)));
                    let grab = Bounds::new(
                        rect.origin - point(px(4.), px(6.)),
                        rect.size + size(px(8.), px(12.)),
                    );
                    table_thumbs.push((
                        TableThumb {
                            track: Bounds::new(point(track, rect.top()), size(avail, px(6.))),
                            rect,
                            grab,
                            header: tbl_header,
                            // Negative on RTL: the thumb runs against the
                            // scroll offset there (see `frac` above), so a
                            // drag right must decrease it.
                            factor: range / f32::from(avail - th_w).max(1.)
                                * if t.rtl { -1. } else { 1. },
                            color: th_c,
                        },
                        window.insert_hitbox(grab, HitboxBehavior::Normal),
                    ));
                }

                // The caret's cell (this table only), outlined like Cditor's.
                if let Some((crow, ccell, _)) = caret_pos
                    && crow >= tbl_header
                    && crow <= i
                    && let Some(rt) = tables.get(crow).and_then(Option::as_ref)
                    && !rt.col_widths.is_empty()
                {
                    let cc = ccell.min(rt.col_widths.len() - 1);
                    let x: Pixels = left + col_offset(&rt.col_widths, rt.cells.len(), cc, rt.rtl);
                    let rect = Bounds::new(
                        point(x, bounds.origin.y + line_tops[crow]),
                        size(
                            cell_span_width(&rt.col_widths, rt.cells.len(), cc),
                            line_heights[crow],
                        ),
                    );
                    caret_cell = Some((rect, accent));
                }

                // Column-resize grips: a slim band on each column's right
                // border (hover-gated; kept while a drag on this table is live).
                if zone.contains(&mouse) || dragging_col.is_some_and(|r| r.header_row == tbl_header)
                {
                    for (col, &cw) in t.col_widths.iter().enumerate() {
                        // The grip sits on the edge where this column ENDS: its
                        // left on an RTL table, its right otherwise.
                        let cx = left + col_offset(&t.col_widths, t.cells.len(), col, t.rtl);
                        let xacc = if t.rtl { cx } else { cx + cw };
                        let band =
                            Bounds::new(point(xacc - px(3.), top), size(px(6.), bottom - top));
                        col_resize_grips.push(ColResizeGrip {
                            band,
                            hit: window.insert_hitbox(band, HitboxBehavior::Normal),
                            header_row: tbl_header,
                            col,
                            width: f32::from(cw),
                            x: xacc,
                            top,
                            bottom,
                            accent,
                        });
                    }
                }
                if zone.contains(&mouse) {
                    // Hovered BODY row → outline + a vertical pill on the border
                    // the reserved gutter is on: the left one, or an RTL
                    // table's right one (#66) — the pill must land in empty
                    // gutter, not over the note column's edge.
                    let pill_x = if t.rtl { left + width } else { left };
                    for line in tbl_header..=i {
                        let Some(rt) = tables.get(line).and_then(Option::as_ref) else {
                            continue;
                        };
                        if rt.is_separator || rt.is_header {
                            continue;
                        }
                        let rtop = bounds.origin.y + line_tops[line];
                        let rh = line_heights[line];
                        if mouse.y >= rtop && mouse.y < rtop + rh {
                            let ph = px(PILL_ALONG).min(rh - px(2.));
                            let py0 = rtop + (rh - ph) / 2.;
                            let pill = Bounds::new(point(pill_x - g / 2., py0), size(g, ph));
                            let plus = Bounds::new(pill.origin, size(g, ph / 2.));
                            let minus =
                                Bounds::new(point(pill.origin.x, py0 + ph / 2.), size(g, ph / 2.));
                            row_aff = Some(TableAffordance {
                                outline: Bounds::new(point(left, rtop), size(width, rh)),
                                plus,
                                minus,
                                plus_hit: window.insert_hitbox(plus, HitboxBehavior::Normal),
                                minus_hit: window.insert_hitbox(minus, HitboxBehavior::Normal),
                                row: line,
                                col: 0,
                                accent,
                            });
                            break;
                        }
                    }
                    // Hovered column → outline + a horizontal pill on the top border.
                    // Not while the pointer is ON the row's pill — the column
                    // highlight under it is noise when you're about to click.
                    let on_row_pill = row_aff
                        .as_ref()
                        .is_some_and(|a| a.plus.contains(&mouse) || a.minus.contains(&mouse));
                    if !on_row_pill && mouse.x >= left && mouse.x < left + width {
                        for (col, &cw) in t.col_widths.iter().enumerate() {
                            let colx = left + col_offset(&t.col_widths, t.cells.len(), col, t.rtl);
                            if (mouse.x >= colx && mouse.x < colx + cw)
                                || col + 1 == t.col_widths.len()
                            {
                                let pw = px(PILL_ALONG).min(cw - px(2.));
                                let px0 = colx + (cw - pw) / 2.;
                                let pill = Bounds::new(point(px0, top - g / 2.), size(pw, g));
                                let plus = Bounds::new(pill.origin, size(pw / 2., g));
                                let minus = Bounds::new(
                                    point(px0 + pw / 2., pill.origin.y),
                                    size(pw / 2., g),
                                );
                                col_aff = Some(TableAffordance {
                                    outline: Bounds::new(point(colx, top), size(cw, bottom - top)),
                                    plus,
                                    minus,
                                    plus_hit: window.insert_hitbox(plus, HitboxBehavior::Normal),
                                    minus_hit: window.insert_hitbox(minus, HitboxBehavior::Normal),
                                    row: tbl_header,
                                    col,
                                    accent,
                                });
                                break;
                            }
                        }
                    }
                }
                tbl_top = None;
            }
        }
        // A border drag (or the pointer on a resize band) owns the interaction —
        // the row/column outlines + pills would just flicker under it.
        if dragging_col.is_some() || col_resize_grips.iter().any(|gr| gr.band.contains(&mouse)) {
            row_aff = None;
            col_aff = None;
        }

        // Map a (line-relative) point to a screen point. Captures `bounds` (Copy)
        // only, so `line_tops` stays free to move into the prepaint state.
        let to_screen =
            |top: Pixels, p: Point<Pixels>| point(bounds.left() + p.x, bounds.top() + top + p.y);

        // Caret/selection positioning must use THIS frame's fresh per-row data —
        // `editor.offset_maps`/`line_insets` aren't committed until paint, so the
        // method forms would lag a frame (a one-frame caret jump after an edit
        // that hides/reveals markers).
        let disp_col =
            |row: usize, sc: usize| display_col_in(maps.get(row).and_then(Option::as_ref), sc);

        // Cache glyph-position indexes for visible long lines. Cache identity
        // includes shaping, wrap boundaries and width, including caret reveal.
        editor.position_cache.borrow_mut().retain(|&row, _| {
            is_visible_row(row)
                && wrapped.get(row).is_some_and(|line| line.len() > 256)
                && bidi(row).is_none()
        });
        let search_positions: Vec<_> = if editor
            .search
            .as_ref()
            .is_some_and(|(matches, _)| matches.len() > 8)
            || editor.annotations.len() > 8
        {
            wrapped
                .iter()
                .enumerate()
                .map(|(row, line)| {
                    (line.len() > 256 && bidi(row).is_none() && is_visible_row(row))
                        .then(|| {
                            crate::search_geometry::CachedLinePositions::get(
                                &mut editor.position_cache.borrow_mut(),
                                row,
                                line,
                            )
                        })
                        .flatten()
                })
                .collect()
        } else {
            Vec::new()
        };
        let search_position = |row: usize, line: &WrappedLine, col: usize, lh: Pixels| {
            if let Some(Some(positions)) = search_positions.get(row) {
                positions.position(col, lh)
            } else {
                line_pos(line, bidi(row), col, lh)
            }
        };

        // Quads covering source range `s..e`, one per wrap row — the
        // selection's multi-row geometry, shared with the find-match
        // highlights (which paint the same shapes in other colors).
        let range_quads =
            |s: usize, e: usize, color: Hsla, window: &mut Window| -> Vec<PaintQuad> {
                let starts = &line_starts;
                let (s_row, _) = row_col(s);
                let (e_row, _) = row_col(e);
                let right = bounds.size.width;
                let mut sels = Vec::new();
                for row in s_row..=e_row {
                    let Some(line) = wrapped.get(row) else {
                        continue;
                    };
                    let lh = line_heights.get(row).copied().unwrap_or(base_lh);
                    // A collapsed row (hidden marker/fence line, the table's
                    // `|---|` separator, a folded body) has no visible text —
                    // painting its quad would smear a band over its neighbors.
                    if lh <= px(0.5) {
                        continue;
                    }
                    let top = line_tops[row];
                    let line_start = starts[row];
                    let a = s.max(line_start) - line_start;
                    let b = e.min(line_end(row)) - line_start;
                    // Table row: highlight between the cell positions of the selection
                    // ends (not raw-source geometry).
                    if let Some(t) = tables.get(row).and_then(Option::as_ref) {
                        let table_w: Pixels = t.col_widths.iter().copied().sum();
                        let tleft = editor.table_left(t, row, &bounds);
                        // Endpoint x's: inside a cell, the exact caret x; at
                        // or past the row's cell span — or in an empty cell
                        // the shaper can't position — the table's edge, so
                        // vacant trailing cells highlight too.
                        let first_start = t.cell_ranges.first().map_or(0, |r| r.start);
                        let last_end = t.cell_ranges.last().map_or(0, |r| r.end);
                        // Clamp to the table's visible band — a wide
                        // (scrolling) table's cells extend past the
                        // viewport, but its highlight must not.
                        let (vis_l, vis_r) =
                            table_visible_band(bounds.left(), bounds.size.width, t.rtl);
                        let mut band = |lo: Pixels, hi: Pixels, y: Pixels, h: Pixels| {
                            let (lo, hi) = (lo.max(vis_l), hi.min(vis_r));
                            if hi > lo {
                                sels.push(fill(
                                    Bounds::from_corners(point(lo, y), point(hi, y + h)),
                                    color,
                                ));
                            }
                        };
                        let pa = (a >= first_start && a <= last_end)
                            .then(|| table_caret_pos(t, a, tleft, &font, font_size, window))
                            .flatten();
                        let pb = (b >= first_start && b <= last_end)
                            .then(|| table_caret_pos(t, b, tleft, &font, font_size, window))
                            .flatten();
                        match (pa, pb) {
                            // Both ends inside the SAME (possibly wrapped)
                            // cell: per-wrap-row bands within that cell —
                            // one full-height band would smear the whole row.
                            (Some((xa, ya, ca, _)), Some((xb, yb, cb, _)))
                                if ca == cb && xa == xb && ya == yb => {}
                            (Some((xa, ya, ca, _)), Some((xb, yb, cb, _))) if ca == cb => {
                                let pad = px(TABLE_CELL_PAD);
                                let cell_x =
                                    tleft + col_offset(&t.col_widths, t.cells.len(), ca, t.rtl);
                                let cw = cell_span_width(&t.col_widths, t.cells.len(), ca);
                                let (cl, cr) = (cell_x + pad, cell_x + cw - pad);
                                let y0 = bounds.top() + top + px(6.);
                                if ya == yb {
                                    band(
                                        xa.min(xb),
                                        xa.max(xb).max(xa.min(xb) + px(2.)),
                                        y0 + ya,
                                        base_lh,
                                    );
                                } else {
                                    band(xa, cr, y0 + ya, base_lh);
                                    let mut y = ya + base_lh;
                                    while y < yb {
                                        band(cl, cr, y0 + y, base_lh);
                                        y += base_lh;
                                    }
                                    band(cl, xb, y0 + yb, base_lh);
                                }
                            }
                            // Anything else: one full-height band between the
                            // endpoint x's (row edges for out-of-span ends).
                            _ => {
                                let xa = if a <= first_start {
                                    tleft
                                } else {
                                    pa.map_or(tleft, |(x, ..)| x)
                                };
                                let xb = if b >= last_end {
                                    tleft + table_w
                                } else {
                                    pb.map_or(tleft + table_w, |(x, ..)| x)
                                };
                                band(xa.min(xb), xa.max(xb), bounds.top() + top, lh);
                            }
                        }
                        continue;
                    }
                    if disp_col(row, a) == disp_col(row, b) {
                        continue;
                    }
                    let inset = row_x(row);
                    let pa = search_position(row, line, disp_col(row, a), lh).unwrap_or_default();
                    let pb = search_position(row, line, disp_col(row, b), lh).unwrap_or_default();
                    let pa = point(pa.x + inset, pa.y);
                    let pb = point(pb.x + inset, pb.y);
                    if pa.y == pb.y {
                        // On an RTL row the range's END sits to the LEFT of its
                        // start, so order the corners by x. One rect per row is
                        // still all this paints: a selection spanning a
                        // direction change is visually split (see
                        // `VisualMap::rects_for_range`) and needs N — a
                        // follow-up.
                        let (x0, x1) = (pa.x.min(pb.x), pa.x.max(pb.x));
                        sels.push(fill(
                            Bounds::from_corners(
                                to_screen(top, point(x0, pa.y)),
                                to_screen(top, point(x1.max(x0 + px(2.)), pb.y + lh)),
                            ),
                            color,
                        ));
                    } else if let Some(r) = bidi(row) {
                        // RTL, spanning wrap rows: the selection runs the other
                        // way. It leaves the first row at its LEFT edge,
                        // continues from the right on each following row, and
                        // ends part-way across the last. Rows are banded to
                        // their own extent, not the content width — a
                        // right-aligned row doesn't span it.
                        let row_of = |y: Pixels| {
                            (f32::from(y) / f32::from(lh).max(1.0)).round().max(0.) as usize
                        };
                        let (ka, kb) = (row_of(pa.y), row_of(pb.y));
                        let mut band = |k: usize, from: Option<Pixels>, to: Option<Pixels>| {
                            let (l, rt) = r.row_extent(k);
                            let (l, rt) = (l + inset, rt + inset);
                            let (x0, x1) = (from.unwrap_or(l), to.unwrap_or(rt));
                            if x1 > x0 {
                                sels.push(fill(
                                    Bounds::from_corners(
                                        to_screen(top, point(x0, lh * k)),
                                        to_screen(top, point(x1, lh * k + lh)),
                                    ),
                                    color,
                                ));
                            }
                        };
                        // The start is the row's RIGHT-hand portion: from the
                        // row's left edge up to the caret x.
                        band(ka, None, Some(pa.x));
                        for k in (ka + 1)..kb.min(r.row_count()) {
                            band(k, None, None);
                        }
                        // ...and the last row from the end x rightwards.
                        band(kb, Some(pb.x), None);
                    } else {
                        // First wrap row: start x → right edge.
                        sels.push(fill(
                            Bounds::from_corners(
                                to_screen(top, pa),
                                to_screen(top, point(right, pa.y + lh)),
                            ),
                            color,
                        ));
                        // Full middle wrap rows.
                        let mut yy = pa.y + lh;
                        while yy < pb.y {
                            sels.push(fill(
                                Bounds::from_corners(
                                    to_screen(top, point(px(0.), yy)),
                                    to_screen(top, point(right, yy + lh)),
                                ),
                                color,
                            ));
                            yy += lh;
                        }
                        // Last wrap row: left edge → end x.
                        sels.push(fill(
                            Bounds::from_corners(
                                to_screen(top, point(px(0.), pb.y)),
                                to_screen(top, point(pb.x, pb.y + lh)),
                            ),
                            color,
                        ));
                    }
                }
                sels
            };

        // Find-match highlights (host-set): the reader's browser-style find
        // colors — soft yellow everywhere, stronger orange on the active
        // match. Behind the text, like the selection.
        let mut search = Vec::new();
        let mut search_bounds = Vec::new();
        if let Some((matches, active)) = editor.search.as_ref() {
            for (i, search_match) in matches.iter().enumerate() {
                let color: Hsla = if Some(i) == *active {
                    rgba(0xFF9500DD).into()
                } else {
                    rgba(0xFFD60055).into()
                };
                let mut first = None;
                for range in &search_match.source {
                    let quads = range_quads(range.start, range.end, color, window);
                    if first.is_none() {
                        first = quads.first().map(|quad| quad.bounds);
                    }
                    search.extend(quads);
                }
                search_bounds.push(first);
            }
        }

        let mut annotations = Vec::new();
        let mut annotation_bounds = Vec::new();
        let mut hidden_annotation_hits = Vec::new();
        let mut annotation_counts = Vec::new();
        let now = std::time::Instant::now();
        let mut animating = false;
        // Single-row chips by source range, for text rolls.
        let mut chips: Vec<(Range<usize>, PaintQuad)> = Vec::new();
        let mask = window.content_mask().bounds;
        let low = mask.top() - px(64.);
        let high = mask.bottom() + px(64.);
        let mut visible_sources = Vec::new();
        for (row, line) in wrapped.iter().enumerate() {
            let top = bounds.top() + line_tops[row];
            let lh = line_heights[row];
            if lh <= px(0.5) || top + lh * (wrap_rows[row] as f32) < low || top > high {
                continue;
            }
            // Restrict even a single multi-megabyte logical line to visual rows.
            let first = ((f32::from(low - top) / f32::from(lh)).floor().max(0.)) as usize;
            let end = ((f32::from(high - top) / f32::from(lh)).ceil().max(1.)) as usize;
            let boundary = |index: usize| {
                line.wrap_boundaries
                    .get(index)
                    .map(|b| line.unwrapped_layout.runs[b.run_ix].glyphs[b.glyph_ix].index)
            };
            let display_start = if first == 0 {
                0
            } else {
                boundary(first - 1).unwrap_or(line.len())
            };
            let display_end = boundary(end.saturating_sub(1)).unwrap_or(line.len());
            let map = maps.get(row).and_then(Option::as_ref);
            let source_start = map
                .and_then(|m| m.get(display_start))
                .copied()
                .unwrap_or(display_start);
            let source_end = if display_end >= line.len() {
                line_end(row) - line_starts[row]
            } else {
                map.and_then(|m| m.get(display_end))
                    .copied()
                    .unwrap_or(display_end)
            };
            visible_sources.push(line_starts[row] + source_start..line_starts[row] + source_end);
        }
        let mut hidden_annotations: std::collections::BTreeMap<(usize, usize), Vec<(u64, Hsla)>> =
            std::collections::BTreeMap::new();
        if editor.annotation_revision == editor.content_gen {
            for visible in visible_sources {
                let start = editor
                    .annotation_ends
                    .partition_point(|&end| end <= visible.start);
                for annotation in editor.annotations[start..]
                    .iter()
                    .take_while(|a| a.range.start < visible.end)
                {
                    if annotation.range.start >= annotation.range.end
                        || annotation.range.end > editor.content.len()
                        || !editor.content.is_char_boundary(annotation.range.start)
                        || !editor.content.is_char_boundary(annotation.range.end)
                    {
                        continue;
                    }
                    let color = if editor.annotation_hover == Some(annotation.id)
                        || editor.annotation_active == Some(annotation.id)
                    {
                        annotation.active_color
                    } else {
                        annotation.color
                    };
                    let (color, border, mark, active) = flashed(
                        &editor.annotation_flashes,
                        annotation.id,
                        color,
                        annotation.border,
                        now,
                    );
                    animating |= active;
                    let mut quads = range_quads(
                        annotation.range.start.max(visible.start),
                        annotation.range.end.min(visible.end),
                        color,
                        window,
                    );
                    quads.retain(|q| q.bounds.size.width > px(1.));
                    if annotation.border.a > 0. && quads.len() > 1 {
                        // A start on a soft wrap resolves to the end of the
                        // previous row; a bordered chip would show that empty
                        // first band. It is spurious when skipping the first
                        // character removes a row.
                        let start = annotation.range.start.max(visible.start);
                        let next = editor.content[start..]
                            .chars()
                            .next()
                            .map_or(start, |c| start + c.len_utf8());
                        let end = annotation.range.end.min(visible.end);
                        if next < end
                            && range_quads(next, end, color, window)
                                .iter()
                                .filter(|q| q.bounds.size.width > px(1.))
                                .count()
                                < quads.len()
                        {
                            quads.remove(0);
                        }
                    }
                    if annotation.border.a > 0. {
                        // A chip: rounded and bordered, painted around the
                        // glyphs without moving them.
                        for quad in &mut quads {
                            let b = quad.bounds;
                            let inset = (b.size.height * 0.1).min(px(3.));
                            quad.bounds = Bounds::from_corners(
                                point(b.left() - px(2.), b.top() + inset),
                                point(b.right() + px(2.), b.bottom() - inset),
                            );
                            quad.corner_radii = Corners::all(px(3.));
                            quad.border_widths = Edges::all(px(1.));
                            quad.border_color = border;
                        }
                    }
                    if let [chip] = quads.as_slice() {
                        chips.push((annotation.range.clone(), chip.clone()));
                    }
                    if let (Some((text, color, rise)), Some(last)) = (mark, quads.last()) {
                        let b = last.bounds;
                        annotation_counts.push((
                            text,
                            Bounds::new(
                                point(b.right() + px(3.), b.top() + px(rise)),
                                size(px(12.), b.size.height),
                            ),
                            color,
                        ));
                    }
                    if quads.is_empty() {
                        let (row, col) = row_col(annotation.range.start);
                        let lh = if tables.get(row).and_then(Option::as_ref).is_some() {
                            base_lh
                        } else {
                            line_heights[row]
                        };
                        let y = if let Some(table) = tables.get(row).and_then(Option::as_ref) {
                            table_caret_pos(
                                table,
                                col,
                                editor.table_left(table, row, &bounds),
                                &font,
                                font_size,
                                window,
                            )
                            .map(|(_, y, ..)| y)
                        } else {
                            search_position(row, &wrapped[row], disp_col(row, col), lh).map(|p| p.y)
                        }
                        .unwrap_or(px(0.));
                        let visual_row = (f32::from(y) / f32::from(lh)).round().max(0.) as usize;
                        hidden_annotations
                            .entry((row, visual_row))
                            .or_default()
                            .push((annotation.id, color));
                    }
                    for quad in quads
                        .into_iter()
                        .filter(|q| q.bounds.bottom() > low && q.bounds.top() < high)
                    {
                        annotation_bounds.push((annotation.id, quad.bounds));
                        annotations.push(quad);
                    }
                }
            }
        }
        let mut outlines = Vec::new();
        for (start, flash) in &editor.range_flashes {
            let t = now.saturating_duration_since(*start).as_secs_f32()
                / flash.duration.as_secs_f32().max(0.001);
            if t >= 1. || flash.revision != editor.content_gen {
                continue;
            }
            animating = true;
            let keep = 1. - smooth(t);
            for (range, fill, border) in &flash.ranges {
                if range.start >= range.end
                    || range.end > editor.content.len()
                    || !editor.content.is_char_boundary(range.start)
                    || !editor.content.is_char_boundary(range.end)
                {
                    continue;
                }
                let fill = Hsla {
                    a: fill.a * keep,
                    ..*fill
                };
                let border = Hsla {
                    a: border.a * keep,
                    ..*border
                };
                let before = outlines.len();
                outlines.extend(
                    range_quads(range.start, range.end, fill, window)
                        .into_iter()
                        .filter(|q| {
                            q.bounds.size.width > px(1.)
                                && q.bounds.bottom() > low
                                && q.bounds.top() < high
                        })
                        .map(|q| {
                            let b = q.bounds;
                            let inset = (b.size.height * 0.1).min(px(3.));
                            gpui::quad(
                                Bounds::from_corners(
                                    point(b.left() - px(2.), b.top() + inset),
                                    point(b.right() + px(2.), b.bottom() - inset),
                                ),
                                px(3.),
                                fill,
                                px(1.),
                                border,
                                BorderStyle::Solid,
                            )
                        }),
                );
                // A ghost is the chip that rolling text sits in.
                if outlines.len() == before + 1 {
                    chips.push((range.clone(), outlines[before].clone()));
                }
            }
        }
        let mut rolls = Vec::new();
        for (start, roll) in &editor.text_rolls {
            let t = now.saturating_duration_since(*start).as_secs_f32()
                / roll.duration.as_secs_f32().max(0.001);
            if t >= 1. || roll.revision != editor.content_gen {
                continue;
            }
            animating = true;
            // The previous words leave first; the current ones overlap them
            // slightly on the way in.
            let out = smooth(t / 0.55);
            let into = smooth((t - 0.3) / 0.7);
            for (range, previous) in &roll.items {
                if range.start >= range.end
                    || range.end > editor.content.len()
                    || !editor.content.is_char_boundary(range.start)
                    || !editor.content.is_char_boundary(range.end)
                {
                    continue;
                }
                // Only a single-row span rolls; wrapped text just swaps.
                let text_quads: Vec<_> = range_quads(range.start, range.end, text_color, window)
                    .into_iter()
                    .filter(|q| q.bounds.size.width > px(1.))
                    .collect();
                let [text_quad] = text_quads.as_slice() else {
                    continue;
                };
                let glyphs = text_quad.bounds;
                if glyphs.bottom() <= low || glyphs.top() >= high {
                    continue;
                }
                let chip = chips
                    .iter()
                    .find(|(r, _)| r == range)
                    .map(|(_, q)| q.clone());
                let roll_color = editor
                    .annotations
                    .iter()
                    .find(|a| a.range == *range)
                    .and_then(|a| a.text_color)
                    .unwrap_or(text_color);
                let shape = |text: SharedString, alpha: f32, window: &mut Window| {
                    let run = TextRun {
                        len: text.len(),
                        font: font.clone(),
                        color: Hsla {
                            a: roll_color.a * alpha,
                            ..roll_color
                        },
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    window
                        .text_system()
                        .shape_line(text, font_size, &[run], None)
                };
                let current: SharedString = editor.content[range.clone()].to_string().into();
                rolls.push(Roll {
                    glyphs,
                    chip,
                    surface: roll.surface,
                    previous: (out < 1.).then(|| (shape(previous.clone(), 1. - out, window), out)),
                    current: (into > 0.).then(|| (shape(current, into, window), into)),
                });
            }
        }
        if animating {
            window.request_animation_frame();
        }
        if editor.outline_revision == editor.content_gen {
            for (range, color) in &editor.outlines {
                if range.start >= range.end
                    || range.end > editor.content.len()
                    || !editor.content.is_char_boundary(range.start)
                    || !editor.content.is_char_boundary(range.end)
                {
                    continue;
                }
                let mut quads: Vec<_> = range_quads(range.start, range.end, *color, window)
                    .into_iter()
                    .filter(|q| q.bounds.size.width > px(1.))
                    .collect();
                // Like chips: a start on a soft wrap yields an empty first band.
                if quads.len() > 1 {
                    let next = editor.content[range.start..]
                        .chars()
                        .next()
                        .map_or(range.start, |c| range.start + c.len_utf8());
                    if next < range.end
                        && range_quads(next, range.end, *color, window)
                            .iter()
                            .filter(|q| q.bounds.size.width > px(1.))
                            .count()
                            < quads.len()
                    {
                        quads.remove(0);
                    }
                }
                // A ring that follows the chip's shape with an even 2 px gap.
                outlines.extend(
                    quads
                        .into_iter()
                        .filter(|q| q.bounds.bottom() > low && q.bounds.top() < high)
                        .map(|q| {
                            let b = q.bounds;
                            let inset = (b.size.height * 0.1).min(px(3.));
                            gpui::quad(
                                Bounds::from_corners(
                                    point(b.left() - px(4.), b.top() + inset - px(2.)),
                                    point(b.right() + px(4.), b.bottom() - inset + px(2.)),
                                ),
                                px(5.),
                                gpui::transparent_black(),
                                px(1.5),
                                *color,
                                BorderStyle::Solid,
                            )
                        }),
                );
            }
        }
        for ((row, visual_row), mut markers) in hidden_annotations {
            markers.sort_by_key(|(id, _)| *id);
            markers.dedup_by_key(|(id, _)| *id);
            if let Some(top) = line_tops.get(row) {
                let count = markers.len();
                let color = markers[0].1;
                let width = if count > 1 { px(24.) } else { px(8.) };
                let lh = if tables.get(row).and_then(Option::as_ref).is_some() {
                    base_lh
                } else {
                    line_heights[row]
                };
                let marker = Bounds::new(
                    point(
                        (bounds.left() - width - px(4.)).max(px(2.)),
                        bounds.top() + *top + lh * visual_row as f32,
                    ),
                    size(width, lh),
                );
                let ids: Vec<_> = markers.iter().map(|(id, _)| *id).collect();
                annotation_bounds.extend(ids.iter().map(|&id| (id, marker)));
                hidden_annotation_hits
                    .push((ids, window.insert_hitbox(marker, HitboxBehavior::Normal)));
                annotations.push(fill(marker, color));
                if count > 1 {
                    annotation_counts.push((
                        count.to_string().into(),
                        marker,
                        window.text_style().color,
                    ));
                }
            }
        }

        let (cursor, selections) = if editor.content.is_empty() {
            let c = fill(
                Bounds::new(
                    point(bounds.left(), bounds.top()),
                    size(px(CARET_WIDTH), base_lh),
                ),
                text_color,
            );
            (Some(c), Vec::new())
        } else if editor.selected_range.is_empty() {
            let (row, col) = row_col(editor.cursor_offset());
            let lh = line_heights.get(row).copied().unwrap_or(base_lh);
            let top = line_tops.get(row).copied().unwrap_or(px(0.));
            // Caret on an image row: the picture stays rendered (a Word-style
            // atomic object), so paint an image-height caret parked at its edge
            // — before the picture at the line's start, after it anywhere else —
            // instead of a text caret placed by the hidden source glyphs.
            if let Some(Block::Image(img)) = widgets.get(row).and_then(Option::as_ref) {
                let inset = row_x(row);
                let (img_w, img_h) = image_display_size(img, image_resize, row);
                let x = bounds.left() + inset + if col == 0 { px(-3.) } else { img_w + px(3.) };
                let c = fill(
                    Bounds::new(
                        point(x, bounds.top() + top + px(IMG_ROW_PAD / 2.)),
                        size(px(CARET_WIDTH), img_h),
                    ),
                    text_color,
                );
                (Some(c), Vec::new())
            } else if let Some(t) = tables.get(row).and_then(Option::as_ref)
                && let Some((x, y_off, _, _)) = table_caret_pos(
                    t,
                    col,
                    editor.table_left(t, row, &bounds),
                    &font,
                    font_size,
                    window,
                )
            {
                // Top pad + the wrap row's y (0 for unwrapped cells).
                let y = bounds.top() + top + px(6.) + y_off;
                let c = fill(
                    Bounds::new(point(x, y), size(px(CARET_WIDTH), base_lh)),
                    text_color,
                );
                (Some(c), Vec::new())
            } else {
                let p = wrapped
                    .get(row)
                    .and_then(|l| line_pos(l, bidi(row), disp_col(row, col), lh))
                    .unwrap_or_default();
                let inset = row_x(row);
                // A row can be taller than its text — grown to fit an inline
                // formula, or breathing like a list item (LIST_ROW_GAP) — with
                // the glyphs centered in it. The caret matches the TEXT height
                // (the row's shaped font size × the ratio, so headings keep
                // their taller carets), centered the same way, not the row.
                let text_lh = wrapped
                    .get(row)
                    .map_or(base_lh, |l| {
                        l.unwrapped_layout.font_size * LINE_HEIGHT_RATIO
                    })
                    .min(lh);
                let c = fill(
                    Bounds::new(
                        to_screen(top, point(p.x + inset, p.y + (lh - text_lh) / 2.)),
                        size(px(CARET_WIDTH), text_lh),
                    ),
                    text_color,
                );
                (Some(c), Vec::new())
            }
        } else {
            // Selection tint = the theme accent at low alpha (fallback: a fixed blue).
            let color = editor
                .markdown_style
                .as_ref()
                .map_or(rgba(0x3b82f640).into(), |s| {
                    let mut c = s.link;
                    c.a = 0.25;
                    c
                });
            (
                None,
                range_quads(
                    editor.selected_range.start,
                    editor.selected_range.end,
                    color,
                    window,
                ),
            )
        };

        PrepaintState {
            wrapped,
            wrap_rows,
            grip,
            grip_hb,
            line_tops,
            line_heights,
            widgets,
            backgrounds,
            tables,
            maps,
            marks,
            rtl,
            inline_images,
            image_grips,
            checkbox_grips,
            code_chips,
            code_card_rects,
            chip_grips,
            alert_fold_grips,
            heading_chevrons,
            heading_fold_grips,
            heading_row_rects,
            link_grips,
            table_zones,
            table_thumbs,
            row_aff,
            col_aff,
            caret_cell,
            col_resize_grips,
            cursor,
            selections,
            search,
            search_bounds,
            annotations,
            outlines,
            annotation_bounds,
            hidden_annotation_hits,
            annotation_counts,
            rolls,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        for quad in prepaint.annotations.drain(..) {
            window.paint_quad(quad);
        }
        for quad in prepaint.outlines.drain(..) {
            window.paint_quad(quad);
        }
        for quad in prepaint.search.drain(..) {
            window.paint_quad(quad);
        }
        // The selection pill anchors on the painted selection (table cells,
        // wraps, resizes); re-render once when that geometry moves.
        let selection_bounds: Vec<_> = prepaint.selections.iter().map(|q| q.bounds).collect();
        let moved = self.editor.update(cx, |editor, _| {
            let moved = editor.selection_bounds != selection_bounds;
            editor.selection_bounds = selection_bounds;
            moved && editor.selection_action.is_some()
        });
        if moved {
            let editor = self.editor.clone();
            window.on_next_frame(move |_, cx| editor.update(cx, |_, cx| cx.notify()));
        }
        for sel in prepaint.selections.drain(..) {
            window.paint_quad(sel);
        }

        let style = window.text_style();
        let font = style.font();
        for (text, marker, color) in prepaint.annotation_counts.drain(..) {
            let run = TextRun {
                len: text.len(),
                font: font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let size = if !text.chars().all(|c| c.is_ascii_digit()) {
                px(12.)
            } else {
                px(10.)
            };
            let line = window.text_system().shape_line(text, size, &[run], None);
            let _ = line.paint(
                marker.origin + point((marker.size.width - line.width()) / 2., px(1.)),
                marker.size.height,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
        }
        let text_color = style.color;
        let font_size = style.font_size.to_pixels(window.rem_size());
        let base_lh = font_size * LINE_HEIGHT_RATIO;
        // Inline-image resize: the accent color for the corner grips, and the
        // active drag (if any) so the dragged image paints at its live width.
        let grip_color = self
            .editor
            .read(cx)
            .markdown_style
            .as_ref()
            .map_or(text_color, |s| s.link);
        let image_resize = self.editor.read(cx).image_resize;
        // Window-space bounds of each painted image + its logical line, collected
        // for the next frame's grip hit-testing (committed below).
        let mut image_rects: Vec<(usize, Bounds<Pixels>)> = Vec::new();
        // Window-space box bounds of each painted task checkbox + its line, for the
        // next frame's click-to-toggle hit-testing (committed below).
        let mut checkbox_rects: Vec<(usize, Bounds<Pixels>)> = Vec::new();
        // A foldable callout's chevron bounds (from this paint), so a click can
        // flip its fold char — the checkbox-toggle pattern.
        let mut alert_fold_rects: Vec<(usize, Bounds<Pixels>)> = Vec::new();
        let mut heading_fold_rects: Vec<(usize, Bounds<Pixels>)> = Vec::new();
        // Logseq-style list nesting guides: `outline` holds the bullet x of each
        // active ancestor level, so a faint vertical line can drop from each down
        // through its descendants. Popped on dedent, reset off the list.
        let mut outline: Vec<Pixels> = Vec::new();
        let viewport_h = window.viewport_size().height;
        for (i, ((line, top), lh)) in prepaint
            .wrapped
            .iter()
            .zip(prepaint.line_tops.iter())
            .zip(prepaint.line_heights.iter())
            .enumerate()
        {
            let origin = point(bounds.origin.x, bounds.origin.y + *top);
            // Right-to-left row (#66): its text right-aligns by `shift`, and
            // every gutter decoration mirrors to the other side so the markers
            // sit where the text now starts — matching the reader.
            let rtl = prepaint.rtl.get(i).and_then(Option::as_ref);
            // Row 0's shift: the gutter decorations mirror against the line as
            // a whole, and the per-row shifts only move the text.
            let shift = rtl
                .and_then(|r| r.shifts.first().copied())
                .unwrap_or(px(0.));
            let content_w = bounds.size.width;
            // Nesting guides for a list/task row: a thin vertical line at each
            // ancestor bullet's x, spanning this row (contiguous rows stack into a
            // continuous guide).
            match prepaint.marks.get(i).copied().flatten() {
                Some(LineMark::List {
                    bullet_x, color, ..
                })
                | Some(LineMark::Check {
                    bullet_x, color, ..
                }) => {
                    while outline.last().is_some_and(|&x| x >= bullet_x) {
                        outline.pop();
                    }
                    let guide = Hsla {
                        a: color.a * 0.5,
                        ..color
                    };
                    for &gx in &outline {
                        let g = gx + px(3.);
                        let g = if rtl.is_some() {
                            rtl_marker_x(content_w, g, px(1.))
                        } else {
                            g
                        };
                        window.paint_quad(fill(
                            Bounds::new(point(origin.x + g, origin.y), size(px(1.), *lh)),
                            guide,
                        ));
                    }
                    outline.push(bullet_x);
                }
                _ => outline.clear(),
            }
            // Windowed paint: a line fully outside the window's viewport paints
            // nothing. The guide bookkeeping above still runs — a visible list
            // row's ancestor guides can start above the viewport. The slack
            // covers code-box pads and table affordances that overhang the row.
            let advance = *lh * prepaint.wrap_rows.get(i).copied().unwrap_or(1) as f32;
            if origin.y + advance + px(64.) < px(0.) || origin.y - px(64.) > viewport_h {
                continue;
            }
            // Fenced code block: one rounded, content-fit box (sized to the
            // widest line, like a table). The first line rounds + pads the top, the
            // last rounds + pads the bottom; the pad fills the layout gap reserved
            // for it (see `code_pads`), so the caret geometry stays text-height and
            // the box never overlaps an adjacent line.
            if let Some(cb) = prepaint.backgrounds.get(i).copied().flatten() {
                let r = px(6.);
                let z = px(0.);
                let (top_pad, bot_pad) = code_pads(Some(cb));
                let corners = Corners {
                    top_left: if cb.top { r } else { z },
                    top_right: if cb.top { r } else { z },
                    bottom_left: if cb.bottom { r } else { z },
                    bottom_right: if cb.bottom { r } else { z },
                };
                let box_origin = point(origin.x, origin.y - top_pad);
                let box_size = size(cb.width, *lh + top_pad + bot_pad);
                window.paint_quad(
                    fill(Bounds::new(box_origin, box_size), cb.color).corner_radii(corners),
                );
            }
            // Blockquote: a muted 2px left border down the line (the body is inset
            // past it by QUOTE_INSET).
            if let Some(LineMark::Quote { bar, .. }) = prepaint.marks.get(i).copied().flatten() {
                // RTL: the rule belongs on the right, where the text starts —
                // mirroring the left-side bar.
                let bx = origin.x + rtl.map_or(px(0.), |_| content_w - px(2.));
                window.paint_quad(fill(
                    Bounds::new(point(bx, origin.y), size(px(2.), *lh)),
                    bar,
                ));
            }
            // Heading fold chevron (hovered or folded headings only): painted
            // past the heading text, muted; clicking toggles the fold (rects
            // committed for on_mouse_down, like the callout chevron).
            if let Some(&(_, folded, cx0)) =
                prepaint.heading_chevrons.iter().find(|(r, _, _)| *r == i)
            {
                let glyph: SharedString = if folded { "▸" } else { "▾" }.into();
                let mut muted = text_color;
                muted.a *= 0.45;
                let run = TextRun {
                    len: glyph.len(),
                    font: font.clone(),
                    color: muted,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window
                    .text_system()
                    .shape_line(glyph, font_size, &[run], None);
                let gx = origin.x + cx0;
                let _ = shaped.paint(
                    point(gx, origin.y),
                    *lh,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
                heading_fold_rects.push((
                    i,
                    Bounds::new(point(gx, origin.y), size(shaped.width(), *lh)),
                ));
                if let Some(hb) = prepaint
                    .heading_fold_grips
                    .iter()
                    .find_map(|(l, hb)| (*l == i).then_some(hb))
                {
                    window.set_cursor_style(CursorStyle::PointingHand, hb);
                }
            }
            // Alert marker line: the colored bar plus a bold label ("Note", …)
            // where the hidden `[!NOTE]` marker was.
            if let Some(LineMark::Alert {
                bar,
                label,
                fold,
                chevron_x,
                ..
            }) = prepaint.marks.get(i).copied().flatten()
            {
                // An RTL callout mirrors like the rest of the row: the rule on
                // the right, then the icon, then the label running leftward.
                // Each box is reflected on its own, which reverses their order
                // exactly as intended.
                let amirror = |x: Pixels, w: Pixels| {
                    if rtl.is_some() {
                        origin.x + content_w - (x - origin.x) - w
                    } else {
                        x
                    }
                };
                window.paint_quad(fill(
                    Bounds::new(
                        point(amirror(origin.x, px(2.)), origin.y),
                        size(px(2.), *lh),
                    ),
                    bar,
                ));
                // Foldable callout: a chevron after the label; clicking it flips
                // the `-`/`+` in the source (rects committed for on_mouse_down).
                if let Some(folded) = fold {
                    let glyph: SharedString = if folded { "▸" } else { "▾" }.into();
                    let run = TextRun {
                        len: glyph.len(),
                        font: font.clone(),
                        color: bar,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let shaped = window
                        .text_system()
                        .shape_line(glyph, font_size, &[run], None);
                    let cx0 = amirror(origin.x + chevron_x, shaped.width());
                    let _ = shaped.paint(
                        point(cx0, origin.y),
                        *lh,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                    alert_fold_rects.push((
                        i,
                        Bounds::new(point(cx0, origin.y), size(shaped.width(), *lh)),
                    ));
                    if let Some(hb) = prepaint
                        .alert_fold_grips
                        .iter()
                        .find_map(|(l, hb)| (*l == i).then_some(hb))
                    {
                        window.set_cursor_style(CursorStyle::PointingHand, hb);
                    }
                }
                // The bold label.
                let label_x = origin.x + px(QUOTE_INSET);
                let label_font = Font {
                    weight: FontWeight::BOLD,
                    ..font.clone()
                };
                let run = TextRun {
                    len: label.len(),
                    font: label_font,
                    color: bar,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window
                    .text_system()
                    .shape_line(label.into(), font_size, &[run], None);
                let _ = shaped.paint(
                    point(amirror(label_x, shaped.width()), origin.y),
                    *lh,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
            // Thematic break: a 1px full-width divider centered in the row.
            if let Some(LineMark::Rule(c)) = prepaint.marks.get(i).copied().flatten() {
                let y = origin.y + (*lh - px(1.)) / 2.;
                let w = bounds.size.width;
                window.paint_quad(fill(Bounds::new(point(origin.x, y), size(w, px(1.))), c));
            }
            // List item: a muted bullet (`•`) or number (`N.`) glyph where the
            // hidden source marker began (`bullet_x`); the body is inset to the
            // measured prefix width so it lines up with the raw line.
            if let Some(LineMark::List {
                bullet_x,
                ordered,
                num,
                level,
                color,
                ..
            }) = prepaint.marks.get(i).copied().flatten()
            {
                let glyph: SharedString = if ordered {
                    crate::syntax::ordered_marker(level, num).into()
                } else {
                    "•".into()
                };
                let run = TextRun {
                    len: glyph.len(),
                    font: font.clone(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window
                    .text_system()
                    .shape_line(glyph, font_size, &[run], None);
                let bx = if rtl.is_some() {
                    rtl_marker_x(content_w, bullet_x, shaped.width())
                } else {
                    bullet_x
                };
                let _ = shaped.paint(
                    point(origin.x + bx, origin.y),
                    *lh,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
            // Task item: a crisp cap-height box (custom-drawn, not a font glyph so
            // it reads at the text's size) with a checkmark when done.
            if let Some(LineMark::Check {
                bullet_x,
                checked,
                color,
                accent,
                ..
            }) = prepaint.marks.get(i).copied().flatten()
            {
                let sz = font_size * CHECKBOX_SCALE;
                let bx = origin.x + checkbox_x(bullet_x, sz, content_w, rtl.is_some());
                let by = origin.y + (*lh - sz) / 2.; // vertically centered on the line
                let box_bounds = Bounds::new(point(bx, by), size(sz, sz));
                checkbox_rects.push((i, box_bounds));
                if let Some(hb) = prepaint
                    .checkbox_grips
                    .iter()
                    .find_map(|(l, hb)| (*l == i).then_some(hb))
                {
                    window.set_cursor_style(CursorStyle::PointingHand, hb);
                }
                // Done: a solid accent fill with a white check (Notion-style).
                // Open: an empty outline.
                window.paint_quad(PaintQuad {
                    bounds: box_bounds,
                    corner_radii: Corners::all(px(3.)),
                    background: if checked {
                        accent.into()
                    } else {
                        hsla(0., 0., 0., 0.).into()
                    },
                    border_widths: Edges::all(px(1.5)),
                    border_color: if checked { accent } else { color },
                    border_style: BorderStyle::Solid,
                });
                if checked {
                    let s = f32::from(sz);
                    let mut pb = PathBuilder::stroke(px(1.6));
                    pb.move_to(point(bx + px(s * 0.24), by + px(s * 0.52)));
                    pb.line_to(point(bx + px(s * 0.42), by + px(s * 0.70)));
                    pb.line_to(point(bx + px(s * 0.76), by + px(s * 0.28)));
                    if let Ok(path) = pb.build() {
                        window.paint_path(path, gpui::white());
                    }
                }
            }
            if let Some(t) = prepaint.tables.get(i).and_then(Option::as_ref) {
                // A wide table keeps natural columns, scrolled by sx and
                // clipped to the viewport — rows paint shifted under a mask.
                let table_w: Pixels = t.col_widths.iter().copied().sum();
                let avail = bounds.size.width - px(TABLE_GUTTER);
                // The clip band, on the side this table's gutter ISN'T (#66).
                let (tleft, _) = table_visible_band(origin.x, bounds.size.width, t.rtl);
                let content_left = self.editor.read(cx).table_left(t, i, &bounds);
                let g = px(TABLE_GUTTER);
                let mask = gpui::ContentMask {
                    bounds: Bounds::new(point(tleft, origin.y - g), size(avail, *lh + g * 2.)),
                };
                // The header row paints the whole table's rounded outer border
                // (one box around all its rows, matching the reading view) — for
                // the Grid style only; the others are box-less. It spans the
                // WHOLE table's height, so it gets its own full-height mask
                // (the per-row mask below would clip it to the header band).
                if t.is_header && matches!(t.style, markdown_syntax::TableStyle::Grid) {
                    let mut total_h = px(0.);
                    for j in i..prepaint.tables.len() {
                        match prepaint.tables[j].as_ref() {
                            Some(tr) => {
                                total_h += prepaint.line_heights[j];
                                if tr.is_last {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    let border_mask = gpui::ContentMask {
                        bounds: Bounds::new(
                            point(tleft, origin.y - g),
                            size(avail, total_h + g * 2.),
                        ),
                    };
                    window.with_content_mask(Some(border_mask), |window| {
                        window.paint_quad(PaintQuad {
                            bounds: Bounds::new(
                                point(content_left, origin.y),
                                size(table_w, total_h),
                            ),
                            corner_radii: Corners::all(px(6.)),
                            background: hsla(0., 0., 0., 0.).into(),
                            border_widths: Edges::all(px(1.)),
                            border_color: t.border,
                            border_style: BorderStyle::Solid,
                        });
                    });
                }
                window.with_content_mask(Some(mask), |window| {
                    paint_table_row(
                        t,
                        point(content_left, origin.y),
                        *lh,
                        &font,
                        font_size,
                        base_lh,
                        text_color,
                        window,
                        cx,
                    );
                });
            } else if let Some(Block::Image(w)) = prepaint.widgets.get(i).and_then(Option::as_ref) {
                // Inline image (W4a): paint the decoded image instead of source,
                // inset to the row's gutter so a list-item image sits past its
                // bullet (painted above, like any list row).
                let inset = row_inset(
                    prepaint.backgrounds.get(i).copied().flatten(),
                    prepaint.marks.get(i).copied().flatten(),
                );
                // While this image's grip is being dragged, preview the live width
                // (aspect-preserved from the saved size) instead of the saved
                // `{width=N}` — the source isn't rewritten until release.
                let (img_w, img_h) = image_display_size(w, image_resize, i);
                let img_x = origin.x + inset;
                let img_bounds = Bounds::new(
                    point(img_x, origin.y + px(IMG_ROW_PAD / 2.)),
                    size(img_w, img_h),
                );
                let _ = window.paint_image(
                    img_bounds,
                    img_bounds,
                    Corners::default(),
                    w.img.clone(),
                    0,
                    false,
                );
                // A draggable corner grip (accent square) + the resize cursor over it,
                // via the hitbox inserted in prepaint. Recorded in `image_rects` for the
                // next frame's grip hit-testing. Keyed by logical line, like
                // `checkbox_grips` below.
                let grip = EditorState::image_grip(img_bounds);
                window.paint_quad(fill(grip, grip_color).corner_radii(Corners::all(px(3.))));
                if let Some((_, hitbox)) = prepaint.image_grips.iter().find(|(line, _)| *line == i)
                {
                    window.set_cursor_style(CursorStyle::ResizeLeftRight, hitbox);
                }
                image_rects.push((i, img_bounds));
            } else if let Some(Block::Chip {
                label,
                link,
                bg,
                border,
                ..
            }) = prepaint.widgets.get(i).and_then(Option::as_ref)
            {
                // File chip (e.g. a PDF embed): a rounded button with the label.
                paint_chip(
                    label, *link, *bg, *border, origin, *lh, &font, font_size, window, cx,
                );
                if let Some(hb) = prepaint
                    .chip_grips
                    .iter()
                    .find_map(|(l, hb)| (*l == i).then_some(hb))
                {
                    window.set_cursor_style(CursorStyle::PointingHand, hb);
                }
            } else {
                // Code blocks + gutter marks inset their text (kept in sync with
                // `EditorState::line_inset` / the fresh prepaint inset).
                let inset = row_inset(
                    prepaint.backgrounds.get(i).copied().flatten(),
                    prepaint.marks.get(i).copied().flatten(),
                );
                let text_origin = point(origin.x + inset + shift, origin.y);
                if let Some(r) = rtl {
                    // Our own rows, painted in reading order — gpui's wrapping
                    // is not used for this line at all (#66). Each row is
                    // right-aligned on its own, so a short last row sits
                    // further right than a full one. `paint_row` draws the run
                    // backgrounds (the inline-code tint) too.
                    for (k, row) in r.rows.iter().enumerate() {
                        gpui_bidi::paint_row(
                            row,
                            point(
                                origin.x + inset + r.shifts.get(k).copied().unwrap_or(px(0.)),
                                origin.y + *lh * k,
                            ),
                            *lh,
                            window,
                            cx,
                        );
                    }
                } else {
                    // Run backgrounds (the inline-code highlight) paint separately from
                    // the glyphs — `paint` alone wouldn't show them.
                    let _ = line.paint_background(
                        text_origin,
                        *lh,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                    let _ = line.paint(text_origin, *lh, gpui::TextAlign::Left, None, window, cx);
                }
                // Inline `$…$` formulas: paint each typeset raster over its spacer, centered on
                // the text row. Record each formula's window bounds for click-to-edit; the one
                // being edited shows the seated editor instead of its raster.
                for im in prepaint.inline_images.get(i).into_iter().flatten() {
                    // The spacer's LEFT edge and which row it landed on. On an
                    // RTL row `display_off` is its RIGHT edge — anchoring there
                    // paints the formula over the words beside it and leaves
                    // the reserved gap empty — so the row's map gives the
                    // spacer's actual visual box instead.
                    let seat = match rtl {
                        Some(r) => {
                            let (k, local) = r.row_of(im.display_off);
                            r.rows.get(k).and_then(|rr| {
                                let end = local + im.len;
                                rr.map.rects_for_range(local..end).first().map(|&(x0, _)| {
                                    (
                                        inset + r.shifts.get(k).copied().unwrap_or(px(0.)) + px(x0),
                                        *lh * k,
                                    )
                                })
                            })
                        }
                        None => line
                            .position_for_index(im.display_off, *lh)
                            .map(|p| (inset + p.x, p.y)),
                    };
                    if let Some((x, row_y)) = seat {
                        let x = origin.x + x;
                        // Center the image in the (grown-to-fit) wrap row.
                        let y = origin.y + row_y + (*lh - im.height) / 2.0;
                        let b = Bounds::new(point(x, y), size(im.width, im.height));
                        let _ =
                            window.paint_image(b, b, Corners::default(), im.img.clone(), 0, false);
                    }
                }
            }
        }

        if focus.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        // Table "+" affordances (issue #16): while the pointer is over a table's
        // hover zone, paint its add-row (below) + add-column (right) strips, cursor
        // them (unconditionally, so gpui applies the hand from its cached map as the
        // pointer moves onto a strip), and commit their rects for on_mouse_down. The
        // hovered strip fills; on_mouse_move drives the repaints (the editor
        // otherwise only repaints on the caret blink). Zones are committed every
        // frame so on_mouse_move knows where the tables are.
        let mouse = window.mouse_position();
        let table_hover_zones: Vec<(Bounds<Pixels>, usize)> = prepaint.table_zones.clone();
        let mut table_row_add_rects: Vec<(Bounds<Pixels>, usize)> = Vec::new();
        let mut table_col_add_rects: Vec<(Bounds<Pixels>, usize, usize)> = Vec::new();
        let mut table_row_del = None;
        let mut table_col_del = None;
        // The caret's cell: a quiet accent outline (under the hover outlines).
        if let Some((rect, accent)) = prepaint.caret_cell {
            let mut c = accent;
            c.a *= 0.55;
            paint_table_outline(rect, c, window);
        }
        // Hovered row/column: accent outline + a border pill with "+" / "−"
        // (Cditor-style — no fill highlight). Rects are committed for
        // on_mouse_down's hit-tests.
        if let Some(a) = &prepaint.row_aff {
            paint_table_outline(a.outline, a.accent, window);
            paint_table_pill(a, false, mouse, window);
            window.set_cursor_style(CursorStyle::PointingHand, &a.plus_hit);
            window.set_cursor_style(CursorStyle::PointingHand, &a.minus_hit);
            table_row_add_rects.push((a.plus, a.row));
            table_row_del = Some((a.minus, a.row));
        }
        if let Some(a) = &prepaint.col_aff {
            paint_table_outline(a.outline, a.accent, window);
            paint_table_pill(a, true, mouse, window);
            window.set_cursor_style(CursorStyle::PointingHand, &a.plus_hit);
            window.set_cursor_style(CursorStyle::PointingHand, &a.minus_hit);
            table_col_add_rects.push((a.plus, a.row, a.col));
            table_col_del = Some((a.minus, a.row, a.col));
        }
        // Column-resize grips: a resize cursor over each border band; the
        // hovered/dragged border draws in the accent color.
        let mut table_col_resize_rects: Vec<(Bounds<Pixels>, usize, usize, f32)> = Vec::new();
        let dragging = self.editor.read(cx).table_col_resize.as_ref();
        for gr in &prepaint.col_resize_grips {
            window.set_cursor_style(CursorStyle::ResizeLeftRight, &gr.hit);
            table_col_resize_rects.push((gr.band, gr.header_row, gr.col, gr.width));
            let active = dragging.is_some_and(|d| d.header_row == gr.header_row && d.col == gr.col)
                || (dragging.is_none() && gr.band.contains(&mouse));
            if active {
                window.paint_quad(fill(
                    Bounds::new(
                        point(gr.x - px(1.), gr.top),
                        size(px(2.), gr.bottom - gr.top),
                    ),
                    gr.accent,
                ));
            }
        }

        // Wide-table scroll thumbs (from prepaint): the slim track under a
        // wide table's last row, with a hand cursor over its grab band —
        // draggable (see `on_mouse_down`).
        for (thumb, hb) in &prepaint.table_thumbs {
            window.paint_quad(
                fill(
                    thumb.track,
                    Hsla {
                        a: 0.12,
                        ..thumb.color
                    },
                )
                .corner_radii(Corners::all(px(3.))),
            );
            window.paint_quad(fill(thumb.rect, thumb.color).corner_radii(Corners::all(px(1.5))));
            window.set_cursor_style(CursorStyle::PointingHand, hb);
        }
        // Gutter drag grip: six muted dots on the hovered line; while a drag
        // is live, a full-width accent bar marks the drop boundary instead.
        {
            let ed = self.editor.read(cx);
            let accent = ed.markdown_style.as_ref().map_or(text_color, |s| s.link);
            let mut dot_c = ed.markdown_style.as_ref().map_or(text_color, |s| s.marker);
            dot_c.a = (dot_c.a * 1.6).min(0.9);
            if let Some((bs, be, t)) = ed.line_drag {
                let y = prepaint.line_tops.get(t).copied().unwrap_or_else(|| {
                    let i = prepaint.line_tops.len().saturating_sub(1);
                    prepaint.line_tops.last().copied().unwrap_or(px(0.))
                        + prepaint.line_heights.get(i).copied().unwrap_or(px(0.))
                            * prepaint.wrap_rows.get(i).copied().unwrap_or(1) as f32
                });
                // No bar when the boundary is a no-op (inside the block).
                if !(t >= bs && t <= be + 1) {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(bounds.origin.x, bounds.origin.y + y - px(1.)),
                            size(bounds.size.width, px(2.)),
                        ),
                        accent,
                    ));
                }
                if let Some(hb) = &prepaint.grip_hb {
                    window.set_cursor_style(CursorStyle::ClosedHand, hb);
                }
            } else if let Some((_, rect)) = prepaint.grip {
                // Snap each dot to the DEVICE pixel grid: at fractional scale
                // factors (Windows 100/125/150%) the 2.5px dots otherwise
                // straddle pixel boundaries at per-dot subpixel phases and
                // antialias to visibly different sizes (macOS @2x masked it).
                let scale = window.scale_factor();
                let snap = |v: Pixels| px((f32::from(v) * scale).round() / scale);
                let d = snap(px(2.5));
                let gap = px(4.5);
                for col in 0..2 {
                    for dr in 0..3 {
                        let dot = Bounds::new(
                            point(
                                snap(rect.origin.x + px(2.) + gap * col as f32),
                                snap(rect.origin.y + px(1.) + gap * dr as f32),
                            ),
                            size(d, d),
                        );
                        window.paint_quad(fill(dot, dot_c).corner_radii(Corners::all(d * 0.5)));
                    }
                }
                if let Some(hb) = &prepaint.grip_hb {
                    window.set_cursor_style(CursorStyle::OpenHand, hb);
                }
            }
        }
        // The grip sits OUTSIDE the editor div's bounds, so the div's own
        // mouse listeners never see a press on it — start the drag from a
        // window-level listener instead, gated on the grip's HITBOX (not the
        // bare rect): is_hovered respects occlusion, so a dialog/menu/popover
        // covering the gutter blocks the drag from starting through it.
        if let (Some((row, _)), Some(hb)) = (prepaint.grip, prepaint.grip_hb.clone()) {
            let editor = self.editor.clone();
            window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
                if phase == gpui::DispatchPhase::Bubble
                    && e.button == MouseButton::Left
                    && hb.is_hovered(window)
                {
                    editor.update(cx, |ed, cx| {
                        let (bs, be) = ed.drag_block_rows(row);
                        ed.line_drag = Some((bs, be, bs));
                        ed.is_selecting = false;
                        ed.menu = None;
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            });
        }
        // Track the hovered grip row from a window-level listener — the
        // gutter sits outside the div, so its own on_mouse_move never fires
        // there. Every editor's listener sees every pointer move, so the
        // common miss (pointer nowhere near this editor) must stay cheap: a
        // read-only check first (grip_hover_row_at bails on the x band and
        // y range in O(1)), entity update only on an actual change — and no
        // listener at all for raw-view editors.
        if self.editor.read(cx).markdown_style.is_some() {
            let editor = self.editor.clone();
            window.on_mouse_event(move |e: &MouseMoveEvent, phase, _w, cx| {
                if phase == gpui::DispatchPhase::Bubble {
                    let ed = editor.read(cx);
                    let gr = ed.grip_hover_row_at(e.position);
                    if gr != ed.grip_hover_row {
                        editor.update(cx, |ed, cx| {
                            ed.grip_hover_row = gr;
                            cx.notify();
                        });
                    }
                }
            });
        }
        // While a drag is live the pointer usually travels down the GUTTER —
        // still outside the div, where its on_mouse_move / on_mouse_up never
        // fire — so track the boundary and land the drop from window-level
        // listeners too (the div's own handlers cover in-bounds travel; both
        // paths take/compare the same state, so double delivery is a no-op).
        if self.editor.read(cx).line_drag.is_some() {
            let editor = self.editor.clone();
            window.on_mouse_event(move |e: &MouseMoveEvent, phase, _w, cx| {
                if phase == gpui::DispatchPhase::Bubble {
                    editor.update(cx, |ed, cx| {
                        if let Some((bs, be, t)) = ed.line_drag {
                            let b = ed.snap_drop_boundary(ed.drop_boundary_at(e.position));
                            if b != t {
                                ed.line_drag = Some((bs, be, b));
                                cx.notify();
                            }
                        }
                    });
                }
            });
            let editor = self.editor.clone();
            window.on_mouse_event(move |e: &MouseUpEvent, phase, _w, cx| {
                if phase == gpui::DispatchPhase::Bubble && e.button == MouseButton::Left {
                    editor.update(cx, |ed, cx| {
                        if let Some((bs, be, t)) = ed.line_drag.take() {
                            ed.apply_line_drag(bs, be, t, cx);
                            cx.notify();
                        }
                    });
                }
            });
        }

        let wrapped = std::mem::take(&mut prepaint.wrapped);
        let line_tops = std::mem::take(&mut prepaint.line_tops);
        let line_heights = std::mem::take(&mut prepaint.line_heights);
        let offset_maps = std::mem::take(&mut prepaint.maps);
        let widget_rows: Vec<bool> = prepaint
            .widgets
            .iter()
            .enumerate()
            .map(|(i, w)| w.is_some() || prepaint.tables.get(i).is_some_and(Option::is_some))
            .collect();
        let table_rows = std::mem::take(&mut prepaint.tables);
        let line_insets: Vec<Pixels> = prepaint
            .backgrounds
            .iter()
            .zip(prepaint.marks.iter())
            .map(|(bg, mark)| row_inset(*bg, *mark))
            .collect();
        let chip_rows: Vec<Option<SharedString>> = prepaint
            .widgets
            .iter()
            .map(|w| match w {
                Some(Block::Chip { src, .. }) => Some(src.clone()),
                _ => None,
            })
            .collect();
        // Code-card chrome: the language tag + Copy button laid out in prepaint.
        // Each label sits on an opaque pill of the card color so it stays
        // readable over a long first line; hover brightens it.
        let mut code_chip_rects = Vec::new();
        for chip in &prepaint.code_chips {
            for (bounds_, text, hb) in [
                (chip.lang_bounds, &chip.lang_text, &chip.lang_hb),
                (chip.copy_bounds, &chip.copy_text, &chip.copy_hb),
            ] {
                let hovered = hb.is_hovered(window);
                window.paint_quad(PaintQuad {
                    bounds: bounds_,
                    corner_radii: Corners::all(px(4.)),
                    background: chip.bg.into(),
                    border_widths: Edges::all(px(0.)),
                    border_color: chip.fg,
                    border_style: BorderStyle::Solid,
                });
                let color = if hovered { text_color } else { chip.fg };
                let run = TextRun {
                    len: text.len(),
                    font: font.clone(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window
                    .text_system()
                    .shape_line(text.clone(), px(13.), &[run], None);
                let _ = shaped.paint(
                    point(
                        bounds_.origin.x + (bounds_.size.width - shaped.width()) / 2.,
                        bounds_.origin.y + px(1.),
                    ),
                    bounds_.size.height - px(2.),
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
                window.set_cursor_style(CursorStyle::PointingHand, hb);
            }
            code_chip_rects.push(CodeChipHit {
                lang: chip.lang_bounds,
                copy: chip.copy_bounds,
                fence_row: chip.fence_row,
            });
        }

        // Hidden-source markers sit in the gutter outside the editor div.
        // Use real hitboxes so clipping and occluding popups block activation.
        if !prepaint.hidden_annotation_hits.is_empty() {
            let hits = prepaint.hidden_annotation_hits.clone();
            for (_, hit) in &hits {
                window.set_cursor_style(CursorStyle::PointingHand, hit);
            }
            let editor = self.editor.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == gpui::DispatchPhase::Capture
                    && event.button == MouseButton::Left
                    && !event.modifiers.shift
                    && event.click_count == 1
                    && let Some((id, _)) = hits.iter().find(|(_, hit)| hit.is_hovered(window))
                {
                    editor.update(cx, |editor, cx| {
                        if editor.annotation_revision == editor.content_gen {
                            if id.len() == 1 {
                                cx.emit(EditorEvent::ActivateAnnotation(id[0]));
                            } else {
                                cx.emit(EditorEvent::ActivateAnnotations(id.clone()));
                            }
                        }
                    });
                    cx.stop_propagation();
                }
            });
            let hits = prepaint.hidden_annotation_hits.clone();
            let editor = self.editor.clone();
            window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
                if phase == gpui::DispatchPhase::Bubble {
                    let id = hits
                        .iter()
                        .find(|(_, hit)| hit.is_hovered(window))
                        .and_then(|(ids, _)| ids.first().copied());
                    let current = editor.read(cx).annotation_hover;
                    // Visible annotations use the div's existing move listener.
                    if (id.is_some()
                        || current
                            .is_some_and(|id| hits.iter().any(|(keys, _)| keys.contains(&id))))
                        && id != current
                    {
                        editor.update(cx, |editor, cx| {
                            editor.annotation_hover = id;
                            cx.notify();
                        });
                    }
                }
            });
        }

        // Text rolls sit over the painted text, inside their chips.
        for roll in prepaint.rolls.drain(..) {
            paint_roll(roll, window, cx);
        }

        // Hovering an inline link shows a hand, like the reading view (the
        // hitboxes come from prepaint; cursor styles must be set during paint).
        for hb in &prepaint.link_grips {
            window.set_cursor_style(CursorStyle::PointingHand, hb);
        }
        self.editor.update(cx, |editor, _| {
            editor.search_bounds = std::mem::take(&mut prepaint.search_bounds);
            editor.hidden_annotation_ids = prepaint
                .hidden_annotation_hits
                .iter()
                .flat_map(|(ids, _)| ids.iter().copied())
                .collect();
            editor.annotation_bounds = std::mem::take(&mut prepaint.annotation_bounds);
            editor.wrapped = wrapped;
            editor.line_tops = line_tops;
            editor.line_heights = line_heights;
            editor.wrap_rows = std::mem::take(&mut prepaint.wrap_rows);
            editor.widget_rows = widget_rows;
            editor.offset_maps = offset_maps;
            editor.chip_rows = chip_rows;
            editor.line_insets = line_insets;
            editor.rtl_rows = std::mem::take(&mut prepaint.rtl);
            editor.table_rows = table_rows;
            editor.image_rects = image_rects;
            editor.checkbox_rects = checkbox_rects;
            editor.table_thumbs = prepaint.table_thumbs.iter().map(|(t, _)| *t).collect();
            editor.code_chip_rects = code_chip_rects;
            editor.table_col_resize_rects = table_col_resize_rects;
            editor.code_card_rects = std::mem::take(&mut prepaint.code_card_rects);
            editor.alert_fold_rects = alert_fold_rects;
            editor.heading_fold_rects = heading_fold_rects;
            editor.heading_row_rects = std::mem::take(&mut prepaint.heading_row_rects);
            editor.table_row_add_rects = table_row_add_rects;
            editor.table_col_add_rects = table_col_add_rects;
            editor.table_hover_zones = table_hover_zones;
            editor.table_row_del = table_row_del;
            editor.table_col_del = table_col_del;
            editor.last_bounds = Some(bounds);
            editor.last_paint_gen = editor.content_gen;
            editor.line_height = base_lh;
            editor.font_size = font_size;
            editor.paint_font = Some(font.clone());
        });
    }
}

/// Shape `text` into wrapped lines at `wrap_width` (one [`WrappedLine`] per
/// logical line, each carrying its own wrap boundaries). Empty on a shaping
/// error, so the editor degrades to blank rather than panicking.
fn shape_all(
    window: &mut Window,
    text: &SharedString,
    font_size: Pixels,
    font: Font,
    color: Hsla,
    wrap_width: Option<Pixels>,
) -> Vec<WrappedLine> {
    let run = TextRun {
        len: text.len(),
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let runs: &[TextRun] = if text.is_empty() {
        &[]
    } else {
        std::slice::from_ref(&run)
    };
    window
        .text_system()
        .shape_text(text.clone(), font_size, runs, wrap_width, None)
        .map(|lines| lines.into_vec())
        .unwrap_or_default()
}

/// Fit-to-width display size for an inline image from its natural (device) size:
/// cap to the content width (or an explicit `{width=N}`), preserving aspect.
fn block_img(
    img: Arc<RenderImage>,
    width_attr: Option<f32>,
    wrap_width: Option<Pixels>,
    scale_factor: f32,
) -> Option<BlockImg> {
    let dev = img.size(0);
    let (dw, dh) = (dev.width.0 as f32, dev.height.0 as f32);
    if dw <= 0. || dh <= 0. || scale_factor <= 0. {
        return None;
    }
    let natural_w = dw / scale_factor;
    let avail = wrap_width.map_or(natural_w, f32::from);
    let target_w = width_attr.unwrap_or(natural_w).min(avail).max(1.);
    Some(BlockImg {
        img,
        width: px(target_w),
        height: px(target_w * dh / dw),
    })
}

/// An inline image's painted size: the saved `BlockImg` size, unless its grip is
/// being dragged (`resize.line == line`), in which case the live drag width wins
/// and the height scales with it (aspect preserved). Used by both the prepaint
/// (grip hitbox) and the paint (image + grip), so the preview stays consistent.
fn image_display_size(w: &BlockImg, resize: Option<ImageResize>, line: usize) -> (Pixels, Pixels) {
    match resize {
        Some(r) if r.line == line => (px(r.width), w.height * (r.width / f32::from(w.width))),
        _ => (w.width, w.height),
    }
}

/// Horizontal text inset for a row from its decorations: [`CODE_INSET`] inside a
/// fenced code block, else the gutter mark's inset (blockquote/list), else zero.
/// At most one applies per line.
fn row_inset(bg: Option<CodeBg>, mark: Option<LineMark>) -> Pixels {
    if bg.is_some() {
        px(CODE_INSET)
    } else {
        mark.map_or(px(0.), LineMark::inset)
    }
}

/// The reserved vertical gap above (`.0`) and below (`.1`) a row, from its
/// code-block background: [`CODE_PAD`] above the block's first line and below its
/// last. Added to the line tops + total height so the padded box has real layout
/// space and never overlaps adjacent lines.
fn code_pads(bg: Option<CodeBg>) -> (Pixels, Pixels) {
    match bg {
        Some(cb) => (
            if cb.top { px(CODE_PAD) } else { px(0.) },
            if cb.bottom { px(CODE_PAD) } else { px(0.) },
        ),
        None => (px(0.), px(0.)),
    }
}

/// A row's full reserved gap above (`.0`) and below (`.1`): the code-block pads
/// plus a table's gutter rows (above the header for the column "−" handles,
/// below the last row for the add-row "+" strip). ONE function shared by the
/// measured layout and prepaint so they can never disagree — when they did
/// (measure missed the table gutters), the element laid out ~2×TABLE_GUTTER
/// shorter per table than it painted, and clicks over the shortfall never
/// reached the editor (the add-row "+" strip's dead bottom half).
fn line_pads(bg: Option<CodeBg>, table: Option<&TableRow>) -> (Pixels, Pixels) {
    let (mut top, mut bot) = code_pads(bg);
    if let Some(t) = table {
        if t.is_header {
            top += px(TABLE_GUTTER);
        }
        if t.is_last {
            bot += px(TABLE_GUTTER);
        }
    }
    (top, bot)
}

/// Recolour `runs` over `tints` (line-local source ranges). `map` takes a
/// display byte to its source byte; `None` means display is source.
fn tint_runs(
    text: &str,
    runs: &[TextRun],
    map: Option<&[usize]>,
    tints: &[(Range<usize>, Hsla)],
) -> Vec<TextRun> {
    let tint_at = |d: usize| {
        let src = map.map_or(d, |m| m.get(d).copied().unwrap_or(d));
        tints
            .iter()
            .find(|(r, _)| r.contains(&src))
            .map(|(_, c)| *c)
    };
    let mut out = Vec::with_capacity(runs.len() + 2 * tints.len());
    let mut d = 0;
    for run in runs {
        let end = (d + run.len).min(text.len());
        let mut start = d;
        let mut tint = tint_at(d);
        let mut push = |from: usize, to: usize, tint: Option<Hsla>| {
            if to > from {
                out.push(TextRun {
                    len: to - from,
                    color: tint.unwrap_or(run.color),
                    ..run.clone()
                });
            }
        };
        for (i, _) in text.get(d..end).unwrap_or_default().char_indices().skip(1) {
            let next = tint_at(d + i);
            if next != tint {
                push(start, d + i, tint);
                start = d + i;
                tint = next;
            }
        }
        push(start, d + run.len, tint);
        d += run.len;
    }
    out
}

/// Inline `![](src)` images: swap each ready image's glyphs for a spacer to
/// paint the raster over. A caret strictly inside an image's
/// `![…](…)` leaves it raw for editing. Sizing matches the reader: ~40px tall,
/// capped 240px wide, aspect from the raster's pixels.
#[allow(clippy::too_many_arguments)]
fn shape_inline_images(
    window: &mut Window,
    line: &str,
    disp: String,
    runs: Vec<TextRun>,
    map: Vec<usize>,
    caret_col: Option<usize>,
    base_font: &Font,
    fs: Pixels,
    block_image: &BlockImageFn,
) -> (String, Vec<TextRun>, Vec<usize>, Vec<InlineImage>) {
    let spans = markdown_syntax::inline_image_spans(line);
    if spans.is_empty() {
        return (disp, runs, map, Vec::new());
    }
    let space_w = f32::from(measure_width(window, " ", base_font, fs)).max(1.);
    let mut places: Vec<(Range<usize>, usize)> = Vec::new();
    let mut imgs: Vec<(Arc<RenderImage>, Pixels, Pixels)> = Vec::new();
    for (full, src) in spans {
        if caret_col.is_some_and(|c| full.start < c && c < full.end) {
            continue; // editing the raw `![](src)`
        }
        let Some(img) = block_image(&line[src]) else {
            continue; // remote / PDF / not-yet-decoded → leave raw
        };
        let sz = img.size(0);
        let (pw, ph) = (sz.width.0 as f32, sz.height.0 as f32);
        if pw <= 0. || ph <= 0. {
            continue;
        }
        let mut h = 40.0_f32;
        let mut w = h * pw / ph;
        if w > 240. {
            let s = 240. / w;
            w *= s;
            h *= s;
        }
        let n = ((w / space_w).ceil() as usize).max(1);
        places.push((full, n));
        imgs.push((img, px(w), px(h)));
    }
    if places.is_empty() {
        return (disp, runs, map, Vec::new());
    }
    let gap = TextRun {
        len: 0,
        font: base_font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let (nd, nr, nm, placed) =
        markdown_syntax::splice_inline_math(&disp, &runs, &map, &places, &gap);
    let inline = placed
        .into_iter()
        .zip(imgs)
        .map(|(p, (img, width, height))| InlineImage {
            display_off: p.display_off,
            len: p.len,
            img,
            width,
            height,
        })
        .collect();
    (nd, nr, nm, inline)
}

/// The WYSIWYG layout pass: shape `content` line-by-line so each logical line
/// can use its own font size (headings are larger — W2) and a standalone image
/// line can render as the image (W4). Returns, per logical line: the shaped
/// source [`WrappedLine`], its row height, and `Some(BlockImg)` when it paints
/// as an image. `md` drives the per-line size + inline styling (`None` = the
/// raw view: base size, no widgets); `diagnostics` are clipped + shifted to
/// each line. The caret's line (`caret_row`) always shows source, so an image
/// stays editable ("raw on caret"). A single line always shapes to one wrapped
/// line (incl. empty), so the counts match the logical lines and blank rows
/// stay positionable.
#[allow(clippy::too_many_arguments)]
fn shape_document(
    window: &mut Window,
    content: &str,
    base_font: &Font,
    base_color: Hsla,
    base_font_size: Pixels,
    diagnostics: &[Diagnostic],
    // Host glyph colours by source range (annotations), applied over the
    // finished runs so the run caches stay colour-free.
    text_colors: &[(Range<usize>, Hsla)],
    md: Option<&SyntaxStyle>,
    wrap_width: Option<Pixels>,
    caret_row: Option<usize>,
    block_image: Option<&BlockImageFn>,
    block_chip: Option<&BlockChipFn>,
    scale_factor: f32,
    // The selected byte range; a line it touches keeps full source (markers
    // shown), the rest hide their markers (W6, reveal-on-caret).
    selection: (usize, usize),
    // An in-progress grip resize: the dragged image is *sized* to its live width
    // here (driving its row height) so the layout reflows live, rather than the
    // image painting over a stale, saved-size row.
    resize: Option<ImageResize>,
    // An in-progress column-border drag: that column takes the live width.
    col_resize: Option<&TableColResize>,
    // The content's cached structural scans (see [`ScanData`]).
    scan: &ScanData,
    // Cross-frame shaping caches (see [`ShapeCaches`]).
    caches: &ShapeCaches,
    // The shaping window, in element-local y (quantized, one frame stale):
    // plain lines wholly outside it may skip shaping when their height is
    // cached. `None` = shape everything (first frame, raw view).
    band: Option<(Pixels, Pixels)>,
    // Collapsed headings (trimmed source lines) — their section lines fold to
    // height 0 like a folded callout's body.
    folded_headings: &std::collections::HashSet<String>,
) -> ShapedDoc {
    let mut out = ShapedDoc::default();
    let lines: Vec<&str> = content.split('\n').collect();
    // Ordered items paint their computed CommonMark position, not their
    // literal source digits (the reader renumbers the same way).
    let ordered_nums = &scan.ordered;
    // Rows a (non-empty) selection touches. A selected block reveals its raw
    // source just like the caret's block does, so what's highlighted is what a
    // copy carries — the per-line pass below already reveals inline markers on
    // selected lines; rendered BLOCKS (math, mermaid, panels, fences) need the
    // same region-wide, or a sweep silently selects invisible `$$`/fence text.
    let sel_rows: Option<Range<usize>> = (selection.0 != selection.1).then(|| {
        let (a, b) = (
            selection.0.min(selection.1).min(content.len()),
            selection.0.max(selection.1).min(content.len()),
        );
        let row_of = |off: usize| content[..off].matches('\n').count();
        row_of(a)..row_of(b) + 1
    });
    let sel_hits = |r: &Range<usize>| {
        sel_rows
            .as_ref()
            .is_some_and(|s| s.start < r.end && r.start < s.end)
    };
    // Folded callouts (`> [!NOTE]-`): each region's BODY lines collapse (the
    // marker line stays, painting the label + chevron) unless the caret is
    // inside the region — reveal-on-caret, so arrowing in unfolds for editing.
    let mut alert_folds: Vec<Range<usize>> = if md.is_some() {
        scan.alert_folds
            .iter()
            .filter(|(r, folded)| *folded && caret_row.is_none_or(|cr| !r.contains(&cr)))
            .map(|(r, _)| r.clone())
            .collect()
    } else {
        Vec::new()
    };
    // Collapsed heading sections fold the same way: the heading line stays
    // (its chevron paints there), the section's lines go to height 0, and the
    // caret STRICTLY INSIDE THE BODY reveals — the fold state itself is
    // view-local (`EditorState::folded_headings`), not in the source. Unlike a
    // callout (whose marker line reveals its body — that's how you reach the
    // fold char), the caret sitting on the heading line itself must not
    // reveal: it's exactly where the caret lands after clicking around a
    // section you then fold.
    if md.is_some() {
        alert_folds.extend(
            markdown_syntax::heading_fold_regions(content, folded_headings)
                .into_iter()
                .filter(|r| caret_row.is_none_or(|cr| cr <= r.start || cr >= r.end)),
        );
    }
    // `<!-- math:ALIGN -->` marker lines to hide (revealed only when the caret lands on them),
    // like table style markers.
    let math_marker_lines: Vec<usize> = if md.is_some() {
        scan.math.iter().filter_map(|r| r.marker_line).collect()
    } else {
        Vec::new()
    };
    // Table regions (W4c); content-fit column widths shared by each region's rows.
    let regions: &[markdown_syntax::TableRegion] = if md.is_some() { &scan.tables } else { &[] };
    // Shared component of the per-line run-cache key (see `CachedLineRuns`).
    let run_epoch = line_run_epoch(base_font, md);
    // Per-row content-key memo, valid for this (scan generation, epoch).
    let row_keys_epoch = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        scan.generation.hash(&mut h);
        run_epoch.hash(&mut h);
        Some(h.finish())
    };
    {
        let mut rk = caches.row_keys.borrow_mut();
        if rk.0 != row_keys_epoch || rk.1.len() != lines.len() {
            *rk = (row_keys_epoch, vec![None; lines.len()]);
        }
    }
    // Measured column widths, cached across frames: rebuilt only when the
    // content generation, font size/epoch, or a live column drag changes —
    // measuring shaped every cell of every table per call. Natural widths
    // deliberately do not depend on the viewport width.
    let region_cols: std::rc::Rc<Vec<Vec<Pixels>>> = {
        let natural_key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            // The scan generation covers every table byte and the `cols=`
            // attr — no need to rehash the table text per frame.
            scan.generation.hash(&mut h);
            f32::from(base_font_size).to_bits().hash(&mut h);
            run_epoch.hash(&mut h);
            if let Some(r) = col_resize {
                r.header_row.hash(&mut h);
                r.col.hash(&mut h);
                r.width.to_bits().hash(&mut h);
                for width in r.widths.iter() {
                    f32::from(*width).to_bits().hash(&mut h);
                }
            }
            h.finish()
        };
        let cols_key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            natural_key.hash(&mut h);
            wrap_width.map(f32::from).map(f32::to_bits).hash(&mut h);
            h.finish()
        };
        let hit = caches
            .region_cols
            .borrow()
            .as_ref()
            .filter(|(k, cols)| *k == cols_key && cols.len() == regions.len())
            .map(|(_, cols)| cols.clone());
        match hit {
            Some(cols) => cols,
            None => {
                let measured = caches
                    .natural_region_cols
                    .borrow()
                    .as_ref()
                    .filter(|(key, cols)| *key == natural_key && cols.len() == regions.len())
                    .map(|(_, cols)| cols.clone());
                let measured = measured.unwrap_or_else(|| {
                    let cols: std::rc::Rc<Vec<Vec<Pixels>>> = std::rc::Rc::new(
                        regions
                            .iter()
                            .map(|r| {
                                table_column_widths(
                                    &lines,
                                    r,
                                    window,
                                    base_font,
                                    base_font_size,
                                    base_color,
                                    col_resize,
                                )
                            })
                            .collect(),
                    );
                    *caches.natural_region_cols.borrow_mut() = Some((natural_key, cols.clone()));
                    cols
                });
                let cols: std::rc::Rc<Vec<Vec<Pixels>>> = std::rc::Rc::new(
                    regions
                        .iter()
                        .zip(measured.iter())
                        .map(|(r, widths)| {
                            let mut widths = widths.clone();
                            if r.col_widths_attr.is_none()
                                && col_resize
                                    .is_none_or(|resize| resize.header_row != r.lines.start)
                                && let Some(available) = wrap_width
                            {
                                fit_table_widths(
                                    &mut widths,
                                    (available - px(TABLE_GUTTER)).max(px(1.)),
                                    base_font_size,
                                );
                            }
                            widths
                        })
                        .collect(),
                );
                *caches.region_cols.borrow_mut() = Some((cols_key, cols.clone()));
                cols
            }
        }
    };
    let table_row_h = base_font_size * LINE_HEIGHT_RATIO + px(12.);
    // Fenced-code-block tracking: collect a block's line indices (so its box can
    // be sized to its widest line + the first/last line marked for rounding) and
    // the running max line width.
    // Direction of the last line that had a strong character, so blank lines
    // between paragraphs inherit it (#66).
    let mut last_strong_rtl = false;
    let mut code_block: Vec<usize> = Vec::new();
    let mut code_w = px(0.);
    let mut line_start = 0;
    let mut in_fence = false;
    // Active GitHub alert run (`> [!NOTE]` …): set by a marker line, carried
    // while blockquote lines continue, cleared by anything else — so every
    // line of the alert gets the kind's bar color.
    let mut alert_run: Option<Hsla> = None;
    let mut y_acc = px(0.);
    for (idx, &line) in lines.iter().enumerate() {
        let line_end = line_start + line.len();
        // Running top of this line — mirrors the prepaint `line_tops` walk
        // (including `line_pads`) so the band test is exact.
        if idx > 0 {
            let (tp, bp) = line_pads(out.backgrounds[idx - 1], out.tables[idx - 1].as_ref());
            y_acc += tp + bp + out.heights[idx - 1] * out.wrap_rows[idx - 1] as f32;
        }

        // A folded callout's body lines collapse (height 0); its marker line
        // falls through to normal handling, painting the label + chevron. The
        // caret entering the region reveals it (filtered above).
        if alert_folds
            .iter()
            .any(|r| r.contains(&idx) && idx != r.start)
        {
            out.push_placeholder(window, base_font_size, wrap_width, px(0.), None, None, 1);
            line_start = line_end + 1;
            continue;
        }

        // Fenced code block (W4b): a ``` line toggles the fence; the delimiter
        // lines + the lines between render as monospace code over a content-fit
        // background (delimiters dimmed). Code is literal — no inline scanning,
        // no heading size, no squiggles. Styling-mode only.
        let is_fence = md.is_some() && line.trim_start().starts_with("```");
        let is_code = md.is_some() && (in_fence || is_fence);
        // This line's alert membership: `(color, is_marker_line)`.
        let alert_line: Option<(Hsla, bool)> = if let Some(st) = md.filter(|_| !is_code) {
            match markdown_syntax::blockquote_prefix(line) {
                Some(plen) => {
                    if let Some(kind) = markdown_syntax::alert_kind(&line[plen..]) {
                        let c = st.alert_color(kind);
                        alert_run = Some(c);
                        Some((c, true))
                    } else {
                        alert_run.map(|c| (c, false))
                    }
                }
                None => {
                    alert_run = None;
                    None
                }
            }
        } else {
            alert_run = None;
            None
        };
        if is_fence {
            in_fence = !in_fence;
        }
        // A ``` fence line collapses (height 0, no text) unless the caret is in
        // its block — so a code block reads as just its boxed body (W6), with the
        // fences re-appearing while you edit inside it.
        // A fence stays hidden even while editing inside the block (the card's
        // language tag shows/edits the language) — it reveals only with the
        // caret or a selection on the fence line itself.
        let collapse_fence = is_fence && caret_row != Some(idx) && !sel_hits(&(idx..idx + 1));
        // A `<!-- table:STYLE -->` or `<!-- math:ALIGN -->` marker line collapses (hidden)
        // too, unless the caret lands on it — so the marker stays out of the way but editable.
        let collapse_marker = caret_row != Some(idx)
            && (regions.iter().any(|r| r.marker_line == Some(idx))
                || math_marker_lines.contains(&idx));

        // Leaving a code block: size the box to its widest line (+ the inset on
        // each side, like a table) and mark its last line so the box rounds + pads
        // its bottom edge. The vertical padding is grown into the painted quad, so
        // line geometry — and the caret — stay untouched.
        if !is_code && !code_block.is_empty() {
            let bw = code_w + px(2. * CODE_INSET);
            let last = *code_block.last().unwrap();
            for &bi in &code_block {
                if let Some(cb) = &mut out.backgrounds[bi] {
                    cb.width = bw;
                    cb.bottom = bi == last;
                }
            }
            code_block.clear();
            code_w = px(0.);
        }

        let fs = if is_code {
            base_font_size
        } else {
            base_font_size * md.map_or(1.0, |_| markdown_syntax::line_scale(line))
        };

        // Block widget (non-code): a standalone `![](src)` line that isn't the
        // caret's renders as a file chip (if the host classifies `src` as one,
        // e.g. a PDF) or else its decoded image, fit to width.
        // A renderable image: a standalone `![](src)` line, or the sole body of a
        // list item (`- ![](src)`). For the list case `marker_len` > 0, so the
        // image renders inset past the bullet (still painted by the gutter) and
        // sized to the remaining width — instead of the row collapsing to the
        // image (losing its bullet) or falling back to raw source.
        let img_row = (!is_code)
            .then(|| markdown_syntax::image_row(line))
            .flatten();
        let img_inset = match img_row {
            Some((_, _, marker_len)) if marker_len > 0 => {
                measure_width(window, &line[..marker_len], base_font, base_font_size)
            }
            _ => px(0.),
        };
        let widget: Option<Block> = if let Some(st) = md
            && let Some((src, w_attr, _)) = img_row
        {
            if let Some(label) = block_chip.and_then(|f| f(src)) {
                // A chip's line still edits as text: the caret's own row reveals
                // raw source instead of the chip.
                (Some(idx) != caret_row).then(|| Block::Chip {
                    src: src.into(),
                    label,
                    link: st.link,
                    bg: st.code_bg,
                    border: st.marker,
                    height: fs * LINE_HEIGHT_RATIO + px(CHIP_PAD * 2.),
                })
            } else {
                // An image renders even on the caret's own row — a Word-style
                // atomic object: the caret parks beside the picture (painted at
                // its edge) instead of revealing the markdown. Delete it via
                // Backspace/Delete or the right-click menu; WYSIWYG-off still
                // shows the raw source for hand-editing.
                block_image
                    .and_then(|f| f(src))
                    .and_then(|img| {
                        // A live grip resize sizes the image to the drag width, so
                        // its row height tracks the drag and the layout reflows.
                        let width_attr = match resize {
                            Some(r) if r.line == idx => Some(r.width),
                            _ => w_attr,
                        };
                        block_img(
                            img,
                            width_attr,
                            wrap_width.map(|w| (w - img_inset).max(px(1.))),
                            scale_factor,
                        )
                    })
                    .map(Block::Image)
            }
        } else {
            None
        };

        // Table row (W4c + cell editing): renders as a grid row; the caret on a
        // header/body row edits in place (caret rendered inside the cell). Only the
        // caret on the `|---|` separator drops the whole table to raw source (to
        // edit alignment), avoiding a broken outer box around a half-raw table.
        let table = regions
            .iter()
            .position(|r| r.lines.contains(&idx))
            .filter(|&ri| !is_code && caret_row != Some(regions[ri].lines.start + 1))
            .map(|ri| {
                let r = &regions[ri];
                TableRow {
                    cells: markdown_syntax::table_cells(line)
                        .into_iter()
                        .map(|c| SharedString::from(c.to_string()))
                        .collect(),
                    cell_ranges: markdown_syntax::table_cell_ranges(line),
                    aligns: r.aligns.clone(),
                    col_widths: region_cols[ri].clone(),
                    is_header: idx == r.lines.start,
                    is_separator: idx == r.lines.start + 1,
                    is_last: idx + 1 == r.lines.end,
                    // 0 for the first body row; None for the header/separator.
                    body_index: idx.checked_sub(r.lines.start + 2),
                    style: r.style,
                    border: md.map_or(base_color, |m| m.marker),
                    shade: md.map_or(hsla(0., 0., 0., 0.), |m| m.code_bg),
                    rtl: r.rtl,
                }
            });

        // A line shows full source while a non-empty selection touches it (so the
        // markers are visible to select) or styling is off. Otherwise its markers
        // are hidden — except, on the caret's own line, the single construct the
        // caret sits in is revealed (per-construct reveal, #5: finer than the old
        // whole-line reveal, so the rest of the line stays rendered).
        let sel_empty = selection.0 == selection.1;
        let caret_col = (sel_empty && selection.0 >= line_start && selection.0 <= line_end)
            .then(|| selection.0 - line_start);
        // An `![](src)` chip line shows full raw source while the caret is on it,
        // so editing reveals the whole `![](src)` rather than the per-construct
        // view. Image rows are exempt — they keep rendering the picture with the
        // caret parked beside it (Word-style; see the widget gate above).
        let chip_line =
            md.is_some() && img_row.is_some() && !matches!(widget, Some(Block::Image(_)));
        let sel_touches = !sel_empty && selection.0 <= line_end && selection.1 >= line_start;
        // A selection no longer reveals raw source (Cditor-style: formatting
        // markers are hidden everywhere) — selected lines keep the hidden view;
        // structural markers still come back via `reveal_inline`.
        let full_source = caret_col.is_some() && chip_line;
        // This line's diagnostics, clipped + shifted to line-local byte offsets —
        // used as spell-check squiggles whether the line shows source or hides its
        // markers.
        let line_diags: Vec<Diagnostic> = diagnostics
            .iter()
            .filter_map(|d| {
                let s = d.range.start.max(line_start);
                let e = d.range.end.min(line_end);
                (s < e).then(|| Diagnostic {
                    range: (s - line_start)..(e - line_start),
                })
            })
            .collect();
        // Gutter decoration (blockquote / list): a non-code/widget/table line with
        // a `>` or list marker. The decoration (border / bullet, marker hidden,
        // body inset) shows only while the caret is OFF the line; on the line it
        // reads as plain source with the prefix revealed (a line-level reveal — the
        // whole prefix shows wherever the caret sits, unlike inline #5).
        // `bullet_x` = measured width of the leading whitespace (where the bullet
        // paints); `text_inset` = measured width of the whole source prefix (where
        // the body sits, matching the raw line so render + edit stay in sync).
        let gutter: Option<(usize, LineMark)> = if let Some(st) = md.filter(|_| {
            // A list-item image keeps its bullet: allow the List gutter even
            // though the row also carries an (inset) image widget.
            !is_code && table.is_none() && (widget.is_none() || img_inset > px(0.))
        }) {
            if let Some(plen) = markdown_syntax::blockquote_prefix(line) {
                if let Some((kind, mlen, fold)) = markdown_syntax::alert_prefix(&line[plen..]) {
                    // The alert's marker line: hide `> [!NOTE] ` (the scan
                    // does the same) and paint a bold label in its place; a
                    // same-line body insets past the label's measured width.
                    let color = st.alert_color(kind);
                    let label_font = Font {
                        weight: FontWeight::BOLD,
                        ..base_font.clone()
                    };
                    let label_w = measure_width(window, kind.label(), &label_font, base_font_size);
                    // A foldable callout's chevron sits after the label and
                    // pushes any same-line body further right.
                    let chevron_x = px(QUOTE_INSET) + label_w + px(8.);
                    let chevron_w = if fold.is_some() {
                        measure_width(window, "▾", base_font, base_font_size) + px(8.)
                    } else {
                        px(0.)
                    };
                    Some((
                        plen + mlen,
                        LineMark::Alert {
                            bar: color,
                            label: kind.label(),
                            text_inset: chevron_x + chevron_w,
                            fold,
                            chevron_x,
                        },
                    ))
                } else {
                    // Continuation lines of an alert keep its colored bar with
                    // normal body text; plain quotes are muted throughout.
                    let (bar, text) = match alert_line {
                        Some((c, _)) => (c, None),
                        None => (st.quote, Some(st.quote)),
                    };
                    Some((plen, LineMark::Quote { bar, text }))
                }
            } else if let Some((plen, indent, checked)) = markdown_syntax::task_prefix(line) {
                // Reader-style geometry: each nesting level advances by
                // marker + gap + (spaces × 4.5), not the raw spaces' width.
                let depth = indent as f32 / TAB_INDENT as f32;
                let box_w = base_font_size * CHECKBOX_SCALE;
                let level =
                    box_w + px(LIST_TEXT_GAP) + px(LIST_LEVEL_PER_SPACE) * TAB_INDENT as f32;
                let bullet_x = level * depth;
                let text_inset = bullet_x + box_w + px(LIST_TEXT_GAP);
                Some((
                    plen,
                    LineMark::Check {
                        bullet_x,
                        text_inset,
                        checked,
                        color: st.quote,
                        accent: st.link,
                    },
                ))
            } else if let Some((plen, indent, ordered, _)) = markdown_syntax::list_prefix(line) {
                let depth = indent as f32 / TAB_INDENT as f32;
                let (num, level) = ordered_nums[idx];
                let glyph = if ordered {
                    // Word-style depth markers (1. -> a. -> i.), shared with
                    // the reader. The level is structural (from the
                    // renumbering pass), not an indent-width guess.
                    crate::syntax::ordered_marker(level, num)
                } else {
                    "\u{2022}".to_string()
                };
                let glyph_w = measure_width(window, &glyph, base_font, base_font_size);
                // The per-level step comes from a REPRESENTATIVE marker, not
                // this line's own glyph: sub-pixel width differences between
                // sibling numbers ("3." vs "4.") would nudge bullet_x apart
                // and the nesting guides would mistake siblings for ancestors
                // (drawing a guide through the numbers).
                let marker_w = if ordered {
                    measure_width(window, "9.", base_font, base_font_size)
                } else {
                    glyph_w
                };
                let step = marker_w.max(px(7.))
                    + px(LIST_TEXT_GAP)
                    + px(LIST_LEVEL_PER_SPACE) * TAB_INDENT as f32;
                let bullet_x = step * depth;
                let text_inset = bullet_x + glyph_w + px(LIST_TEXT_GAP);
                Some((
                    plen,
                    LineMark::List {
                        bullet_x,
                        text_inset,
                        ordered,
                        num,
                        level,
                        color: st.quote,
                    },
                ))
            } else {
                None
            }
        } else {
            None
        };
        let caret_here = caret_col.is_some();
        // A thematic break (`---`) renders as a full-width divider, but only while
        // the caret is off it; on the line it reads as the raw `---` (editable).
        let is_rule = !is_code
            && widget.is_none()
            && table.is_none()
            && !caret_here
            && !full_source
            && md.is_some()
            && markdown_syntax::thematic_break(line);
        // The caret sitting in the BODY keeps the list/quote mark — the line
        // used to reveal whole (raw prefix at raw indent), so every edit
        // jumped the text left. Only a caret inside the hidden prefix (left
        // arrow past the body start) reveals the raw marker for editing.
        let caret_in_prefix = gutter
            .as_ref()
            .zip(caret_col)
            .is_some_and(|(&(plen, _), col)| col < plen);
        // A selection across a gutter line keeps the mark too: the whole-line
        // raw reveal made list rows renumber (source digits) and jump to raw
        // indent mid-select. The BODY still reveals its inline markers (see
        // `reveal_inline` below) so what's highlighted is what's copied.
        let full_source = full_source && gutter.is_none();
        let reveal_inline = sel_touches;
        let mark = if is_rule {
            md.map(|st| LineMark::Rule(st.rule))
        } else {
            gutter
                .filter(|_| !caret_in_prefix && !full_source)
                .map(|(_, m)| m)
        };
        let reveal_prefix = gutter
            .filter(|_| caret_in_prefix)
            .map_or(0, |(plen, _)| plen);
        let hide_prefix = gutter
            .filter(|_| !caret_in_prefix)
            .map_or(0, |(plen, _)| plen);
        // Footnote definitions (`[^1]: …`) and raw-HTML lines render muted, the way
        // the reading view shows them — a whole-line color, no hidden markers.
        let muted_line = md
            .filter(|_| !is_code && widget.is_none() && table.is_none())
            .filter(|_| {
                markdown_syntax::footnote_def(line).is_some() || markdown_syntax::html_block(line)
            })
            .map(|st| st.quote);
        // A blockquote's body is muted; a list keeps the normal body color (only
        // its bullet is muted).
        let line_base = match mark {
            Some(LineMark::Quote { text, .. }) => text.unwrap_or(base_color),
            _ => muted_line.unwrap_or(base_color),
        };
        // Inline `$…$` formulas spliced into this line (populated by the hidden-markers branch).
        let mut line_inline_images: Vec<InlineImage> = Vec::new();
        // Set (in the markdown branch below) when this line is eligible for the
        // per-line height cache — its (row height, wrap rows) get recorded at
        // push time so a later frame can window it out without shaping.
        let mut plain_hkey: Option<u64> = None;
        let (shaped_text, runs, bg, map) = if collapse_fence || collapse_marker {
            // Hidden ``` fence line or table-style marker: nothing, zero height.
            (
                SharedString::default(),
                std::rc::Rc::new(Vec::new()),
                None,
                None,
            )
        } else if is_rule {
            // Thematic break: the divider is painted from the mark; no body text.
            (
                SharedString::default(),
                std::rc::Rc::new(Vec::new()),
                None,
                None,
            )
        } else if let Some(st) = md.filter(|_| is_code) {
            // Monospace runs; ``` delimiters dimmed.
            let base_run = |len: usize| TextRun {
                len,
                font: st.mono.clone(),
                color: if is_fence { st.marker } else { st.code },
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let runs = if line.is_empty() {
                Vec::new()
            } else {
                vec![base_run(line.len())]
            };
            // First visible code line of the block rounds the box's top corners.
            let top = code_block.is_empty();
            (
                SharedString::from(line.to_string()),
                std::rc::Rc::new(runs),
                Some(CodeBg {
                    color: st.code_bg,
                    width: px(0.), // back-patched to the block's widest line
                    top,
                    bottom: false,
                }),
                None,
            )
        } else if let Some(st) = md.filter(|_| widget.is_none() && table.is_none() && !full_source)
        {
            // Markers hidden (except the caret's construct): shape the display
            // string + keep a map back to source.
            // Cross-frame cache: most repaints (caret blink, hover, another
            // editor's edit) rebuild identical lines — reuse the built display
            // string + runs when the line and its reveal state are unchanged.
            // CONTENT part of the key, memoized per row for the current
            // (generation, epoch) — steady-state frames skip rehashing the
            // line's bytes. The caret/reveal parts vary per frame and are
            // cheap (a hash of four small values).
            let ckey = {
                let cached = caches.row_keys.borrow().1.get(idx).copied().flatten();
                match cached {
                    Some(k) => k,
                    None => {
                        use std::hash::{Hash, Hasher};
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        line.hash(&mut h);
                        line_base.a.to_bits().hash(&mut h);
                        line_base.h.to_bits().hash(&mut h);
                        line_base.s.to_bits().hash(&mut h);
                        line_base.l.to_bits().hash(&mut h);
                        for d in &line_diags {
                            d.range.start.hash(&mut h);
                            d.range.end.hash(&mut h);
                        }
                        run_epoch.hash(&mut h);
                        let k = h.finish();
                        if let Some(slot) = caches.row_keys.borrow_mut().1.get_mut(idx) {
                            *slot = Some(k);
                        }
                        k
                    }
                }
            };
            let key = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                ckey.hash(&mut h);
                caret_col.hash(&mut h);
                reveal_prefix.hash(&mut h);
                hide_prefix.hash(&mut h);
                reveal_inline.hash(&mut h);
                h.finish()
            };
            // Windowed shaping: an offscreen plain text line whose height is
            // already cached skips run-building and shaping entirely — the
            // cached (row height, wrap rows) keep the layout exact, and its
            // placeholder WrappedLine is never painted (paint culls the same
            // band). Never skipped: lines with inline math/images (height
            // depends on async raster providers, not just the text) and
            // collapsed lines (fold state isn't in the key).
            let line_wrap_plain = match mark {
                Some(m) => wrap_width.map(|w| (w - m.inset()).max(px(0.))),
                None => wrap_width,
            };
            let skip_key = (!line.contains('$')
                && !line.contains("![")
                && !collapse_fence
                && !collapse_marker)
                .then(|| {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    key.hash(&mut h);
                    f32::from(fs).to_bits().hash(&mut h);
                    line_wrap_plain
                        .map(f32::from)
                        .unwrap_or(-1.)
                        .to_bits()
                        .hash(&mut h);
                    h.finish()
                });
            if let (Some((lo, hi)), Some(hk)) = (band, skip_key) {
                let hit = caches.line_heights.borrow().get(&hk).copied();
                if let Some((lh_c, wr_c)) = hit {
                    let total = lh_c * wr_c as f32;
                    if y_acc > hi || y_acc + total < lo {
                        // The REAL mark: paint's nesting-guide bookkeeping
                        // walks offscreen ancestors' marks.
                        out.push_placeholder(
                            window,
                            base_font_size,
                            wrap_width,
                            lh_c,
                            None,
                            mark,
                            wr_c,
                        );
                        line_start = line_end + 1;
                        continue;
                    }
                }
            }
            plain_hkey = skip_key;
            let cached = caches
                .line_runs
                .borrow()
                .get(&key)
                .filter(|c| c.src == line && c.line_base == line_base)
                .map(|c| (c.disp.clone(), c.runs.clone(), c.map.clone()));
            let (disp, runs, m) = match cached {
                Some(v) => v,
                None => {
                    let (disp, runs, m) = markdown_syntax::hidden_runs(
                        line,
                        base_font,
                        line_base,
                        &line_diags,
                        caret_col,
                        reveal_prefix,
                        hide_prefix,
                        reveal_inline,
                        st,
                    );
                    let v = (
                        SharedString::from(disp),
                        std::rc::Rc::new(runs),
                        std::rc::Rc::new(m),
                    );
                    caches.line_runs.borrow_mut().insert(
                        key,
                        CachedLineRuns {
                            src: line.to_string(),
                            line_base,
                            disp: v.0.clone(),
                            runs: v.1.clone(),
                            map: v.2.clone(),
                        },
                    );
                    v
                }
            };
            // A checked task's body renders struck through + muted (the reader
            // does the same) — a whole-line restyle over the finished runs.
            let (disp, runs, m) = if matches!(mark, Some(LineMark::Check { checked: true, .. })) {
                let runs = std::rc::Rc::new(
                    runs.iter()
                        .cloned()
                        .map(|mut r| {
                            r.strikethrough = Some(gpui::StrikethroughStyle {
                                thickness: px(1.0),
                                color: None,
                            });
                            r.color = st.quote;
                            r
                        })
                        .collect(),
                );
                (disp, runs, m)
            } else {
                (disp, runs, m)
            };
            // Inline `![](src)` images: swap each ready one's glyphs for a
            // spacer to paint the raster over.
            // Span checks gate the calls, so span-less lines (the common case)
            // keep their shared payloads untouched.
            match block_image {
                Some(imgf) if !markdown_syntax::inline_image_spans(line).is_empty() => {
                    let (disp, runs, m, imgs) = shape_inline_images(
                        window,
                        line,
                        disp.to_string(),
                        runs.as_ref().clone(),
                        m.as_ref().clone(),
                        caret_col,
                        base_font,
                        fs,
                        imgf,
                    );
                    line_inline_images.extend(imgs);
                    (
                        SharedString::from(disp),
                        std::rc::Rc::new(runs),
                        None,
                        Some(std::rc::Rc::new(m)),
                    )
                }
                _ => (disp, runs, None, Some(m)),
            }
        } else {
            // Full source with diagnostics (the caret/selected line, or md off).
            (
                SharedString::from(line.to_string()),
                std::rc::Rc::new(markdown_syntax::styled_runs(
                    line,
                    base_font,
                    line_base,
                    &line_diags,
                    md,
                )),
                None,
                None,
            )
        };

        // Code lines are inset by CODE_INSET on each side; a gutter mark insets the
        // left only. Either wraps at a correspondingly narrower width. A table
        // row or widget line (image, chip) never wraps its raw source — a
        // grid row / the widget renders instead, and a wrapped hidden source
        // would multiply the row's advance by its wrap count. (Revealed-on-
        // caret lines have `table`/`widget` = None here and wrap normally.)
        let line_wrap = if table.is_some() || widget.is_some() {
            None
        } else if is_code {
            wrap_width.map(|w| (w - px(2. * CODE_INSET)).max(px(0.)))
        } else if let Some(m) = mark {
            wrap_width.map(|w| (w - m.inset()).max(px(0.)))
        } else {
            wrap_width
        };
        let line_tints: Vec<(Range<usize>, Hsla)> = text_colors
            .iter()
            .filter_map(|(r, c)| {
                let s = r.start.max(line_start);
                let e = r.end.min(line_end);
                (s < e).then(|| ((s - line_start)..(e - line_start), *c))
            })
            .collect();
        let runs = if line_tints.is_empty() || table.is_some() {
            runs
        } else {
            std::rc::Rc::new(tint_runs(
                &shaped_text,
                &runs,
                map.as_deref().map(Vec::as_slice),
                &line_tints,
            ))
        };
        let shaped = shape_runs(window, &shaped_text, fs, &runs, line_wrap);
        if let Some(wl) = shaped.into_iter().next() {
            let h = if collapse_fence || collapse_marker {
                px(0.)
            } else {
                match &table {
                    // The `|---|` separator collapses in grid mode — the old
                    // renderer doesn't show it; the first body row's top divider
                    // becomes the header/body border.
                    Some(t) if t.is_separator => px(0.),
                    // A drag-narrowed column wraps its cells — the row grows to
                    // the tallest cell's wrap rows (memoized: this shaped every
                    // cell per row per frame).
                    Some(t) => {
                        let row_key = {
                            use std::hash::{Hash, Hasher};
                            let mut h = std::collections::hash_map::DefaultHasher::new();
                            line.hash(&mut h);
                            for w in &t.col_widths {
                                f32::from(*w).to_bits().hash(&mut h);
                            }
                            t.is_header.hash(&mut h);
                            run_epoch.hash(&mut h);
                            h.finish()
                        };
                        let hit = caches.cell_rows.borrow().get(&row_key).copied();
                        let rows = match hit {
                            Some(r) => r,
                            None => {
                                let r = table_row_wrap_rows(
                                    window,
                                    &t.cells,
                                    &t.col_widths,
                                    t.is_header,
                                    base_font,
                                    base_font_size,
                                );
                                caches.cell_rows.borrow_mut().insert(row_key, r);
                                r
                            }
                        };
                        table_row_h + base_font_size * LINE_HEIGHT_RATIO * (rows - 1) as f32
                    }
                    None => match widget.as_ref() {
                        // Reserve a little space around an inline image so a list
                        // of photos doesn't stack edge-to-edge.
                        Some(Block::Image(i)) => i.height + px(IMG_ROW_PAD),
                        Some(b) => b.height(),
                        // A text row grows to fit its tallest inline `$…$` formula (a fraction
                        // is taller than the text), so the formula doesn't overlap neighbours.
                        None => {
                            let math_h = line_inline_images
                                .iter()
                                .map(|im| im.height)
                                .max()
                                .unwrap_or(px(0.));
                            let mut h =
                                (fs * LINE_HEIGHT_RATIO).max(math_h + px(INLINE_MATH_ROW_PAD));
                            // List items breathe like the reader's (LIST_ROW_GAP).
                            if md.is_some() && markdown_syntax::list_prefix(line).is_some() {
                                h += px(LIST_ROW_GAP);
                            }
                            h
                        }
                    },
                }
            };
            let line_w = wl.width();
            // #66: an RTL line can't use gpui's wrapping. gpui slices the
            // already-reordered glyph run by ascending x, so its rows come out
            // in reverse reading order, and its break candidates come from an
            // `is_word_char` that doesn't know Arabic — so words split down the
            // middle. Lay the line out in LOGICAL order instead. This has to
            // happen here, not later with the rest of the RTL geometry: our
            // break points give a different row COUNT, and the row span and
            // height are decided right here.
            // A blank line has no strong character, so it has no direction of
            // its own — it inherits the paragraph it sits in. Without this the
            // caret jumps to the LEFT margin the moment you press Enter after a
            // Persian paragraph, and a selection running through the blank line
            // puts its marker on the wrong side.
            // A line with no strong character of its own — blank, or nothing
            // but markers like `> [!NOTE]` — inherits the paragraph it sits in.
            // Otherwise a Persian callout's title sat on the opposite side from
            // its body, and pressing Enter after a Persian paragraph dropped the
            // caret on the left margin.
            //
            // ponytail: it inherits from ABOVE, so an alert whose title follows
            // English text but whose body is Persian still splits. Looking ahead
            // to the body would fix it; no one has hit that yet.
            let line_rtl = match crate::syntax::content_direction_opt(line) {
                Some(d) => {
                    last_strong_rtl = d.is_rtl();
                    d.is_rtl()
                }
                // Nothing but markers (`> [!NOTE]`, a bare `- `): its content is
                // the line BELOW, so take that one's direction — a callout's
                // title has to land on the same side as its body. Only within
                // the block: a blank line ends it, and there the line above is
                // what matters (pressing Enter after a Persian paragraph should
                // keep the caret on the right).
                None if !line.trim().is_empty() => lines
                    .get(idx + 1)
                    .filter(|next| !next.trim().is_empty())
                    .and_then(|next| crate::syntax::content_direction_opt(next))
                    .map_or(last_strong_rtl, |d| d.is_rtl()),
                None => last_strong_rtl,
            };
            // Lay out any line CONTAINING right-to-left text, not just one that
            // reads that way: an English sentence quoting a Persian name still
            // needs the mapping, or the caret misplaces inside that run. Such a
            // line stays left-aligned (`line_rtl` drives the shift, this drives
            // the mapping).
            //
            // ponytail: word breaking is space-based, so a line mixing RTL with
            // unspaced CJK would wrap poorly. Lines without any RTL keep gpui's
            // wrapping, so plain CJK is untouched.
            let has_rtl = line_rtl || crate::syntax::contains_rtl(line);
            let rtl_rows = (bg.is_none() && widget.is_none() && table.is_none() && has_rtl)
                .then(|| gpui_bidi::layout_rows(&shaped_text, &runs, line_wrap, fs, window))
                .filter(|rows| !rows.is_empty());
            let span = rtl_rows
                .as_ref()
                .map_or(wl.wrap_boundaries().len() + 1, Vec::len);
            if let Some(hk) = plain_hkey {
                caches.line_heights.borrow_mut().insert(hk, (h, span));
            }
            out.wrap_rows.push(span);
            out.rtl_rows.push(rtl_rows.map(|rows| (line_rtl, rows)));
            out.wrapped.push(wl);
            out.heights.push(h);
            out.widgets.push(widget);
            out.backgrounds.push(bg);
            out.tables.push(table);
            out.maps.push(map);
            out.marks.push(mark);
            out.inline_images.push(line_inline_images);
            // Track a (visible) code line + its width so the block's box can be
            // sized to its widest line and its last line marked.
            if is_code && !collapse_fence {
                code_block.push(out.backgrounds.len() - 1);
                code_w = code_w.max(line_w);
            }
        }
        line_start = line_end + 1; // skip the '\n'
    }
    // A code block running to the end of the document: size its box + mark its
    // last line (round the box bottom + pad).
    if !code_block.is_empty() {
        let bw = code_w + px(2. * CODE_INSET);
        let last = *code_block.last().unwrap();
        for &bi in &code_block {
            if let Some(cb) = &mut out.backgrounds[bi] {
                cb.width = bw;
                cb.bottom = bi == last;
            }
        }
    }
    // Cap the run cache: edits retire entries (each changed line re-keys), so
    // without a bound it grows one dead entry per keystroke. Clearing wholesale
    // can cause a cold frame. Plain rows rebuild in the shaping band; widgets,
    // code, and table measurements still have whole-document work.
    {
        let mut cache = caches.line_runs.borrow_mut();
        if cache.len() > lines.len() * 2 + 64 {
            cache.clear();
        }
        let mut rows = caches.cell_rows.borrow_mut();
        if rows.len() > lines.len() * 2 + 64 {
            rows.clear();
        }
        let mut lh = caches.line_heights.borrow_mut();
        if lh.len() > lines.len() * 2 + 64 {
            lh.clear();
        }
    }
    out
}

/// Paint a file chip — a rounded, bordered button with a flat document icon +
/// `label` — filling the row (sized in `shape_document` to include vertical
/// padding), its width fit to the label. Left-click opens it, right-click edits
/// (handled by the mouse handlers via `chip_rows`).
#[allow(clippy::too_many_arguments)]
fn paint_chip(
    label: &str,
    link: Hsla,
    bg: Hsla,
    border: Hsla,
    origin: Point<Pixels>,
    row_h: Pixels,
    font: &Font,
    font_size: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    let text = SharedString::from(label.to_string());
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: link,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(text, font_size, &[run], None);
    let pad_x = px(10.);
    let icon_h = font_size * 0.92;
    let icon_w = icon_h * 0.74;
    let gap = px(7.); // between the icon and the label
    let box_w = pad_x * 2. + icon_w + gap + shaped.width();
    window.paint_quad(PaintQuad {
        bounds: Bounds::new(origin, size(box_w, row_h)),
        corner_radii: Corners::all(px(6.)),
        background: bg.into(),
        border_widths: Edges::all(px(1.)),
        border_color: border,
        border_style: BorderStyle::Solid,
    });
    let ix = origin.x + pad_x;
    paint_doc_icon(
        ix,
        origin.y + (row_h - icon_h) / 2.,
        icon_w,
        icon_h,
        link,
        window,
    );
    let line_h = font_size * LINE_HEIGHT_RATIO;
    let _ = shaped.paint(
        point(ix + icon_w + gap, origin.y + (row_h - line_h) / 2.),
        line_h,
        gpui::TextAlign::Left,
        None,
        window,
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, TestAppContext, VisualTestContext};

    #[test]
    fn tint_runs_recolours_mapped_source_ranges() {
        let run = |len, color| TextRun {
            len,
            font: gpui::font("Helvetica"),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let (base, bold, tint) = (
            hsla(0., 0., 0., 1.),
            hsla(0., 0., 0.5, 1.),
            hsla(0.6, 1., 0.5, 1.),
        );
        // Source `a **Bé** c` displays as `a Bé c`; tint the source `Bé`.
        let text = "a Bé c";
        let map = [0, 1, 4, 5, 6, 9, 10, 11];
        let runs = [run(2, base), run(3, bold), run(2, base)];
        let out = tint_runs(text, &runs, Some(&map), &[(4..7, tint)]);
        let got: Vec<_> = out.iter().map(|r| (r.len, r.color)).collect();
        assert_eq!(got, vec![(2, base), (3, tint), (2, base)]);
        // Without a map, a tint splits a run at char boundaries.
        let out = tint_runs("xyz", &[run(3, base)], None, &[(1..2, tint)]);
        let got: Vec<_> = out.iter().map(|r| (r.len, r.color)).collect();
        assert_eq!(got, vec![(1, base), (1, tint), (1, base)]);
    }

    fn image_editor<'a>(
        cx: &'a mut TestAppContext,
        source: &str,
    ) -> (Entity<EditorState>, &'a mut VisualTestContext) {
        // Decode an in-memory image synchronously: no host assets, filesystem,
        // async loading, or platform-dependent natural image dimensions.
        let image = gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16"/></svg>"#.to_vec(),
        )
        .to_image_data(gpui::SvgRenderer::new(Arc::new(())))
        .unwrap();
        // Keep the window root empty so automatic refreshes cannot overwrite
        // the geometry committed by the explicit EditorElement draw below.
        let (_, cx) = cx.add_window_view(|_, _| gpui::Empty);
        cx.simulate_resize(size(px(400.), px(400.)));
        let editor = cx.update(|window, cx| cx.new(|cx| EditorState::new(window, cx)));
        editor.update(cx, |editor, cx| {
            editor.set_text(source, cx);
            editor.set_markdown_style(markdown_syntax::search_style(), cx);
            editor.set_block_image_provider(move |_| Some(image.clone()));
        });
        (editor, cx)
    }

    fn draw_images(
        editor: &Entity<EditorState>,
        cx: &mut VisualTestContext,
        y: Pixels,
        expected_rows: &[usize],
    ) -> PrepaintState {
        // An offset root models scrolling while retaining the real measured
        // layout, prepaint hitbox insertion, and paint/geometry publication.
        let (_, prepaint) = cx.draw(
            point(px(37.), y),
            size(
                AvailableSpace::Definite(px(320.)),
                AvailableSpace::MinContent,
            ),
            |_, _| EditorElement {
                editor: editor.clone(),
            },
        );
        assert_eq!(
            prepaint
                .image_grips
                .iter()
                .map(|(row, _)| *row)
                .collect::<Vec<_>>(),
            expected_rows,
            "prepaint grip rows at scroll origin {y:?}",
        );
        editor.read_with(cx, |editor, _| {
            assert_eq!(
                editor
                    .image_rects
                    .iter()
                    .map(|(row, _)| *row)
                    .collect::<Vec<_>>(),
                expected_rows,
                "painted image rows at scroll origin {y:?}",
            );
            for &(row, rect) in &editor.image_rects {
                let (_, hitbox) = prepaint
                    .image_grips
                    .iter()
                    .find(|(line, _)| *line == row)
                    .unwrap();
                assert_eq!(
                    hitbox.bounds,
                    EditorState::image_grip(rect),
                    "grip for logical row {row}"
                );
                assert_eq!(rect.origin.x, px(37.) + editor.line_insets[row]);
                assert_eq!(
                    rect.origin.y,
                    y + editor.line_tops[row] + px(IMG_ROW_PAD / 2.)
                );
            }
        });
        prepaint
    }

    #[gpui::test]
    fn scrolled_image_grips_match_painted_logical_rows(cx: &mut TestAppContext) {
        // Two early images, a raw math block, and non-adjacent later images
        // (one in a list). Wrapped text separates the viewport bands.
        let paragraph = "wrapped text ".repeat(80);
        let source = format!(
            "![early](image){{width=120}}\n{paragraph}\n![early2](image){{width=80}}\n$$\nx\n$$\n- ![later](image){{width=100}}\ntext\n![last](image){{width=60}}\n{paragraph}\n![below](image){{width=40}}"
        );
        let (editor, cx) = image_editor(cx, &source);
        draw_images(&editor, cx, px(0.), &[0]);
        let scroll_y = editor.read_with(cx, |editor, _| {
            assert!(editor.wrap_rows[1] > 1);
            // The second early image ends one pixel beyond the upper culling
            // margin. Later grips must retain logical rows 6/8, not indices 0/1.
            -(editor.line_tops[2] + editor.line_heights[2]) - px(65.)
        });
        draw_images(&editor, cx, scroll_y, &[6, 8]);
        editor.read_with(cx, |editor, _| {
            assert!(editor.line_insets[6] > px(0.));
            assert_eq!(editor.image_rects[0].1.size, size(px(100.), px(100.)));
            assert_eq!(editor.image_rects[1].1.size, size(px(60.), px(60.)));
        });

        // The live resize is keyed by the same logical row and reflows later
        // images. Their hitboxes must follow the newly painted corners.
        editor.update(cx, |editor, _| {
            editor.image_resize = Some(ImageResize {
                line: 6,
                start_width: 100.,
                start_x: px(0.),
                width: 140.,
            });
        });
        draw_images(&editor, cx, scroll_y, &[6, 8]);
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.image_rects[0].1.size, size(px(140.), px(140.)));
            assert_eq!(editor.image_rects[1].1.size, size(px(60.), px(60.)));
            assert_eq!(editor.text(), source);
        });
        editor.update(cx, |editor, _| editor.image_resize = None);
        draw_images(&editor, cx, px(0.), &[0]);
    }

    #[gpui::test]
    fn image_grips_and_paint_agree_at_viewport_margin_boundaries(cx: &mut TestAppContext) {
        let (editor, cx) = image_editor(cx, &"![](image){width=100}\n".repeat(3));
        draw_images(&editor, cx, px(0.), &[0, 1, 2]);
        let (top, height) = editor.read_with(cx, |editor, _| {
            (editor.line_tops[1], editor.line_heights[1])
        });
        let viewport_h = cx.update(|window, _| window.viewport_size().height);
        // Exactly touching either 64px margin remains visible; moving one
        // pixel farther culls that row in BOTH prepaint and paint.
        let above = -top - height - px(64.);
        draw_images(&editor, cx, above, &[1, 2]);
        draw_images(&editor, cx, above - px(1.), &[2]);
        let below = viewport_h + px(64.) - top;
        draw_images(&editor, cx, below, &[0, 1]);
        draw_images(&editor, cx, below + px(1.), &[0]);
    }
}

/// Where a flash peaks, as a fraction of its duration.
const FLASH_PEAK: f32 = 0.35;

/// Ease-in-out (smoothstep): no jolt at either end.
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    let t = t.clamp(0., 1.);
    gpui::Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
    .into()
}

/// An annotation's (fill, border, mark (text, colour, rise px), still
/// animating) at `now` under its latest flash.
fn flashed(
    flashes: &[(std::time::Instant, AnnotationFlash)],
    id: u64,
    fill: Hsla,
    border: Hsla,
    now: std::time::Instant,
) -> (Hsla, Hsla, Option<(SharedString, Hsla, f32)>, bool) {
    let Some((start, flash)) = flashes.iter().rev().find(|(_, f)| f.ids.contains(&id)) else {
        return (fill, border, None, false);
    };
    let elapsed = now.saturating_duration_since(*start).as_secs_f32();
    let total = flash.duration.as_secs_f32().max(0.001);
    // The mark appears as the colour peaks: it rises 3 px while fading in,
    // holds, then fades out.
    let mark = flash.mark.as_ref().and_then(|(text, color)| {
        let t = elapsed - total * FLASH_PEAK;
        let (fade_in, hold, fade_out) = (0.25, 0.7, 0.35);
        let (a, rise) = if t < 0. {
            return None;
        } else if t < fade_in {
            let x = smooth(t / fade_in);
            (x, 3. * (1. - x))
        } else if t < fade_in + hold {
            (1., 0.)
        } else if t < fade_in + hold + fade_out {
            (1. - smooth((t - fade_in - hold) / fade_out), 0.)
        } else {
            return None;
        };
        Some((
            text.clone(),
            Hsla {
                a: color.a * a,
                ..*color
            },
            rise,
        ))
    });
    let t = elapsed / total;
    if t >= 1. {
        let active = mark.is_some();
        return (fill, border, mark, active);
    }
    let (f, b) = if t < FLASH_PEAK {
        let x = smooth(t / FLASH_PEAK);
        (
            mix(flash.from.0, flash.peak.0, x),
            mix(flash.from.1, flash.peak.1, x),
        )
    } else {
        let x = smooth((t - FLASH_PEAK) / (1. - FLASH_PEAK));
        (mix(flash.peak.0, fill, x), mix(flash.peak.1, border, x))
    };
    (f, b, mark, true)
}

#[cfg(test)]
mod flash_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn flash_eases_through_peak_to_rest_and_check_passes() {
        let red: Hsla = rgba(0xff0000ff).into();
        let blue: Hsla = rgba(0x0000ffff).into();
        let green: Hsla = rgba(0x00ff00ff).into();
        let start = Instant::now();
        let flash = AnnotationFlash {
            ids: [1].into(),
            from: (red, red),
            peak: (blue, blue),
            duration: Duration::from_millis(1000),
            mark: Some(("✓".into(), green)),
        };
        let flashes = [(start, flash)];
        let at = |ms| flashed(&flashes, 1, green, green, start + Duration::from_millis(ms));
        assert_eq!(at(0).0, red, "starts from the proposal colour");
        assert_eq!(at(0).2, None, "✓ waits for the peak");
        assert_eq!(at(350).0, blue, "peaks at FLASH_PEAK");
        assert!(at(500).2.is_some_and(|(_, c, _)| c.a > 0.));
        let rest = at(1000);
        assert_eq!(rest.0, green, "settles on the resting colour");
        assert!(rest.3, "the ✓ still animates after the colour settles");
        assert!(!at(3000).3, "nothing paints once finished");
        assert!(
            !flashed(&flashes, 2, green, green, start).3,
            "other ids are untouched"
        );
    }
}

/// One rolling span, laid out in prepaint.
pub(crate) struct Roll {
    /// The current text's glyph box (one row).
    glyphs: Bounds<Pixels>,
    /// The chip painted under it this frame, if any.
    chip: Option<PaintQuad>,
    surface: Hsla,
    /// (shaped text at its alpha, progress 0..1) while visible.
    previous: Option<(gpui::ShapedLine, f32)>,
    current: Option<(gpui::ShapedLine, f32)>,
}

/// Mask the real text with the page and its chip, then paint the previous
/// words rising out and the current words rising in, clipped to the chip. At
/// the end the current words sit exactly on the real ones: a seamless hand-off.
fn paint_roll(roll: Roll, window: &mut Window, cx: &mut App) {
    let Roll {
        glyphs,
        chip,
        surface,
        previous,
        current,
    } = roll;
    let lh = glyphs.size.height;
    let clip = chip.as_ref().map_or(glyphs, |c| c.bounds);
    window.paint_quad(fill(
        Bounds::from_corners(
            point(glyphs.left() - px(1.), clip.top() + px(1.)),
            point(glyphs.right() + px(1.), clip.bottom() - px(1.)),
        ),
        surface,
    ));
    if let Some(chip) = chip {
        window.paint_quad(chip);
    }
    let rise = px(6.);
    window.with_content_mask(Some(gpui::ContentMask { bounds: clip }), |window| {
        if let Some((line, out)) = previous {
            let _ = line.paint(
                point(glyphs.left(), glyphs.top() - rise * out),
                lh,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
        }
        if let Some((line, into)) = current {
            let _ = line.paint(
                point(glyphs.left(), glyphs.top() + rise * (1. - into)),
                lh,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
        }
    });
}
