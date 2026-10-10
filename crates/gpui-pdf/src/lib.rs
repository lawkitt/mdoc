//! **PDF viewing for [GPUI](https://www.gpui.rs/)** — a page-virtualized viewer
//! built on the pure-Rust [`hayro`](https://crates.io/crates/hayro) rasterizer (no
//! native libraries, no system-font dependency).
//!
//! Two layers, use whichever fits:
//!
//! - **Low-level primitives** (host-agnostic, pure): [`parse`] a PDF once, read page
//!   sizes with [`page_dims`], rasterize a page to a [`gpui::RenderImage`] with
//!   [`render_page`], and compute the on-screen page range with [`keep_window`].
//!   Build your own viewer on these.
//! - **A ready component**: [`PdfView`] — a self-contained gpui entity that owns its
//!   document, scroll position, off-thread rendering, and viewport eviction, so an
//!   800-page file stays as light as a one-pager. It also has built-in **zoom**,
//!   **page navigation** (including a jump-to-page input), and **DPI-aware**
//!   rendering with a host-settable **quality** multiplier. Construct it inside
//!   `cx.new` and render the `Entity<PdfView>` like any child view.
//!
//! ```no_run
//! # use std::rc::Rc;
//! # use std::path::PathBuf;
//! # use gpui::AppContext;
//! # use gpui_pdf::{PdfView, PdfStyle};
//! # fn demo(cx: &mut gpui::App, path: PathBuf) {
//! let view = cx.new(|cx| {
//!     PdfView::new(path, Rc::new(PdfStyle::default), Rc::new(|| 1.0), cx)
//! });
//! // then `view.clone()` into your element tree; call `view.update(cx, |v, cx|
//! // v.release(window, cx))` before dropping it (e.g. when its tab closes).
//! # let _ = view;
//! # }
//! ```

pub mod scrollbar;
use gpui::prelude::FluentBuilder;

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyView, App, AppContext, Context, EventEmitter, FocusHandle, Hsla, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Render, RenderImage,
    ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Window, div, hsla, img, point,
    px,
};
// Only the forms layer maps field rects to window space.
use gpui::{Bounds, Pixels};
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::{DecryptionError, LoadPdfError, Pdf};
use image::{Frame, RgbaImage};

use gpui::deferred;

mod text;
pub use text::extract_page_text;
pub(crate) use text::{NormRect, PageText};

/// PDF outline / table-of-contents + link extraction (always available — no deps).
mod outline;
pub use outline::{LinkTarget, page_links};
pub(crate) use outline::{OutlineItem, PdfLink};

mod forms;
pub(crate) use forms::{FormField, form_fields, normalize_form_appearances};

// ─────────────────────────────── Low-level primitives ───────────────────────────────

/// A parsed PDF. Parse once (not per page) — re-parsing a large file for every page
/// is slow and churns the allocator. [`hayro`]'s `Pdf` is `Send + Sync` (std
/// feature) and caches pages internally, so it's shared via `Arc` across the
/// background render tasks.
pub type Document = Pdf;

/// Why loading a PDF failed.
#[derive(Debug)]
pub enum LoadError {
    /// The PDF is encrypted and the supplied password was missing or wrong — the
    /// caller can prompt for a password and retry via [`parse_with_password`].
    Locked,
    /// Any other failure (malformed file, unsupported encryption, …).
    Other(String),
}

/// Parse a PDF's bytes into a reusable [`Document`]. The `Document` owns the bytes,
/// so the caller can drop its own copy. Returns [`LoadError::Locked`] for a
/// password-protected file (retry with [`parse_with_password`]).
pub fn parse(bytes: Arc<Vec<u8>>) -> Result<Arc<Document>, LoadError> {
    parse_with_password(bytes, "")
}

/// Like [`parse`], but supplies a decryption `password` for an encrypted PDF.
/// Returns [`LoadError::Locked`] if the file is password-protected and `password`
/// is missing or incorrect.
pub(crate) fn parse_with_password(
    bytes: Arc<Vec<u8>>,
    password: &str,
) -> Result<Arc<Document>, LoadError> {
    // Form display correctness: give every form widget a directly-renderable
    // appearance stream before hayro sees the bytes (see `forms`). A no-op
    // (or an encrypted/unparseable file) keeps the original bytes.
    let bytes = normalize_form_appearances(&bytes)
        .map(Arc::new)
        .unwrap_or(bytes);
    match Pdf::new_with_password(bytes, password) {
        Ok(pdf) => Ok(Arc::new(pdf)),
        Err(LoadPdfError::Decryption(DecryptionError::PasswordProtected)) => Err(LoadError::Locked),
        Err(e) => Err(LoadError::Other(format!("parse PDF: {e:?}"))),
    }
}

/// Everything [`PdfView`] needs to display a document: the parsed doc, per-page
/// sizes, the outline, per-page links — and, under `forms`, the form fields.
struct Prepared {
    doc: Arc<Document>,
    dims: Vec<(f32, f32)>,
    toc: Vec<OutlineItem>,
    links: Vec<Vec<PdfLink>>,
    /// Enumerated from the ORIGINAL bytes (not the display-normalized ones), so
    /// values/rects match what [`set_form_value`] will rewrite on disk.
    fields: Vec<FormField>,
}

/// Parse `bytes` with `password` and measure it — the off-thread half of a load,
/// shared by the initial open, a password [`PdfView::unlock`], and
/// [`PdfView::replace_bytes`].
fn prepare(bytes: Arc<Vec<u8>>, password: &str) -> Result<Prepared, LoadError> {
    let fields = form_fields(&bytes);
    let doc = parse_with_password(bytes, password)?;
    let dims = page_dims(&doc);
    let toc = crate::outline::outline(&doc);
    let links = crate::outline::page_links(&doc);
    Ok(Prepared {
        doc,
        dims,
        toc,
        links,
        fields,
    })
}

/// Each page's `(width, height)` in points — cheap to read (no rasterization), so a
/// viewer can lay out correctly-sized page slots before any page renders.
pub fn page_dims(doc: &Document) -> Vec<(f32, f32)> {
    doc.pages().iter().map(|p| p.render_dimensions()).collect()
}

/// How rasterized pages are painted: the original paper, or *themed* — paper and
/// ink remapped to a dark palette (ADR 0035).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageTone {
    #[default]
    Original,
    /// White maps to `paper`, black to `ink`; greys interpolate by luma and
    /// coloured pixels keep their hue (Zathura `recolor` + `keephue` model).
    Themed { paper: [u8; 3], ink: [u8; 3] },
}

impl PageTone {
    pub fn themed(paper: Hsla, ink: Hsla) -> Self {
        let rgb = |c: Hsla| {
            let c = gpui::Rgba::from(c);
            [c.r, c.g, c.b].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        };
        Self::Themed {
            paper: rgb(paper),
            ink: rgb(ink),
        }
    }
}

/// Themed remap: luma `y` blends ink (y = 0) to paper (y = 255) and each pixel keeps
/// its chroma offset from its own luma, so greys land exactly on the blend and
/// coloured ink keeps its hue. Pure integer arithmetic with no table lookups, so the
/// composite loop stays vectorizable.
struct ToneMap {
    ink: [i32; 3],
    /// `paper - ink` per channel.
    span: [i32; 3],
}

impl ToneMap {
    fn new(paper: [u8; 3], ink: [u8; 3]) -> Self {
        let ink = ink.map(i32::from);
        Self {
            ink,
            span: [0, 1, 2].map(|c| i32::from(paper[c]) - ink[c]),
        }
    }

    #[inline(always)]
    fn apply(&self, [r, g, b]: [u8; 3]) -> [u8; 3] {
        // BT.601 luma in 8.8 fixed point; weights sum to 256 so a grey maps to itself.
        let y = (77 * i32::from(r) + 150 * i32::from(g) + 29 * i32::from(b)) >> 8;
        // `span * y / 255`, rounded: × 257 / 65536 is exact at y = 0 and y = 255.
        let tone = |c: usize, v: u8| {
            let base = self.ink[c] + ((self.span[c] * y * 257 + 32768) >> 16);
            (base + i32::from(v) - y).clamp(0, 255) as u8
        };
        [tone(0, r), tone(1, g), tone(2, b)]
    }
}

/// Composite premultiplied RGBA `src` over white into BGRA `out` (gpui's
/// RenderImage is BGRA), passing each opaque colour through `tone`. Generic so the
/// original-paper path compiles to the plain composite loop.
#[inline(always)]
fn composite(src: &[u8], out: &mut [u8], tone: impl Fn([u8; 3]) -> [u8; 3]) {
    for (out, p) in out
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(src.as_chunks::<4>().0)
    {
        // out = src + 255-a; src ≤ a so no overflow.
        let add = 255 - p[3];
        let [r, g, b] = tone([
            p[0].saturating_add(add),
            p[1].saturating_add(add),
            p[2].saturating_add(add),
        ]);
        *out = [b, g, r, 255];
    }
}

/// Rasterize a single page (0-based) of an already-parsed [`Document`] at `scale`
/// (PDF point-size × this) to a BGRA `RenderImage` composited onto white, then
/// painted in `tone`. Higher scale = sharper but more memory; [`PdfView`] picks
/// `scale` from the display's pixel ratio, zoom, and quality so pages are crisp
/// without wasting memory.
pub fn render_page(
    doc: &Document,
    idx: usize,
    scale: f32,
    tone: PageTone,
) -> Result<Arc<RenderImage>, String> {
    let pixmaps = hayro::render_pdf(doc, scale, InterpreterSettings::default(), Some(idx..=idx))
        .ok_or_else(|| format!("render page {idx}"))?;
    let pixmap = pixmaps
        .into_iter()
        .next()
        .ok_or_else(|| format!("no page {idx}"))?;

    let (w, h) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    let src = pixmap.data_as_u8_slice(); // premultiplied RGBA8, row-major
    let mut bgra = vec![0u8; src.len()];
    match tone {
        PageTone::Original => composite(src, &mut bgra, |c| c),
        PageTone::Themed { paper, ink } => {
            let map = ToneMap::new(paper, ink);
            composite(src, &mut bgra, |c| map.apply(c));
        }
    }
    let buf = RgbaImage::from_raw(w, h, bgra).ok_or_else(|| "bad pixel buffer".to_string())?;
    Ok(Arc::new(RenderImage::new(vec![Frame::new(buf)])))
}

/// True if a link/image `src` points at a PDF (by extension, case-insensitive).
pub fn is_pdf(src: &str) -> bool {
    let src = src.trim_end();
    // A URL query (`report.pdf?v=2`) doesn't change what the file is; `#`
    // stays — it's the page-jump anchor (`file.pdf#p3`), stripped by callers.
    let path = src.split('?').next().unwrap_or(src);
    path.to_lowercase().ends_with(".pdf")
}

// ─────────────────────────────── Viewport virtualization ───────────────────────────────

/// Base on-screen page width (points) at zoom 1.0; pages keep their aspect ratio.
pub const PAGE_WIDTH: f32 = 820.0;
/// Vertical gap between page slots (matches the column `gap`).
const PAGE_GAP: f32 = 10.0;
/// Top/bottom padding of the page column (matches the column `py`).
const PAGE_PAD_Y: f32 = 16.0;
/// Width of the custom vertical scrollbar gutter (px). The thumb floats over the
/// right edge of the viewport (overlay-style), so it doesn't shift the page column.
const SCROLLBAR_W: f32 = 12.0;
/// Extra pages to keep rasterized above and below the visible range. A single-page
/// margin keeps the first visible page responsive on long documents while still
/// covering the usual one-page scroll gesture. Raster work is also capped below.
const MARGIN: usize = 1;
/// Keep raster work bounded so page rendering cannot starve editor input and
/// scrolling on a document with many pages.
const MAX_ACTIVE_RENDERS: usize = 2;

/// A page's on-screen height for a given column width, preserving aspect ratio.
fn display_height((w, h): (f32, f32), page_width: f32) -> f32 {
    if w > 0.0 {
        page_width * (h / w)
    } else {
        page_width
    }
}

/// The inclusive page-index range `(start, end)` to keep rasterized for the given
/// scroll position: the pages intersecting the viewport, padded by [`MARGIN`]. Pure
/// (mirrors [`PdfView`]'s slot layout) so it's unit-testable. `page_width` is the
/// on-screen column width (base × zoom); `scroll_y` is how far the content is
/// scrolled down (px ≥ 0); `viewport_h` is the visible height (px).
pub(crate) fn keep_window(
    dims: &[(f32, f32)],
    page_width: f32,
    scroll_y: f32,
    viewport_h: f32,
) -> (usize, usize) {
    if dims.is_empty() {
        return (0, 0);
    }
    // Before the first paint the viewport height is unknown (0); assume a page or so
    // high so the first pages still render.
    let vh = if viewport_h > 1.0 { viewport_h } else { 900.0 };
    let top = scroll_y.max(0.0);
    let bottom = top + vh;

    let mut y = PAGE_PAD_Y;
    let mut first: Option<usize> = None;
    let mut last = 0usize;
    for (i, dim) in dims.iter().enumerate() {
        let page_top = y;
        let page_bottom = y + display_height(*dim, page_width);
        if page_bottom > top && page_top < bottom {
            first.get_or_insert(i);
            last = i;
        }
        y = page_bottom + PAGE_GAP;
    }
    let first = first.unwrap_or(0);
    let start = first.saturating_sub(MARGIN);
    let end = (last + MARGIN).min(dims.len() - 1);
    (start, end)
}

/// The index of the topmost page intersecting the viewport top — the "current" page
/// for a page counter. Pure; mirrors the slot layout.
fn current_page(dims: &[(f32, f32)], page_width: f32, scroll_y: f32) -> usize {
    let top = scroll_y.max(0.0);
    let mut y = PAGE_PAD_Y;
    for (i, dim) in dims.iter().enumerate() {
        let page_bottom = y + display_height(*dim, page_width);
        if page_bottom > top {
            return i;
        }
        y = page_bottom + PAGE_GAP;
    }
    dims.len().saturating_sub(1)
}

/// The y offset (px) of page `index`'s top in the laid-out column. Pure.
fn page_top_y(dims: &[(f32, f32)], page_width: f32, index: usize) -> f32 {
    let mut y = PAGE_PAD_Y;
    for dim in dims.iter().take(index) {
        y += display_height(*dim, page_width) + PAGE_GAP;
    }
    y
}

/// The rasterization scale for one page: enough pixels to fill its on-screen size at
/// the display's pixel ratio × the host's quality multiplier, clamped so high
/// zoom/DPI can't mint runaway bitmaps. Pure. `page_width` is the on-screen column
/// width (base × zoom); `page_pt_width` is the page's width in PDF points.
fn render_scale(page_width: f32, scale_factor: f32, quality: f32, page_pt_width: f32) -> f32 {
    if page_pt_width > 0.0 {
        (page_width * scale_factor * quality / page_pt_width).clamp(0.5, MAX_RENDER_SCALE)
    } else {
        1.5
    }
}

// ─────────────────────────────── Component: PdfView ───────────────────────────────

/// The render tone for the effective themed colors (`None` = original paper).
fn page_tone(themed: Option<&ThemedPages>) -> PageTone {
    themed.map_or(PageTone::Original, |t| PageTone::themed(t.paper, t.ink))
}

/// Automatic zoom-to-fit modes — see [`PdfView::fit_width`] / [`PdfView::fit_page`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FitMode {
    /// The page column fills the viewport width.
    Width,
    /// The whole current page fits inside the viewport.
    Page,
}

/// Smallest / largest zoom the viewer allows.
const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 3.0;
/// Multiplicative step for one zoom-in / zoom-out.
const ZOOM_STEP: f32 = 1.25;
/// Smallest / largest render-quality multiplier the viewer honors.
const MIN_QUALITY: f32 = 0.25;
const MAX_QUALITY: f32 = 3.0;
/// Cap on the rasterization scale, so high zoom on a Retina display can't mint
/// enormous page bitmaps. Above this, pages soften slightly instead of ballooning.
const MAX_RENDER_SCALE: f32 = 4.0;

/// A page's render state. The bitmap is kept while a re-render (zoom / quality
/// change) is in flight, so the page never blanks — gpui rescales the old bitmap
/// until the crisp one lands.
#[derive(Default, Clone)]
struct Slot {
    /// Last rasterized bitmap (may be a stale scale during a re-render); shown scaled
    /// meanwhile. `None` when never rendered or evicted.
    image: Option<Arc<RenderImage>>,
    /// Generation `image` was rendered at; if it differs from the view's generation,
    /// the bitmap is stale and the page is re-rendered (while still shown).
    image_gen: u64,
    /// A background rasterization is in flight (don't spawn another).
    loading: Option<u64>,
}

/// Colors for the [`PdfView`] chrome. Map your theme onto this; [`PdfStyle::default`]
/// is a neutral dark palette.
#[derive(Clone, Copy)]
pub struct PdfStyle {
    /// Viewer background.
    pub bg: Hsla,
    /// Page-slot border and the header divider.
    pub border: Hsla,
    /// Background of an unrendered page slot (placeholder) and control hover.
    pub placeholder_bg: Hsla,
    /// "Page N" / "Loading…" placeholder text.
    pub placeholder_fg: Hsla,
    /// Header filename and control text.
    pub header_fg: Hsla,
    /// Header "· N pages" / page counter text.
    pub header_muted: Hsla,
    /// Overlays drawn over original (white) paper.
    pub overlays: PageOverlays,
    /// Dark page colors. `Some` paints pages themed by default and shows a header
    /// toggle back to the original paper; `None` always shows the original paper.
    pub themed_pages: Option<ThemedPages>,
}

impl Default for PdfStyle {
    fn default() -> Self {
        Self {
            bg: hsla(0.0, 0.0, 0.12, 1.0),
            border: hsla(0.0, 0.0, 1.0, 0.10),
            placeholder_bg: hsla(0.0, 0.0, 1.0, 0.04),
            placeholder_fg: hsla(0.0, 0.0, 1.0, 0.40),
            header_fg: hsla(0.0, 0.0, 1.0, 0.70),
            header_muted: hsla(0.0, 0.0, 1.0, 0.40),
            overlays: PageOverlays::default(),
            themed_pages: None,
        }
    }
}

/// Colors of the interactive layers over a page: find matches and link / form
/// hover. Tuned per paper tone, since faint fills vanish on dark pages.
#[derive(Clone, Copy, PartialEq)]
pub struct PageOverlays {
    pub search: Hsla,
    pub search_current: Hsla,
    pub search_border: Hsla,
    pub link_hover: Hsla,
    pub field_hover: Hsla,
}

/// Defaults tuned for white paper.
impl Default for PageOverlays {
    fn default() -> Self {
        Self {
            search: hsla(0.09, 0.95, 0.5, 0.3),
            search_current: hsla(0.09, 0.95, 0.5, 0.55),
            search_border: hsla(0.09, 0.95, 0.4, 0.95),
            link_hover: hsla(0.58, 0.9, 0.55, 0.12),
            field_hover: hsla(0.25, 0.8, 0.5, 0.12),
        }
    }
}

/// Themed page colors: what white `paper` and black `ink` become (see
/// [`PageTone::Themed`]), and the overlays tuned for that paper.
#[derive(Clone, Copy, PartialEq)]
pub struct ThemedPages {
    pub paper: Hsla,
    pub ink: Hsla,
    pub overlays: PageOverlays,
}

/// Supplies the current [`PdfStyle`] at paint time. Because [`PdfView`] is a
/// persistent entity (not rebuilt by its parent each frame), it reads its colors
/// through this closure — returning fresh colors each call lets the viewer follow
/// live theme changes (and differ per window) without the host pushing updates.
pub type PdfStyleFn = Rc<dyn Fn() -> PdfStyle>;

/// Supplies the current render-quality multiplier at paint time (1.0 = native DPI;
/// < 1 is faster and softer, > 1 supersamples). Read like [`PdfStyleFn`], so a host
/// setting change (e.g. a Settings slider) re-renders all open viewers — in every
/// window — automatically. Clamped to a sane range internally.
pub type PdfQualityFn = Rc<dyn Fn() -> f32>;

/// Invoked from the source-name row's close control. Set via [`PdfView::set_on_close`].
pub type CloseFn = Rc<dyn Fn(&mut Window, &mut gpui::App)>;

/// Cache state for a page's extracted text layer.
enum TextSlot {
    Loading,
    Ready(PageText),
    Failed,
}

/// One find-in-PDF match: the page it's on and one normalized rect per line it spans.
/// (`search` feature.)
struct SearchMatch {
    page: usize,
    rects: Vec<NormRect>,
}

/// A page-virtualized PDF viewer: a scrollable column of page slots, each sized from
/// the PDF's page dimensions up front (so the scrollbar is correct for the whole
/// document) but only rasterized while near the viewport. Pages scrolled away are
/// freed — CPU pixel buffer *and* GPU atlas texture — so memory is bounded by what's
/// on screen rather than the page count.
///
/// Built-in controls: a header with page navigation (‹ / ›, a click-to-edit page
/// counter you can type a number into) and zoom (−, a percentage that resets to
/// 100%, +); the keyboard shortcuts PageUp / PageDown / Home / End and ⌘=/⌘-/⌘0
/// (when the viewer is focused — click it first); DPI-aware rasterization scaled by
/// the host's [quality](PdfQualityFn) multiplier; and no blanking on zoom/quality
/// changes (the old bitmap is shown, rescaled, until the crisp one lands).
///
/// Emitted when the view's lock state changes — encrypted-and-locked, unlocked, or a
/// failed unlock — so a host rendering a password prompt around the viewer knows to
/// re-render. (Fired only on these transitions, not on every redraw.)
pub enum PdfEvent {
    LockChanged,
    /// The file couldn't be read or parsed (see [`PdfView::load_error`]) —
    /// terminal for this view; the viewer shows the message in place of the
    /// loading placeholder. Also fired when a retry-unlock fails with a
    /// non-password error (e.g. an unsupported encryption handler discovered
    /// at unlock time), so a password prompt knows to stand down.
    LoadFailed,
    /// A form-field widget was clicked (`forms` feature): the field's
    /// description and its current window-space bounds — everything a host
    /// needs to toggle a checkbox or seat a text input right over the widget,
    /// write through [`set_form_value`], persist, and call
    /// [`PdfView::replace_bytes`].
    FieldClicked {
        field: FormField,
        bounds: Bounds<Pixels>,
    },
}

/// Construct with [`PdfView::new`] inside `cx.new`; it loads and measures the file
/// off-thread. Render the resulting `Entity<PdfView>` like any child view. Call
/// [`release`](PdfView::release) before dropping it (e.g. when its tab closes) to
/// free the atlas textures gpui won't free on plain drop.
pub struct PdfView {
    display_name: Option<String>,
    #[cfg(test)]
    render_requests: usize,
    path: PathBuf,
    style: PdfStyleFn,
    quality: PdfQualityFn,
    loading_indicator: Option<Rc<dyn Fn(SharedString) -> gpui::AnyElement>>,
    /// The parsed PDF (shared with the background render tasks); `None` until the
    /// off-thread load finishes.
    pdf: Option<Arc<Document>>,
    /// The raw file bytes, kept so an encrypted PDF can be retried with a password
    /// without re-reading the file. `None` until the load reads them.
    bytes: Option<Arc<Vec<u8>>>,
    /// The PDF is encrypted — the host shows a notice instead of the viewer.
    locked: bool,
    /// A terminal read/parse failure — the viewer renders this message
    /// instead of sitting on "Loading PDF…" forever.
    load_error: Option<SharedString>,
    on_close: Option<CloseFn>,
    /// `(width, height)` in points per page — drives page-slot sizing.
    dims: Vec<(f32, f32)>,
    /// Per-page render state; only pages near the viewport hold a bitmap.
    pages: Vec<Slot>,
    active_renders: usize,
    released: bool,
    scroll: ScrollHandle,
    /// Active zoom-to-fit mode: re-fits on every viewport resize until a
    /// manual zoom clears it.
    fit: Option<FitMode>,
    /// Viewport size the fit was last computed for (so a resize re-fits once).
    fit_viewport: (f32, f32),
    awaiting_fit_layout: bool,
    /// On-screen zoom factor (1.0 = base width). Affects layout and render scale.
    zoom: f32,
    /// The quality multiplier the pages were last rendered at; compared against the
    /// `quality` source each frame to detect a host setting change.
    last_quality: f32,
    /// The user chose the original paper over the style's themed pages. Kept per
    /// view across style changes, so it survives a host theme round trip.
    original_paper: bool,
    /// The tone pages were last rendered in; a change re-renders like a zoom.
    last_tone: PageTone,
    /// Bumped whenever the render scale or tone changes. Visible pages with
    /// an older `image_gen` re-render; in-flight renders from an older generation are
    /// discarded so a stale-scale bitmap never lands.
    generation: u64,
    /// Painted bitmaps awaiting a `drop_image` (which needs a `Window`); drained at
    /// the top of `ensure_window`. Used when a fresh bitmap replaces an old one.
    pending_drops: Vec<Arc<RenderImage>>,
    /// `Some` while the page-number field is being edited (the typed digits).
    page_input: Option<String>,
    focus: FocusHandle,
    /// The document outline (bookmarks), extracted once on load; empty if the PDF
    /// has none. Drives the optional table-of-contents panel.
    outline: Vec<OutlineItem>,
    /// Whether the table-of-contents panel is open.
    toc_open: bool,
    /// Per-page clickable link annotations (internal page jumps + external URIs),
    /// extracted once on load. Overlaid as transparent click targets on each page.
    links: Vec<Vec<PdfLink>>,
    /// Form-field widgets (`forms` feature), enumerated on load from the
    /// original bytes. Overlaid like `links`; a click emits
    /// [`PdfEvent::FieldClicked`].
    form_fields: Vec<FormField>,
    /// Per-page extracted text layer, built lazily for search.
    page_text: std::collections::HashMap<usize, TextSlot>,
    /// Whether the find-in-PDF bar is open. (`search` feature.)
    search_open: bool,
    /// The current search query (edited in the find bar).
    search_query: String,
    /// Matches across the document, in reading order (page, then top-to-bottom).
    matches: Vec<SearchMatch>,
    /// Index into `matches` of the focused match (the one ↑/↓/Enter cycle through).
    current_match: Option<usize>,
}

impl PdfView {
    /// Create a viewer for `path`, kicking off the off-thread read + parse + measure.
    /// `style` supplies chrome colors and `quality` the DPI multiplier, both read at
    /// paint time (see [`PdfStyleFn`] / [`PdfQualityFn`]). Call inside
    /// `cx.new(|cx| PdfView::new(path, style, quality, cx))`.
    pub fn new(
        path: PathBuf,
        style: PdfStyleFn,
        quality: PdfQualityFn,
        cx: &mut Context<Self>,
    ) -> Self {
        let load_path = path.clone();
        cx.spawn(async move |this, cx| {
            // Read the file off-thread, keep the bytes (so an encrypted PDF can be
            // retried with a password without re-reading), then parse + measure.
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let bytes = Arc::new(std::fs::read(&load_path).map_err(|e| e.to_string())?);
                    Ok::<_, String>((bytes.clone(), prepare(bytes, "")))
                })
                .await;
            let (bytes, prepared) = match loaded {
                Ok(x) => x,
                Err(e) => {
                    log::error!("read pdf: {e}");
                    let _ = this.update(cx, |this, cx| {
                        this.fail_load(format!("Couldn’t read the file: {e}"), cx);
                    });
                    return;
                }
            };
            let _ = this.update(cx, |this, cx| {
                this.bytes = Some(bytes);
                match prepared {
                    Ok(p) => this.install_document(p, cx),
                    // Encrypted: hold for the host to prompt + call `unlock`.
                    Err(LoadError::Locked) => {
                        this.locked = true;
                        cx.emit(PdfEvent::LockChanged);
                        cx.notify();
                    }
                    Err(LoadError::Other(e)) => {
                        log::error!("parse pdf: {e}");
                        this.fail_load(format!("Couldn’t open the PDF: {e}"), cx);
                    }
                }
            });
        })
        .detach();

        let last_quality = quality().clamp(MIN_QUALITY, MAX_QUALITY);
        let last_tone = page_tone(style().themed_pages.as_ref());
        Self {
            #[cfg(test)]
            render_requests: 0,
            display_name: None,
            path,
            style,
            quality,
            loading_indicator: None,
            pdf: None,
            bytes: None,
            locked: false,
            load_error: None,
            on_close: None,
            dims: Vec::new(),
            pages: Vec::new(),
            active_renders: 0,
            released: false,
            scroll: ScrollHandle::new(),
            fit: None,
            fit_viewport: (0.0, 0.0),
            awaiting_fit_layout: false,
            zoom: 1.0,
            last_quality,
            original_paper: false,
            last_tone,
            generation: 0,
            pending_drops: Vec::new(),
            page_input: None,
            focus: cx.focus_handle(),
            outline: Vec::new(),
            toc_open: false,
            links: Vec::new(),
            form_fields: Vec::new(),
            page_text: std::collections::HashMap::new(),
            search_open: false,
            search_query: String::new(),
            matches: Vec::new(),
            current_match: None,
        }
    }

    /// Store a freshly parsed document + its measurements and clear the lock — the
    /// shared tail of the initial load, a successful [`PdfView::unlock`], and
    /// [`PdfView::replace_bytes`].
    fn install_document(&mut self, prepared: Prepared, cx: &mut Context<Self>) {
        let n = prepared.dims.len();
        self.pdf = Some(prepared.doc);
        self.dims = prepared.dims;
        self.outline = prepared.toc;
        self.links = prepared.links;
        {
            self.form_fields = prepared.fields;
        }
        // Page slots: on the initial load, empty ones (the next render's
        // `ensure_window` rasterizes the visible window). On a replace_bytes
        // reload, KEEP the old bitmaps and bump the generation instead — the
        // stale page paints until its crisp replacement lands, the same
        // no-blanking swap zoom and quality changes use (blanking every slot
        // flashed the whole viewer black on each form-field write).
        self.generation = self.generation.wrapping_add(1);
        if self.pages.len() != n {
            for slot in std::mem::replace(&mut self.pages, vec![Slot::default(); n]) {
                if let Some(arc) = slot.image {
                    self.pending_drops.push(arc);
                }
            }
        }
        // Stale text layers would match search against the old bytes.
        self.page_text.clear();
        self.locked = false;
        cx.emit(PdfEvent::LockChanged);
        cx.notify();
    }

    /// A field's window-space bounds right now, from its page-normalized rect
    /// — the inverse of `point_to_page`'s mapping (`bounds_for_item` is in the
    /// scroll element's unscrolled frame; y shifts by the scroll offset).
    /// `None` before the page has laid out. (`forms` feature.)
    fn field_screen_bounds(
        &self,
        page: usize,
        nrect: (f32, f32, f32, f32),
    ) -> Option<Bounds<Pixels>> {
        let cb = self.scroll.bounds_for_item(page)?;
        let (nx, ny, nw, nh) = nrect;
        let w = f32::from(cb.size.width);
        let h = f32::from(cb.size.height);
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        Some(Bounds::new(
            point(
                cb.origin.x + self.scroll.offset().x + px(nx * w),
                cb.origin.y + self.scroll.offset().y + px(ny * h),
            ),
            gpui::size(px(nw * w), px(nh * h)),
        ))
    }

    /// Whether parsing and page preparation completed successfully. Hosts can
    /// keep an existing preview until a replacement reaches this state.
    pub fn is_loaded(&self) -> bool {
        self.pdf.is_some()
    }

    /// Whether the PDF is encrypted and awaiting a password — the host should show a
    /// prompt and call [`PdfView::unlock`] rather than rendering the viewer.
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Record a terminal load failure (the message renders in the viewer) and
    /// tell the host.
    fn fail_load(&mut self, msg: String, cx: &mut Context<Self>) {
        self.load_error = Some(msg.into());
        cx.emit(PdfEvent::LoadFailed);
        cx.notify();
    }

    /// The terminal read/parse failure shown in place of the viewer, if any.
    pub fn load_error(&self) -> Option<&SharedString> {
        self.load_error.as_ref()
    }

    /// Customize active load feedback without changing parsing or page scheduling.
    /// Unscheduled page placeholders continue to show their page number.
    pub fn set_loading_indicator(&mut self, render: Rc<dyn Fn(SharedString) -> gpui::AnyElement>) {
        self.loading_indicator = Some(render);
    }

    /// Show a close (✕) control beside the source name that calls `f`, so the
    /// host can hide this viewer. Without one, the name row has no control.
    pub fn set_on_close(&mut self, f: CloseFn) {
        self.on_close = Some(f);
    }

    // ───────────────────────────── Find-in-PDF (search) ─────────────────────────────

    /// Toggle the find bar. On open, extract every page's text (off-thread, cached)
    /// and compute matches; on close, drop them. (`search` feature.)
    pub(crate) fn toggle_search(&mut self, cx: &mut Context<Self>) {
        self.search_open = !self.search_open;
        if self.search_open {
            self.ensure_all_text(cx);
            self.recompute_matches(true, cx);
            if let Some(i) = self.current_match {
                self.goto_match(i, cx);
            }
        } else {
            self.matches.clear();
            self.current_match = None;
        }
        cx.notify();
    }

    /// Close the find bar and clear matches. (`search` feature.)
    pub(crate) fn close_search(&mut self, cx: &mut Context<Self>) {
        self.search_open = false;
        self.matches.clear();
        self.current_match = None;
        cx.notify();
    }

    /// Re-run the search after the query changed: recompute matches and jump to the
    /// first one. (`search` feature.)
    fn on_search_query_changed(&mut self, cx: &mut Context<Self>) {
        self.ensure_all_text(cx);
        self.recompute_matches(true, cx);
        if let Some(i) = self.current_match {
            self.goto_match(i, cx);
        }
    }

    /// Kick off text extraction for every page (idempotent, cached), so a search sees
    /// pages that were never scrolled into view. (`search` feature.)
    fn ensure_all_text(&mut self, cx: &mut Context<Self>) {
        for p in 0..self.dims.len() {
            self.ensure_page_text(p, cx);
        }
    }

    /// Rebuild the match list from every page whose text is ready. With
    /// `reset_current`, focus the first match; otherwise keep the focused match (by
    /// page + position) across the rebuild, so a mid-sweep refresh doesn't jump. (`search`.)
    fn recompute_matches(&mut self, reset_current: bool, cx: &mut Context<Self>) {
        let prev = if reset_current {
            None
        } else {
            self.current_match
                .and_then(|i| self.matches.get(i))
                .map(|m| (m.page, m.rects.first().map(|r| r.y).unwrap_or(0.0)))
        };
        self.matches.clear();
        let q = self.search_query.trim().to_string();
        if !q.is_empty() {
            for page in 0..self.dims.len() {
                if let Some(TextSlot::Ready(pt)) = self.page_text.get(&page) {
                    for rects in pt.find_matches(&q) {
                        if !rects.is_empty() {
                            self.matches.push(SearchMatch { page, rects });
                        }
                    }
                }
            }
        }
        self.current_match = if self.matches.is_empty() {
            None
        } else if let Some((pg, y)) = prev {
            self.matches
                .iter()
                .enumerate()
                .filter(|(_, m)| m.page == pg)
                .min_by(|(_, a), (_, b)| {
                    let da = (a.rects.first().map(|r| r.y).unwrap_or(0.0) - y).abs();
                    let db = (b.rects.first().map(|r| r.y).unwrap_or(0.0) - y).abs();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .or(Some(0))
        } else {
            // Fresh query: start from the page the user is on, not the document top.
            self.match_from_viewport()
        };
        cx.notify();
    }

    /// The index of the first match at or below the current viewport top, so opening
    /// the find bar (or editing the query) starts from the page being read rather than
    /// the start of the document. Wraps to the first match if none are below. Matches
    /// are in reading order, so the first one past the fold is just `position`. (`search`.)
    fn match_from_viewport(&self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        let pw = self.page_width();
        let scroll_y = f32::from(-self.scroll.offset().y).max(0.0);
        let idx = self.matches.iter().position(|m| {
            let ry = m.rects.first().map(|r| r.y).unwrap_or(0.0);
            page_top_y(&self.dims, pw, m.page) + ry * display_height(self.dims[m.page], pw)
                >= scroll_y
        });
        Some(idx.unwrap_or(0))
    }

    /// Focus the next match (wrapping) and scroll to it. (`search` feature.)
    pub(crate) fn next_match(&mut self, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        let n = self.matches.len();
        let i = self.current_match.map_or(0, |c| (c + 1) % n);
        self.current_match = Some(i);
        self.goto_match(i, cx);
    }

    /// Focus the previous match (wrapping) and scroll to it. (`search` feature.)
    pub(crate) fn prev_match(&mut self, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        let n = self.matches.len();
        let i = self.current_match.map_or(0, |c| (c + n - 1) % n);
        self.current_match = Some(i);
        self.goto_match(i, cx);
    }

    /// Bring match `idx` into view — but only scroll if it isn't already comfortably
    /// visible, so starting a search on the page you're reading doesn't yank it around.
    /// When it does scroll, the match lands a little below the viewport top. (`search`.)
    fn goto_match(&mut self, idx: usize, cx: &mut Context<Self>) {
        if self.dims.is_empty() {
            return;
        }
        let Some(m) = self.matches.get(idx) else {
            return;
        };
        let page = m.page.min(self.dims.len() - 1);
        let ry = m.rects.first().map(|r| r.y).unwrap_or(0.0);
        let rh = m.rects.first().map(|r| r.h).unwrap_or(0.0);
        let pw = self.page_width();
        let disp_h = display_height(self.dims[page], pw);
        let top = page_top_y(&self.dims, pw, page) + ry * disp_h;
        let bottom = top + rh * disp_h;
        let scroll_y = f32::from(-self.scroll.offset().y).max(0.0);
        let viewport_h = f32::from(self.scroll.bounds().size.height).max(1.0);
        if top < scroll_y + 8.0 || bottom > scroll_y + viewport_h - 8.0 {
            let y = (top - 80.0).max(0.0);
            self.scroll
                .set_offset(point(self.scroll.offset().x, px(-y)));
        }
        cx.notify();
    }

    /// Extract `page`'s text layer off-thread (cached), so search can match it
    /// on the next frame.
    fn ensure_page_text(&mut self, page: usize, cx: &mut Context<Self>) {
        if self.page_text.contains_key(&page) {
            return;
        }
        let Some(pdf) = self.pdf.clone() else {
            return;
        };
        self.page_text.insert(page, TextSlot::Loading);
        cx.spawn(async move |this, cx| {
            let extracted = cx
                .background_executor()
                .spawn(async move { extract_page_text(&pdf, page) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.page_text.insert(
                    page,
                    match extracted {
                        Some(pt) => TextSlot::Ready(pt),
                        None => TextSlot::Failed,
                    },
                );
                cx.notify();
                // If a search is running and this was the last page to extract, fold in
                // its matches (keeping the focused one). Doing it once at the end keeps
                // the whole-document sweep from re-searching on every page.
                if this.search_open
                    && !this.search_query.trim().is_empty()
                    && !this
                        .page_text
                        .values()
                        .any(|s| matches!(s, TextSlot::Loading))
                {
                    this.recompute_matches(false, cx);
                }
            });
        })
        .detach();
    }

    /// Free every rasterized page — CPU pixel buffer (by dropping the `Arc`s) *and*
    /// the GPU atlas texture. gpui caches one atlas texture per `RenderImage` on
    /// paint and only frees it via `drop_image`; a raw `ImageSource::Render` is never
    /// auto-evicted, so call this before dropping the view or the textures leak.
    pub fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.released = true;
        self.generation = self.generation.wrapping_add(1);
        for arc in std::mem::take(&mut self.pending_drops) {
            cx.drop_image(arc, Some(window));
        }
        for slot in std::mem::take(&mut self.pages) {
            if let Some(arc) = slot.image {
                cx.drop_image(arc, Some(window));
            }
        }
    }

    /// Current reading position for a host's lightweight session restoration.
    pub fn reading_position(&self) -> (usize, f32) {
        (self.current_page_index(), self.zoom)
    }

    /// Current automatic sizing policy, or `None` for manual zoom.
    /// Whether the user switched themed pages back to the original paper.
    pub fn shows_original_paper(&self) -> bool {
        self.original_paper
    }

    /// Show the original paper instead of the style's themed pages (or go back).
    /// Re-renders visible pages on the next frame; old bitmaps stay until replaced.
    pub fn set_original_paper(&mut self, original: bool, cx: &mut Context<Self>) {
        if self.original_paper != original {
            self.original_paper = original;
            cx.notify();
        }
    }

    /// The themed colors in effect: the style's, unless overridden to original paper.
    fn themed_pages(&self, style: &PdfStyle) -> Option<ThemedPages> {
        style.themed_pages.filter(|_| !self.original_paper)
    }

    pub fn fit_mode(&self) -> Option<FitMode> {
        self.fit
    }

    /// Set the zoom factor (clamped), keeping the current page in view. Visible pages
    /// re-render crisp at the new scale; their current bitmaps stay on screen
    /// (rescaled) until the fresh ones land, so nothing blanks.
    pub fn set_zoom(&mut self, zoom: f32, cx: &mut Context<Self>) {
        // A manual zoom takes over from any active fit mode.
        self.fit = None;
        self.apply_zoom(zoom, cx);
    }

    fn apply_zoom(&mut self, zoom: f32, cx: &mut Context<Self>) {
        let z = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        if (z - self.zoom).abs() < 0.001 {
            return;
        }
        let anchor = self.current_page_index();
        self.zoom = z;
        self.generation = self.generation.wrapping_add(1);
        self.go_to_page(anchor, cx);
        cx.notify();
    }

    /// Fit the page column to the viewport width. Sticky: re-fits as the
    /// viewer resizes, until a manual zoom (buttons, ⌘±/⌘0, [`set_zoom`])
    /// takes over. Toggles off if already active.
    ///
    /// [`set_zoom`]: Self::set_zoom
    pub fn fit_width(&mut self, cx: &mut Context<Self>) {
        self.toggle_fit(FitMode::Width, cx);
    }

    /// Fit the whole current page inside the viewport (both axes). Sticky
    /// like [`fit_width`](Self::fit_width); toggles off if already active.
    pub fn fit_page(&mut self, cx: &mut Context<Self>) {
        self.toggle_fit(FitMode::Page, cx);
    }

    fn toggle_fit(&mut self, mode: FitMode, cx: &mut Context<Self>) {
        self.fit = if self.fit == Some(mode) {
            None
        } else {
            Some(mode)
        };
        self.apply_fit(cx);
        cx.notify();
    }

    /// Compute + apply the zoom for the active fit mode against the current
    /// viewport. No-op before first layout (render re-applies once bounds are
    /// known) or with no fit active.
    fn apply_fit(&mut self, cx: &mut Context<Self>) {
        let Some(mode) = self.fit else {
            return;
        };
        let vp = self.scroll.bounds().size;
        let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
        if vw <= 1.0 || vh <= 1.0 {
            return;
        }
        self.fit_viewport = (vw, vh);
        // Chrome around the page column: the overlay scrollbar plus the page
        // slots' 1px borders and a hair of breathing room.
        let avail_w = (vw - SCROLLBAR_W - 6.0).max(50.0);
        let zoom = match mode {
            FitMode::Width => avail_w / PAGE_WIDTH,
            FitMode::Page => {
                let (pw, ph) = self
                    .dims
                    .get(self.current_page_index())
                    .copied()
                    .filter(|(w, h)| *w > 0.0 && *h > 0.0)
                    .unwrap_or((612.0, 792.0));
                let avail_h = (vh - 2.0 * PAGE_PAD_Y).max(50.0);
                (avail_h / (PAGE_WIDTH * (ph / pw))).min(avail_w / PAGE_WIDTH)
            }
        };
        self.apply_zoom(zoom, cx);
    }

    /// Zoom in one step.
    pub(crate) fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(self.zoom * ZOOM_STEP, cx);
    }

    /// Zoom out one step.
    pub(crate) fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(self.zoom / ZOOM_STEP, cx);
    }

    /// Reset zoom to 100%.
    pub(crate) fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(1.0, cx);
    }

    /// Scroll so page `index` (0-based, clamped) is at the top of the viewport.
    pub fn go_to_page(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.dims.is_empty() {
            return;
        }
        let i = index.min(self.dims.len() - 1);
        // Align page `i`'s top with the viewport top so the page counter (which reads
        // the topmost visible page from the scroll offset) agrees with where we land.
        // Page 0 goes to the true document top (keeping its column padding).
        let y = if i == 0 {
            0.0
        } else {
            page_top_y(&self.dims, self.page_width(), i)
        };
        self.scroll
            .set_offset(point(self.scroll.offset().x, px(-y)));
        cx.notify();
    }

    /// Toggle the table-of-contents (outline) panel.
    pub(crate) fn toggle_toc(&mut self, cx: &mut Context<Self>) {
        self.toc_open = !self.toc_open;
        cx.notify();
    }

    /// Whether the document has an outline (bookmarks) to show.
    pub(crate) fn has_outline(&self) -> bool {
        !self.outline.is_empty()
    }

    /// Go to the next page.
    pub(crate) fn next_page(&mut self, cx: &mut Context<Self>) {
        self.go_to_page(self.current_page_index() + 1, cx);
    }

    /// Go to the previous page.
    pub(crate) fn prev_page(&mut self, cx: &mut Context<Self>) {
        self.go_to_page(self.current_page_index().saturating_sub(1), cx);
    }

    /// On-screen column width at the current zoom.
    fn page_width(&self) -> f32 {
        PAGE_WIDTH * self.zoom
    }

    /// The topmost visible page index for the current scroll position.
    fn current_page_index(&self) -> usize {
        let scroll_y = f32::from(-self.scroll.offset().y);
        current_page(&self.dims, self.page_width(), scroll_y)
    }

    /// Keep only the pages near the viewport rasterized: render missing / stale
    /// visible pages (at a DPI-, zoom-, and quality-aware scale) and evict the rest.
    /// Called every frame from `render` — cheap (a window calc + slot scan); it only
    /// spawns/evicts when something actually changed. This is what bounds an open
    /// PDF's memory to the on-screen pages instead of the whole document.
    fn ensure_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Free atlas textures retired by an earlier re-render (we have a Window here).
        for arc in std::mem::take(&mut self.pending_drops) {
            cx.drop_image(arc, Some(window));
        }
        if self.released || self.dims.is_empty() || self.pdf.is_none() {
            return; // not loaded yet (still reading/parsing)
        }

        // Fit needs the scroll area's first layout. Let this frame measure it,
        // then request a new frame to fit before allocating any page bitmaps.
        let viewport = self.scroll.bounds().size;
        if self.fit.is_some()
            && (f32::from(viewport.width) <= 1.0 || f32::from(viewport.height) <= 1.0)
        {
            if !self.awaiting_fit_layout {
                self.awaiting_fit_layout = true;
                cx.notify();
            }
            return;
        }
        self.awaiting_fit_layout = false;

        // A host quality change invalidates every bitmap (new scale), like a zoom.
        let quality = (self.quality)().clamp(MIN_QUALITY, MAX_QUALITY);
        if (quality - self.last_quality).abs() > 0.01 {
            self.last_quality = quality;
            self.generation = self.generation.wrapping_add(1);
        }
        // So does a tone change (host theme or the original-paper toggle).
        let tone = page_tone(self.themed_pages(&(self.style)()).as_ref());
        if tone != self.last_tone {
            self.last_tone = tone;
            self.generation = self.generation.wrapping_add(1);
        }

        let page_width = self.page_width();
        let scale_factor = window.scale_factor();
        let scroll_y = f32::from(-self.scroll.offset().y);
        let viewport_h = f32::from(self.scroll.bounds().size.height);
        let (start, end) = keep_window(&self.dims, page_width, scroll_y, viewport_h);
        let generation = self.generation;

        // Decide what to (re-)render and what to evict. A visible page renders if it
        // has no bitmap or a stale-generation one; it keeps showing the old bitmap
        // meanwhile. An off-window page drops its bitmap.
        let mut to_render: Vec<usize> = Vec::new();
        let mut to_evict: Vec<Arc<RenderImage>> = Vec::new();
        for (i, slot) in self.pages.iter_mut().enumerate() {
            let in_window = i >= start && i <= end;
            if in_window {
                if slot.loading.is_none() && (slot.image.is_none() || slot.image_gen != generation)
                {
                    to_render.push(i);
                }
            } else if let Some(arc) = slot.image.take() {
                slot.image_gen = 0;
                to_evict.push(arc);
            }
        }
        for arc in to_evict {
            cx.drop_image(arc, Some(window));
        }
        if to_render.is_empty() {
            return;
        }

        // Render pages intersecting the viewport before the one-page prefetch
        // margin, so the page you're looking at fills in before its neighbors.
        let first_visible = current_page(&self.dims, page_width, scroll_y);
        let last_visible = current_page(&self.dims, page_width, scroll_y + viewport_h.max(1.0));
        to_render.sort_by_key(|&i| {
            first_visible
                .saturating_sub(i)
                .max(i.saturating_sub(last_visible))
        });
        // Rapid scrolling/zooming must not queue a raster for every page passed.
        // Reconsider the latest viewport after each completion, with visible
        // pages ahead of prefetch. At most two CPU rasters compete with editing.
        to_render.truncate(MAX_ACTIVE_RENDERS.saturating_sub(self.active_renders));

        let pdf = self.pdf.clone().unwrap();
        for i in to_render {
            self.active_renders += 1;
            self.pages[i].loading = Some(generation);
            #[cfg(test)]
            {
                self.render_requests += 1;
            }
            let pdf = pdf.clone();
            let scale = render_scale(page_width, scale_factor, quality, self.dims[i].0);
            cx.spawn(async move |this, cx| {
                let wanted = this
                    .update(cx, |this, _| {
                        let (start, end) = keep_window(
                            &this.dims,
                            this.page_width(),
                            f32::from(-this.scroll.offset().y),
                            f32::from(this.scroll.bounds().size.height),
                        );
                        !this.released
                            && this.generation == generation
                            && (start..=end).contains(&i)
                    })
                    .unwrap_or(false);
                let page = if wanted {
                    cx.background_executor()
                        .spawn(async move { render_page(&pdf, i, scale, tone).ok() })
                        .await
                } else {
                    None
                };
                let _ = this.update(cx, |this, cx| {
                    // Store only if still wanted: same generation (scale unchanged)
                    // and still inside the viewport window. Otherwise discard — the
                    // bitmap was never painted, so it holds no atlas texture.
                    let in_window = {
                        let pw = this.page_width();
                        let sy = f32::from(-this.scroll.offset().y);
                        let vh = f32::from(this.scroll.bounds().size.height);
                        let (s, e) = keep_window(&this.dims, pw, sy, vh);
                        i >= s && i <= e
                    };
                    this.active_renders = this.active_renders.saturating_sub(1);
                    let gen_now = this.generation;
                    let mut retired = None;
                    if let Some(slot) = this.pages.get_mut(i) {
                        if slot.loading == Some(generation) {
                            slot.loading = None;
                        }
                        if !this.released
                            && gen_now == generation
                            && in_window
                            && let Some(img) = page
                        {
                            retired = slot.image.replace(img);
                            slot.image_gen = generation;
                        }
                    }
                    // The replaced bitmap was painted, so its atlas texture must be
                    // freed — defer to the next `ensure_window` (which has a Window).
                    if let Some(old) = retired {
                        this.pending_drops.push(old);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// One header control button (clickable, hover-highlighted).
    fn control(
        &self,
        id: &'static str,
        label: impl Into<gpui::SharedString>,
    ) -> gpui::Stateful<gpui::Div> {
        let style = (self.style)();
        let label = label.into();
        div()
            .id(id)
            .when(cfg!(test), |v| v.debug_selector(move || id.into()))
            .key_context("PdfControl")
            .tab_index(0)
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .focus_visible(|s| s.bg(style.placeholder_bg))
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .min_w(px(20.0))
            .px(px(6.0))
            .py(px(1.0))
            .rounded(px(4.0))
            .cursor_pointer()
            .text_color(style.header_fg)
            .hover(|s| s.bg(style.placeholder_bg))
            .child(label)
    }

    /// Host-facing source identity, independent of a temporary PDF backing path.
    pub fn set_display_name(&mut self, name: String, cx: &mut Context<Self>) {
        self.display_name = Some(name);
        cx.notify();
    }

    pub fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus.clone()
    }

    /// Build a `.tooltip(..)` closure for a header control. gpui core has the tooltip
    /// *hook* but no tooltip *view* (those live in higher-level UI crates we don't
    /// depend on), so we render a small themed one ([`Tip`]), reading colors through
    /// the same style closure at show time.
    fn tip(
        &self,
        text: impl Into<SharedString>,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let style_fn = self.style.clone();
        let text = text.into();
        move |_window, cx| {
            let s = style_fn();
            let text = text.clone();
            cx.new(move |_| Tip {
                text,
                fg: s.header_fg,
                bg: s.placeholder_bg,
                border: s.border,
            })
            .into()
        }
    }
}

impl EventEmitter<PdfEvent> for PdfView {}

impl Render for PdfView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A sticky fit mode re-fits when the viewport size changes (window or
        // sidebar resize) — and computes the first real fit once layout exists.
        if self.fit.is_some() {
            let vp = self.scroll.bounds().size;
            let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
            if (vw - self.fit_viewport.0).abs() > 1.0 || (vh - self.fit_viewport.1).abs() > 1.0 {
                self.apply_fit(cx);
            }
        }
        // Keep only the on-screen pages rasterized for the current scroll position.
        self.ensure_window(window, cx);
        let style = (self.style)();

        if let Some(err) = &self.load_error {
            return load_failed(style, err.clone()).into_any_element();
        }
        if self.dims.is_empty() {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(style.bg)
                .child(self.loading_indicator.as_ref().map_or_else(
                    || loading(style).into_any_element(),
                    |render| render("Loading PDF…".into()),
                ))
                .into_any_element();
        }

        let name = self.display_name.clone().unwrap_or_else(|| {
            self.path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let pane_width = f32::from(self.scroll.bounds().size.width);
        let narrow = pane_width > 0. && pane_width < 600.;
        let total = self.dims.len();
        let page_width = self.page_width();
        let current = current_page(&self.dims, page_width, f32::from(-self.scroll.offset().y));
        let themed = self.themed_pages(&style);
        // Unrendered themed slots paint in the paper color, never a light box.
        let slot_bg = themed.map_or(style.placeholder_bg, |t| t.paper);
        let overlays = themed.map_or(style.overlays, |t| t.overlays);

        // Page slots are *direct* children of the scroll container (assembled below), so
        // `ScrollHandle::bounds_for_item(i)` yields page `i`'s real laid-out bounds —
        // which `point_to_page` maps the cursor against, instead of re-summing
        // `display_height`. That sum drifts from gpui's pixel-snapped layout (~0.18px per
        // Letter page → ~2 text rows off by page 150 of a long PDF).
        let mut slots: Vec<gpui::AnyElement> = Vec::with_capacity(self.dims.len());
        for (i, dim) in self.dims.iter().enumerate() {
            let disp_h = display_height(*dim, page_width);
            let slot = div()
                .relative()
                // The scroll container is now a flex column, so pages must NOT shrink to
                // fit the viewport — keep their full height and let the column overflow.
                .flex_shrink_0()
                .w(px(page_width))
                .h(px(disp_h))
                .rounded(px(2.0))
                .border_1()
                .border_color(style.border)
                .overflow_hidden();
            let slot = match self.pages.get(i).and_then(|s| s.image.as_ref()) {
                Some(image) => slot.child(img(image.clone()).w(px(page_width)).h(px(disp_h))),
                // No bitmap yet: a sized placeholder so layout is stable; it fills in
                // once rasterized.
                None => slot
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(slot_bg)
                    .child(
                        if self.pages.get(i).is_some_and(|slot| slot.loading.is_some()) {
                            self.loading_indicator.as_ref().map_or_else(
                                || {
                                    div()
                                        .text_color(style.placeholder_fg)
                                        .child(format!("Page {}", i + 1))
                                        .into_any_element()
                                },
                                |render| render(format!("Preparing page {}…", i + 1).into()),
                            )
                        } else {
                            div()
                                .text_color(style.placeholder_fg)
                                .child(format!("Page {}", i + 1))
                                .into_any_element()
                        },
                    ),
            };
            let slot = {
                let mut slot = slot;
                // Find-in-PDF: box every match on this page; emphasize the focused one.
                for (mi, m) in self.matches.iter().enumerate() {
                    if m.page != i {
                        continue;
                    }
                    let current = self.current_match == Some(mi);
                    let fill = if current {
                        overlays.search_current
                    } else {
                        overlays.search
                    };
                    for r in &m.rects {
                        let mut b = div()
                            .absolute()
                            .left(px(r.x * page_width))
                            .top(px(r.y * disp_h))
                            .w(px(r.w * page_width))
                            .h(px(r.h * disp_h))
                            .rounded(px(1.0))
                            .bg(fill);
                        if current {
                            b = b.border_1().border_color(overlays.search_border);
                        }
                        slot = slot.child(b);
                    }
                }
                slot
            };
            // Clickable link annotations: transparent overlays that navigate on click
            // (internal page jump or external URL), with a faint hover so they read as
            // links. Coordinates are normalized to the page, like search matches.
            let mut slot = slot;
            if let Some(links) = self.links.get(i) {
                for (li, link) in links.iter().enumerate() {
                    let target = link.target.clone();
                    slot = slot.child(
                        div()
                            .id(SharedString::from(format!("pdf-link-{i}-{li}")))
                            .absolute()
                            .left(px(link.x * page_width))
                            .top(px(link.y * disp_h))
                            .w(px(link.w * page_width))
                            .h(px(link.h * disp_h))
                            .rounded(px(2.0))
                            .cursor_pointer()
                            .hover(move |h| h.bg(overlays.link_hover))
                            .on_click(cx.listener(move |this, _, _window, cx| match &target {
                                LinkTarget::Page(p) => this.go_to_page(*p, cx),
                                LinkTarget::Uri(u) if allowed_uri(u) => cx.open_url(u),
                                LinkTarget::Uri(_) => {}
                            })),
                    );
                }
            }
            // Form-field widgets (forms): transparent click targets like links,
            // with a faint hover so a fillable field reads as interactive. A
            // click emits FieldClicked with the widget's live window bounds —
            // the host toggles/seats an input and writes via set_form_value.
            {
                let (pw_pt, ph_pt) = self.dims[i];
                for (fi, field) in self.form_fields.iter().enumerate() {
                    if field.page != i || pw_pt <= 0.0 || ph_pt <= 0.0 {
                        continue;
                    }
                    let (x0, y0, x1, y1) = field.rect;
                    let (nx, ny) = (x0 / pw_pt, 1.0 - y1 / ph_pt);
                    let (nw, nh) = ((x1 - x0) / pw_pt, (y1 - y0) / ph_pt);
                    let f = field.clone();
                    slot = slot.child(
                        div()
                            .id(SharedString::from(format!("pdf-field-{i}-{fi}")))
                            .absolute()
                            .left(px(nx * page_width))
                            .top(px(ny * disp_h))
                            .w(px(nw * page_width))
                            .h(px(nh * disp_h))
                            .rounded(px(2.0))
                            .cursor_pointer()
                            .hover(move |h| h.bg(overlays.field_hover))
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                if let Some(bounds) =
                                    this.field_screen_bounds(f.page, (nx, ny, nw, nh))
                                {
                                    cx.emit(PdfEvent::FieldClicked {
                                        field: f.clone(),
                                        bounds,
                                    });
                                }
                            })),
                    );
                }
            }
            slots.push(slot.into_any_element());
        }

        // Click-to-edit page counter: shows "N / total", or the typed digits while
        // editing (Enter jumps, Esc cancels — handled in `on_key_down`).
        let counter_label = match &self.page_input {
            Some(buf) if !buf.is_empty() => format!("{buf} / {total}"),
            Some(_) => format!("⌷ / {total}"),
            None => format!("{} / {total}", current + 1),
        };
        let editing = self.page_input.is_some();
        let counter = div()
            .id("pdf-page-counter")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "pdf-page-counter".into())
            })
            .flex_shrink_0()
            .role(gpui::Role::Button)
            .aria_label("Go to page")
            .key_context("PdfControl")
            .tab_index(0)
            .focus_visible(|s| s.bg(style.placeholder_bg))
            .min_w(px(78.0))
            .flex()
            .items_center()
            .justify_center()
            .px(px(8.0))
            .py(px(1.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(if editing {
                style.header_fg
            } else {
                style.border
            })
            .text_color(style.header_muted)
            .cursor_pointer()
            .hover(|s| s.bg(style.placeholder_bg))
            .child(counter_label)
            .on_click(cx.listener(|this, _, window, cx| {
                this.page_input.get_or_insert_with(String::new);
                window.focus(&this.focus, cx);
                cx.notify();
            }))
            .tooltip(self.tip("Go to page (⌘⌥G)"));

        // The table-of-contents toggle sits at the left, next to the title, since the
        // panel opens on that side. Only shown when the PDF has an outline.
        let toc_toggle = self.has_outline().then(|| {
            let toc_bg = if self.toc_open {
                style.placeholder_bg
            } else {
                Hsla { a: 0.0, ..style.bg }
            };
            self.control("pdf-toc", "≡")
                .aria_label("Document outline")
                .bg(toc_bg)
                .on_click(cx.listener(|this, _, _window, cx| this.toggle_toc(cx)))
                .tooltip(self.tip("Table of contents"))
        });

        // Keep each tool group intact; wrap groups instead of hiding tools.
        let navigation = div()
            .flex()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .children(toc_toggle)
            .child(
                self.control("pdf-prev", "‹")
                    .aria_label("Previous page")
                    .on_click(cx.listener(|this, _, _, cx| this.prev_page(cx)))
                    .tooltip(self.tip("Previous page (PageUp)")),
            )
            .child(counter)
            .child(
                self.control("pdf-next", "›")
                    .aria_label("Next page")
                    .on_click(cx.listener(|this, _, _, cx| this.next_page(cx)))
                    .tooltip(self.tip("Next page (PageDown)")),
            );
        let zoom = div()
            .flex()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .child(
                self.control("pdf-zoom-out", "−")
                    .aria_label("Zoom out")
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_out(cx)))
                    .tooltip(self.tip("Zoom out (⌘−)")),
            )
            .child(
                self.control(
                    "pdf-zoom-reset",
                    format!("{}%", (self.zoom * 100.0).round() as i32),
                )
                .aria_label("Reset zoom")
                .on_click(cx.listener(|this, _, _, cx| this.reset_zoom(cx)))
                .tooltip(self.tip("Reset zoom (⌘0)")),
            )
            .child(
                self.control("pdf-zoom-in", "+")
                    .aria_label("Zoom in")
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_in(cx)))
                    .tooltip(self.tip("Zoom in (⌘+)")),
            );
        let fit = div()
            .flex()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .child({
                let mut c = self
                    .control("pdf-fit-width", "↔")
                    .aria_label("Fit width")
                    .on_click(cx.listener(|this, _, _, cx| this.fit_width(cx)))
                    .tooltip(self.tip("Fit width"));
                if self.fit == Some(FitMode::Width) {
                    c = c.bg(style.placeholder_bg);
                }
                c
            })
            .child({
                let mut c = self
                    .control("pdf-fit-page", "⤢")
                    .aria_label("Fit page")
                    .on_click(cx.listener(|this, _, _, cx| this.fit_page(cx)))
                    .tooltip(self.tip("Fit page"));
                if self.fit == Some(FitMode::Page) {
                    c = c.bg(style.placeholder_bg);
                }
                c
            });
        let header = div()
            .id("pdf-toolbar")
            .when(cfg!(test), |v| v.debug_selector(|| "pdf-toolbar".into()))
            .flex_shrink_0()
            .px(px(8.))
            .py(px(6.))
            .border_b_1()
            .border_color(style.border)
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .text_size(px(12.))
            .text_color(style.header_fg)
            .child(navigation)
            .child(zoom)
            .child(fit);

        // Page tone toggle (ADR 0035): only when the style offers themed pages.
        let header = header.when(style.themed_pages.is_some(), |header| {
            let label = if self.original_paper {
                "Show dark pages"
            } else {
                "Show original paper"
            };
            let bg = if self.original_paper {
                style.placeholder_bg
            } else {
                Hsla { a: 0.0, ..style.bg }
            };
            header.child(
                self.control("pdf-page-tone", "◐")
                    .aria_label(label)
                    .bg(bg)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_original_paper(!this.original_paper, cx)
                    }))
                    .tooltip(self.tip(label)),
            )
        });

        // Find toggle (search): a magnifier that opens the find bar.
        let header = {
            let bg = if self.search_open {
                style.placeholder_bg
            } else {
                Hsla { a: 0.0, ..style.bg }
            };
            header.child(
                self.control("pdf-find", "🔍")
                    .aria_label("Find")
                    .bg(bg)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_search(cx)))
                    .tooltip(self.tip("Find (⌘F)")),
            )
        };

        let root = div()
            .track_focus(&self.focus)
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(style.bg)
            // Capture focus before scrollbar thumbs consume the click; keyboard
            // navigation must still belong to the viewport after a thumb drag.
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                if event.button == MouseButton::Left {
                    window.focus(&this.focus, cx);
                }
            }))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _window, cx| {
                let key = ev.keystroke.key.as_str();
                if key == "tab" {
                    if ev.keystroke.modifiers.shift {
                        _window.focus_prev(cx);
                    } else {
                        _window.focus_next(cx);
                    }
                    cx.stop_propagation();
                    return;
                }
                // Page-number entry mode swallows keys until Enter/Esc.
                if this.page_input.is_some() {
                    match key {
                        "escape" => {
                            this.page_input = None;
                            cx.notify();
                        }
                        "enter" => {
                            let n = this
                                .page_input
                                .as_deref()
                                .and_then(|s| s.trim().parse::<usize>().ok());
                            this.page_input = None;
                            if let Some(n) = n
                                && !this.dims.is_empty()
                            {
                                this.go_to_page(n.saturating_sub(1).min(this.dims.len() - 1), cx);
                            }
                            cx.notify();
                        }
                        "backspace" => {
                            if let Some(b) = this.page_input.as_mut() {
                                b.pop();
                            }
                            cx.notify();
                        }
                        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_digit()) => {
                            if let Some(b) = this.page_input.as_mut()
                                && b.len() < 7
                            {
                                b.push_str(k);
                            }
                            cx.notify();
                        }
                        _ => {}
                    }
                    return;
                }
                let secondary = ev.keystroke.modifiers.secondary();
                // ⌘⌥G: jump to a page (focus the page-number field to type into).
                if secondary && ev.keystroke.modifiers.alt && key == "g" {
                    this.page_input.get_or_insert_with(String::new);
                    cx.notify();
                    return;
                }
                {
                    // ⌘F toggles the find bar; ⌘G / ⌘⇧G step matches (bar open or not).
                    if secondary && key == "f" {
                        this.toggle_search(cx);
                        return;
                    }
                    if secondary && key == "g" {
                        if ev.keystroke.modifiers.shift {
                            this.prev_match(cx);
                        } else {
                            this.next_match(cx);
                        }
                        return;
                    }
                    // While the bar is open, type to edit the query and Enter/⇧Enter to
                    // step matches. Keys we don't consume (arrows, PageUp/Down…) fall
                    // through, so the page still scrolls with the bar open.
                    if this.search_open {
                        match key {
                            "escape" => {
                                this.close_search(cx);
                                return;
                            }
                            "enter" => {
                                if ev.keystroke.modifiers.shift {
                                    this.prev_match(cx);
                                } else {
                                    this.next_match(cx);
                                }
                                return;
                            }
                            "backspace" => {
                                this.search_query.pop();
                                this.on_search_query_changed(cx);
                                return;
                            }
                            _ => {
                                if let Some(ch) =
                                    ev.keystroke.key_char.as_ref().filter(|s| {
                                        !s.is_empty() && !s.chars().any(char::is_control)
                                    })
                                {
                                    this.search_query.push_str(ch);
                                    this.on_search_query_changed(cx);
                                    return;
                                }
                            }
                        }
                    }
                }
                match key {
                    "pagedown" => this.next_page(cx),
                    "pageup" => this.prev_page(cx),
                    "home" => this.go_to_page(0, cx),
                    "end" => {
                        let last = this.dims.len().saturating_sub(1);
                        this.go_to_page(last, cx);
                    }
                    "=" | "+" if secondary => this.zoom_in(cx),
                    "-" if secondary => this.zoom_out(cx),
                    "0" if secondary => this.reset_zoom(cx),
                    _ => {}
                }
            }));

        // Find bar overlay (search): a floating bar with the query, match count, and
        // prev/next/close. Deferred so it paints over the page area below the header.
        let root = if self.search_open {
            let searching = !self.search_query.trim().is_empty()
                && self
                    .page_text
                    .values()
                    .any(|s| matches!(s, TextSlot::Loading));
            let count = if self.search_query.trim().is_empty() {
                // Empty query: the field already shows the "Find…" placeholder, so the
                // count reads "0 / 0" rather than repeating it.
                "0 / 0".to_string()
            } else if searching {
                "searching…".to_string()
            } else if self.matches.is_empty() {
                "no results".to_string()
            } else {
                format!(
                    "{} / {}",
                    self.current_match.map_or(0, |i| i + 1),
                    self.matches.len()
                )
            };
            let has_query = !self.search_query.is_empty();
            // Query field with a caret. It's static: a blinking caret would need either a
            // focused text widget or a per-frame animation that re-renders the whole
            // viewer; the viewer captures keystrokes directly instead.
            let field = div()
                .min_w(px(120.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(1.0));
            let caret = || {
                div()
                    .w(px(1.5))
                    .h(px(13.0))
                    .rounded(px(1.0))
                    .bg(style.header_fg)
            };
            let field = if has_query {
                field
                    .text_color(style.header_fg)
                    .child(SharedString::from(self.search_query.clone()))
                    .child(caret())
            } else {
                field
                    .child(caret())
                    .child(div().text_color(style.header_muted).child("Find…"))
            };
            root.child(deferred(
                div()
                    .absolute()
                    .top(px(44.0))
                    .right(px(12.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(style.border)
                    .bg(style.bg)
                    .text_size(px(12.0))
                    .child(field)
                    .child(if searching && let Some(render) = &self.loading_indicator {
                        render("Searching…".into())
                    } else {
                        div()
                            .text_color(style.header_muted)
                            .child(count)
                            .into_any_element()
                    })
                    .child(
                        self.control("pdf-find-prev", "‹")
                            .on_click(cx.listener(|this, _, _w, cx| this.prev_match(cx)))
                            .tooltip(self.tip("Previous match (⇧⏎ / ⌘⇧G)")),
                    )
                    .child(
                        self.control("pdf-find-next", "›")
                            .on_click(cx.listener(|this, _, _w, cx| this.next_match(cx)))
                            .tooltip(self.tip("Next match (⏎ / ⌘G)")),
                    )
                    .child(
                        self.control("pdf-find-close", "✕")
                            .on_click(cx.listener(|this, _, _w, cx| this.close_search(cx)))
                            .tooltip(self.tip("Close (Esc)")),
                    ),
            ))
        } else {
            root
        };

        let scrollbar = scrollbar::overlay_scrollbar(
            "pdf-scrollbar-thumb",
            &self.scroll,
            false,
            style.header_muted,
            cx,
        );
        let horizontal_scrollbar = scrollbar::overlay_scrollbar(
            "pdf-horizontal-scrollbar",
            &self.scroll,
            true,
            style.header_muted,
            cx,
        );

        // "Scroll to top": a floating button over the page area, shown once scrolled
        // down past half a viewport. Jumps to the document top (also bound to Home).
        let scroll_top_btn = {
            let viewport_h = f32::from(self.scroll.bounds().size.height);
            let scroll_y = f32::from(-self.scroll.offset().y).max(0.0);
            (viewport_h > 1.0 && scroll_y > viewport_h * 0.5).then(|| {
                div()
                    .id("pdf-scroll-top")
                    .absolute()
                    .bottom(px(16.0))
                    .right(px(SCROLLBAR_W + 10.0))
                    .size(px(34.0))
                    .rounded(px(17.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(16.0))
                    .bg(style.bg)
                    .text_color(style.header_fg)
                    .border_1()
                    .border_color(style.border)
                    .shadow_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(style.placeholder_bg))
                    .child("↑")
                    .tooltip(self.tip("Scroll to top (Home)"))
                    .on_click(cx.listener(|this, _, _window, cx| this.go_to_page(0, cx)))
            })
        };

        // Table-of-contents panel: a scrollable, indented outline; click an entry to
        // jump to its page. Built only when toggled open + the PDF has an outline.
        let toc_panel = (self.toc_open && self.has_outline()).then(|| {
            let mut col = div()
                .id("pdf-toc-panel")
                .when(narrow, |v| v.absolute().left(px(0.)).top(px(0.)).occlude())
                .flex_shrink_0()
                .w(px(280.0).min(px(pane_width.max(320.) - 32.)))
                .h_full()
                .overflow_y_scroll()
                .border_r_1()
                .border_color(style.border)
                .bg(style.bg)
                .py(px(6.0));
            for (i, item) in self.outline.iter().enumerate() {
                let page = item.page;
                let muted = page.is_none();
                let mut row = div()
                    .id(SharedString::from(format!("pdf-toc-{i}")))
                    .pl(px(10.0 + item.level as f32 * 14.0))
                    .pr(px(10.0))
                    .py(px(3.0))
                    .text_size(px(12.0))
                    .text_color(if muted {
                        style.header_muted
                    } else {
                        style.header_fg
                    })
                    .child(item.title.clone());
                // Resolvable entries are clickable (jump to the page); unresolved ones
                // (named destinations) are shown muted and inert.
                if !muted {
                    row = row
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|h| h.bg(style.placeholder_bg))
                        .on_click(cx.listener(move |this, _, _window, cx| {
                            if let Some(p) = page {
                                this.go_to_page(p, cx);
                            }
                        }));
                }
                col = col.child(row);
            }
            col
        });

        let on_close = self.on_close.clone();
        root.child(
            div()
                .px_2()
                .py_1()
                .min_w_0()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap_1()
                .text_size(px(11.))
                .text_color(style.header_muted)
                .child(
                    div()
                        .id("pdf-source-name")
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "pdf-source-name".into())
                        })
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .child(name.clone())
                        .tooltip(self.tip(name)),
                )
                .when_some(on_close, |row, on_close| {
                    row.child(
                        self.control("pdf-close", "✕")
                            .aria_label("Close original")
                            .tooltip(self.tip("Close original"))
                            .on_click(move |_, window, cx| on_close(window, cx)),
                    )
                }),
        )
        .child(header)
        .child(
            // Content row: the optional TOC panel beside the scrollable page column.
            div()
                .relative()
                .min_w_0()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .children(toc_panel)
                .child(
                    // A relative wrapper so the scrollbar can float over the scroll
                    // area's right edge without taking layout space (overlay scrollbar).
                    div()
                        .relative()
                        .min_w_0()
                        .flex_1()
                        .min_h_0()
                        .child(
                            div()
                                .id("pdf-scroll")
                                .min_w_0()
                                .size_full()
                                .overflow_scroll()
                                .track_scroll(&self.scroll)
                                // The page column lives directly on the scroll element
                                // (not nested) so each page is a tracked scroll item —
                                // `point_to_page` reads real bounds via `bounds_for_item`.
                                .flex()
                                .flex_col()
                                .items_start()
                                .when(
                                    page_width <= f32::from(self.scroll.bounds().size.width),
                                    |v| v.items_center(),
                                )
                                .gap(px(PAGE_GAP))
                                .py(px(PAGE_PAD_Y))
                                // Scrolling doesn't re-run render on its own; notify so
                                // the next frame re-runs `ensure_window` + page counter.
                                .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                                .children(slots),
                        )
                        .child(scrollbar)
                        .child(horizontal_scrollbar)
                        .children(scroll_top_btn),
                ),
        )
        .into_any_element()
    }
}

fn allowed_uri(uri: &str) -> bool {
    let Some((scheme, _)) = uri.split_once(':') else {
        return false;
    };
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "mailto"
    )
}

/// The terminal-failure pane: the file name stays in the tab; the pane says
/// why the viewer is empty (instead of an eternal "Loading PDF…").
fn load_failed(style: PdfStyle, err: SharedString) -> impl IntoElement {
    div()
        .size_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .items_center()
        .justify_center()
        .bg(style.bg)
        .child(
            div()
                .text_color(style.placeholder_fg)
                .child("This PDF couldn’t be opened"),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(style.placeholder_fg)
                .max_w(px(520.))
                .child(err),
        )
}

fn loading(style: PdfStyle) -> impl IntoElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(style.bg)
        .child(div().text_color(style.placeholder_fg).child("Loading PDF…"))
}

/// A minimal themed tooltip view — gpui's `.tooltip()` takes any view, and we don't
/// pull in a UI crate just for one label.
struct Tip {
    text: SharedString,
    fg: Hsla,
    bg: Hsla,
    border: Hsla,
}

impl Render for Tip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // gpui anchors the tooltip's top-left at the mouse position (+1px), i.e. *inside*
        // the hovered control. A transparent top padding on the root shifts the visible
        // box down to clear the control + its bar/header padding. (Padding is applied to
        // a `layout_as_root` element; a top *margin* on the root is ignored.)
        div().pt(px(22.0)).child(
            div()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(self.border)
                .bg(self.bg)
                .text_color(self.fg)
                .text_size(px(11.0))
                .child(self.text.clone()),
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn is_pdf_sees_through_url_queries() {
        assert!(super::is_pdf("report.pdf"));
        assert!(super::is_pdf("Report.PDF "));
        assert!(super::is_pdf("https://x.test/report.pdf?v=2"));
        assert!(!super::is_pdf("report.pdf.png"));
        assert!(!super::is_pdf("a.png?name=report.pdf"));
    }

    #[test]
    fn only_external_web_and_mail_links_are_openable() {
        assert!(super::allowed_uri("https://example.test/file"));
        assert!(super::allowed_uri("HTTP://example.test/file"));
        assert!(super::allowed_uri("mailto:person@example.test"));
        assert!(!super::allowed_uri("file:///secret"));
        assert!(!super::allowed_uri("javascript:alert(1)"));
        assert!(!super::allowed_uri("relative/path"));
    }

    use super::*;

    const PAPER: [u8; 3] = [0x26, 0x26, 0x2b];
    const INK: [u8; 3] = [0xd8, 0xd8, 0xdc];

    #[test]
    fn themed_tone_maps_paper_and_ink_and_keeps_hue() {
        let map = ToneMap::new(PAPER, INK);
        assert_eq!(map.apply([255, 255, 255]), PAPER);
        assert_eq!(map.apply([0, 0, 0]), INK);
        // Greys interpolate monotonically from ink to paper.
        let greys: Vec<u8> = (0..=255).map(|v| map.apply([v, v, v])[0]).collect();
        assert!(greys.windows(2).all(|w| w[1] <= w[0]));
        // Coloured ink stays its colour, lighter: red text and a blue link.
        let [r, g, b] = map.apply([200, 0, 0]);
        assert!(r > g + 80 && r > b + 80, "{r} {g} {b}");
        let [r, g, b] = map.apply([0, 0, 238]);
        assert!(b > r + 80 && b > g + 80 && r > 100, "{r} {g} {b}");
    }

    #[test]
    fn composite_flattens_alpha_onto_paper() {
        let map = ToneMap::new(PAPER, INK);
        // Transparent, opaque black, then half-covered black (premultiplied).
        let src = [0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 128];
        let mut out = [0u8; 12];
        composite(&src, &mut out, |c| c);
        assert_eq!(out, [255, 255, 255, 255, 0, 0, 0, 255, 127, 127, 127, 255]);
        composite(&src, &mut out, |c| map.apply(c));
        let bgr = |c: [u8; 3]| [c[2], c[1], c[0], 255];
        assert_eq!(out[..4], bgr(PAPER));
        assert_eq!(out[4..8], bgr(INK));
    }

    #[test]
    fn page_tone_converts_style_colors() {
        assert_eq!(page_tone(None), PageTone::Original);
        let themed = ThemedPages {
            paper: gpui::rgb(0x26262b).into(),
            ink: gpui::rgb(0xd8d8dc).into(),
            overlays: PageOverlays::default(),
        };
        assert_eq!(
            page_tone(Some(&themed)),
            PageTone::Themed {
                paper: PAPER,
                ink: INK
            }
        );
    }

    #[test]
    fn detects_pdf_extension() {
        assert!(is_pdf("a.pdf"));
        assert!(is_pdf("images/B.PDF"));
        assert!(!is_pdf("a.png"));
        assert!(!is_pdf("notapdf"));
    }

    // 10 US-Letter-ish pages (portrait): disp_h = 820 * 11/8.5 ≈ 1061.2 px each.
    fn letter_pages(n: usize) -> Vec<(f32, f32)> {
        vec![(8.5, 11.0); n]
    }

    #[test]
    fn window_at_top_covers_first_pages_plus_margin() {
        let dims = letter_pages(10);
        // Top of the document, ~900px viewport → page 0 visible, ±MARGIN(1).
        assert_eq!(keep_window(&dims, PAGE_WIDTH, 0.0, 900.0), (0, 1));
    }

    #[test]
    fn window_follows_scroll() {
        let dims = letter_pages(10);
        // Scrolled into page 2 (page tops ≈ 16, 1087, 2158, …).
        assert_eq!(keep_window(&dims, PAGE_WIDTH, 2200.0, 900.0), (1, 3));
    }

    #[test]
    fn window_clamps_at_the_end() {
        let dims = letter_pages(10);
        // Scrolled near the bottom: last pages, end clamped to the final index.
        let (start, end) = keep_window(&dims, PAGE_WIDTH, 9000.0, 900.0);
        assert_eq!(end, 9);
        assert!(start >= 5);
    }

    #[test]
    fn empty_doc_is_safe() {
        assert_eq!(keep_window(&[], PAGE_WIDTH, 0.0, 900.0), (0, 0));
    }

    #[test]
    fn current_page_tracks_scroll() {
        let dims = letter_pages(10);
        assert_eq!(current_page(&dims, PAGE_WIDTH, 0.0), 0);
        // Page tops ≈ 16, 1087, 2158; scrolled to 2200 sits in page 2.
        assert_eq!(current_page(&dims, PAGE_WIDTH, 2200.0), 2);
    }

    #[test]
    fn page_top_y_accumulates() {
        let dims = letter_pages(10);
        assert_eq!(page_top_y(&dims, PAGE_WIDTH, 0), PAGE_PAD_Y);
        // page 1 top = pad + one page height + gap.
        let expected = PAGE_PAD_Y + display_height((8.5, 11.0), PAGE_WIDTH) + PAGE_GAP;
        assert!((page_top_y(&dims, PAGE_WIDTH, 1) - expected).abs() < 0.01);
    }

    #[test]
    fn zoom_widens_layout() {
        let dims = letter_pages(3);
        // A wider column pushes later pages further down.
        let one = page_top_y(&dims, PAGE_WIDTH, 2);
        let two = page_top_y(&dims, PAGE_WIDTH * 2.0, 2);
        assert!(two > one);
    }

    #[test]
    fn render_scale_scales_with_dpi_quality_and_clamps() {
        // US-Letter (612pt) at base width, native (1×): ~1.34.
        let s = render_scale(PAGE_WIDTH, 1.0, 1.0, 612.0);
        assert!((s - 1.339).abs() < 0.01);
        // 2× display ratio doubles it.
        assert!((render_scale(PAGE_WIDTH, 2.0, 1.0, 612.0) - 2.0 * s).abs() < 0.01);
        // Quality multiplies too.
        assert!((render_scale(PAGE_WIDTH, 1.0, 2.0, 612.0) - 2.0 * s).abs() < 0.01);
        // Clamped at the top end.
        assert_eq!(render_scale(PAGE_WIDTH, 4.0, 3.0, 100.0), MAX_RENDER_SCALE);
        // Zero-width page falls back, never divides by zero.
        assert_eq!(render_scale(PAGE_WIDTH, 2.0, 1.0, 0.0), 1.5);
    }
}

#[cfg(test)]
mod perf_tests;

#[cfg(test)]
mod scrolling_tests {
    use super::*;
    fn draw(cx: &mut gpui::VisualTestContext) {
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            cx.run_until_parked();
        }
    }
    #[gpui::test]
    fn narrow_toolbar_exposes_and_wraps_pdf_tools(cx: &mut gpui::TestAppContext) {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
        let (view, cx) = cx.add_window_view(|_, cx| {
            PdfView::new(path, Rc::new(PdfStyle::default), Rc::new(|| 1.), cx)
        });
        cx.run_until_parked();
        for width in [320., 440., 900.] {
            cx.simulate_resize(gpui::size(px(width), px(480.)));
            draw(cx);
            let toolbar = cx.debug_bounds("pdf-toolbar").unwrap();
            let name = cx.debug_bounds("pdf-source-name").unwrap();
            assert!(name.bottom() <= toolbar.top());
            for selector in [
                "pdf-prev",
                "pdf-page-counter",
                "pdf-next",
                "pdf-zoom-out",
                "pdf-zoom-reset",
                "pdf-zoom-in",
                "pdf-fit-width",
                "pdf-fit-page",
            ] {
                let bounds = cx.debug_bounds(selector).unwrap();
                assert!(
                    toolbar.contains(&bounds.origin) && toolbar.contains(&bounds.bottom_right()),
                    "{selector} at {width}"
                );
            }
            assert!(cx.debug_bounds("pdf-find").is_some());
            // The highlight tools are gone (ADR 0031).
            for selector in ["pdf-mark", "pdf-area"] {
                assert!(cx.debug_bounds(selector).is_none());
            }
            assert!(cx.debug_bounds("pdf-tools").is_none());
            assert!(toolbar.bottom() < px(150.));
        }
        cx.simulate_resize(gpui::size(px(320.), px(480.)));
        draw(cx);
        let fit = cx.debug_bounds("pdf-fit-page").unwrap();
        cx.simulate_click(fit.center(), Default::default());
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(view.read(cx).fit_mode(), Some(FitMode::Page)));
        draw(cx);
        let zoom = cx.debug_bounds("pdf-zoom-in").unwrap();
        cx.simulate_click(zoom.center(), Default::default());
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(view.read(cx).fit_mode(), None));
        {
            draw(cx);
            let find = cx.debug_bounds("pdf-find").unwrap();
            cx.simulate_click(find.center(), Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| assert!(view.read(cx).search_open));
        }
    }

    #[gpui::test]
    fn page_tone_toggle_follows_style_and_rerenders(cx: &mut gpui::TestAppContext) {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
        let dark = Rc::new(std::cell::Cell::new(true));
        let style_dark = dark.clone();
        let (view, cx) = cx.add_window_view(move |_, cx| {
            let dark = style_dark.clone();
            let style = move || PdfStyle {
                themed_pages: dark.get().then(|| ThemedPages {
                    paper: gpui::rgb(0x26262b).into(),
                    ink: gpui::rgb(0xd8d8dc).into(),
                    overlays: PageOverlays::default(),
                }),
                ..PdfStyle::default()
            };
            PdfView::new(path, Rc::new(style), Rc::new(|| 1.), cx)
        });
        cx.simulate_resize(gpui::size(px(700.), px(600.)));
        draw(cx);
        let tone = |cx: &mut gpui::VisualTestContext| cx.update(|_, cx| view.read(cx).last_tone);
        assert!(matches!(tone(cx), PageTone::Themed { .. }));
        let rendered = cx.update(|_, cx| view.read(cx).render_requests);
        assert!(rendered > 0);

        let toggle = cx.debug_bounds("pdf-page-tone").unwrap();
        cx.simulate_click(toggle.center(), Default::default());
        draw(cx);
        assert!(cx.update(|_, cx| view.read(cx).shows_original_paper()));
        assert_eq!(tone(cx), PageTone::Original);
        assert!(cx.update(|_, cx| view.read(cx).render_requests) > rendered);

        // Light theme: no toggle, original paper; the override survives the trip.
        dark.set(false);
        draw(cx);
        assert!(cx.debug_bounds("pdf-page-tone").is_none());
        dark.set(true);
        draw(cx);
        assert!(cx.update(|_, cx| view.read(cx).shows_original_paper()));
        assert_eq!(tone(cx), PageTone::Original);
    }

    #[gpui::test]
    fn horizontal_gestures_thumb_and_page_navigation_preserve_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
        let (view, cx) = cx.add_window_view(|_, cx| {
            PdfView::new(path, Rc::new(PdfStyle::default), Rc::new(|| 1.), cx)
        });
        cx.simulate_resize(gpui::size(px(500.), px(600.)));
        cx.run_until_parked();
        view.update(cx, |view, cx| view.set_zoom(1.5, cx));
        draw(cx);
        let viewport = cx.update(|_, cx| view.read(cx).scroll.bounds());
        cx.update(|_, cx| assert!(view.read(cx).scroll.max_offset().x > px(100.)));
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: viewport.center(),
            delta: gpui::ScrollDelta::Pixels(point(px(-80.), px(0.))),
            modifiers: Default::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        draw(cx);
        cx.update(|_, cx| assert!(view.read(cx).scroll.offset().x < px(-10.)));
        let before = cx.update(|_, cx| view.read(cx).scroll.offset());
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: viewport.center(),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-60.))),
            modifiers: gpui::Modifiers {
                shift: true,
                ..Default::default()
            },
            touch_phase: gpui::TouchPhase::Moved,
        });
        draw(cx);
        cx.update(|_, cx| {
            let after = view.read(cx).scroll.offset();
            assert!(after.x < before.x);
            assert_eq!(after.y, before.y);
        });
        let thumb = cx
            .debug_bounds("pdf-horizontal-scrollbar")
            .expect("horizontal thumb");
        cx.simulate_mouse_down(thumb.center(), MouseButton::Left, Default::default());
        cx.simulate_mouse_move(
            thumb.center() + point(px(60.), px(0.)),
            Some(MouseButton::Left),
            Default::default(),
        );
        cx.simulate_mouse_move(
            thumb.center() + point(px(100.), px(0.)),
            Some(MouseButton::Left),
            Default::default(),
        );
        cx.simulate_mouse_up(
            thumb.center() + point(px(100.), px(0.)),
            MouseButton::Left,
            Default::default(),
        );
        draw(cx);
        cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
        view.update(cx, |v, cx| {
            assert!(v.scroll.offset().x < before.x - px(80.));
            assert_eq!(
                v.scroll.offset().y,
                before.y,
                "horizontal drag must not move the vertical thumb"
            );
            let x = v.scroll.offset().x;
            v.go_to_page(0, cx);
            assert_eq!(v.scroll.offset().x, x);
            {
                let cb = v.scroll.bounds_for_item(0).unwrap();
                let bounds = v.field_screen_bounds(0, (0.5, 0.2, 0.1, 0.1)).unwrap();
                assert_eq!(bounds.origin.x, cb.origin.x + x + cb.size.width * 0.5);
            }
        });
    }
}
