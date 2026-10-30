//! mdoc's **WYSIWYG** (live-preview) markdown editor — and, without a
//! [`SyntaxStyle`] installed, its **raw**-markdown editor. A from-scratch
//! multi-line text editor for GPUI.
//!
//! Host-agnostic — depends only on `gpui` (+ `unicode-segmentation`); no
//! `gpui-component`. Built directly on gpui's text primitives: an
//! [`EntityInputHandler`] for keyboard + IME input, `shape_line` for per-line
//! text shaping, and a custom [`Element`](gpui::Element) that lays out + paints the lines,
//! cursor, and selection. The editor **auto-grows** to its content height (no
//! inner scrollbar), so a host can stack many editors in one scroll view.
//! Editing fundamentals: cursor/selection, undo/redo, IME, soft-wrap,
//! clipboard, spell-check diagnostics (squiggles + suggestion menu).
//!
//! WYSIWYG mode is [`EditorState::set_markdown_style`] plus the block
//! providers (`set_block_image_provider` & co). Comments reference its
//! feature milestones by code:
//!
//! - **W1** — inline styling: bold/italic/strike/code/links/wiki-links/tags,
//!   markers dimmed in place (`markdown_syntax::scan_line`).
//! - **W2** — heading font sizes (variable per-line heights).
//! - **W4** — block widgets: **W4a** inline images, **W4b** fenced code
//!   blocks, **W4c** tables (Word-style editing); mermaid + `$$math$$`
//!   rasters ride the same widget path.
//! - **W6** — marker *hiding* with reveal-on-caret: the painted text drops
//!   the syntax markers, and per-row offset maps translate display ↔ source.
//!
//! Usage: create an [`EditorState`] entity and render it; call [`bind_keys`]
//! once at startup so the editing actions resolve while it's focused.

use std::ops::Range;
use std::sync::Arc;

use gpui::{
    App, Bounds, Context, CursorStyle, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    Font, FontWeight, Hitbox, Hsla, InteractiveElement, IntoElement, KeyBinding, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, PathBuilder, Pixels, Point,
    Render, RenderImage, ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Task,
    TextRun, Window, WrappedLine, actions, div, point, px, rgb, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

mod markdown_syntax;
mod syntax;
pub use markdown_syntax::{Heading, SyntaxStyle, headings};

mod search;
mod search_geometry;
pub use search::{SearchIndex, SearchMatch, SearchOptions};
mod spell_exclusions;
pub use spell_exclusions::spell_exclusions;

mod input;
use input::{EditKind, Snapshot};
pub use mdoc_history::{EditorTransaction, HistoryPath};
use mdoc_history::{HistoryChange, SourceEdit};
mod transactions;

mod tables;
use tables::{TableColResize, TableThumb};

mod element;
use element::EditorElement;

/// Key context the editing actions are scoped to (so they only fire while an
/// editor is focused).
const CONTEXT: &str = "Editor";

actions!(
    mdoc_editor,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        Home,
        End,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Newline,
        LineBreak,
        Paste,
        Copy,
        Cut,
        ShowCharacterPalette,
        Undo,
        Redo,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        NextWordStart,
        SelectToNextWordStart,
        DeleteWordLeft,
        DeleteWordRight,
        DocumentStart,
        DocumentEnd,
        SelectToDocumentStart,
        SelectToDocumentEnd,
        LineStart,
        LineEnd,
        SelectToLineStart,
        SelectToLineEnd,
        DeleteToLineStart,
        DeleteToWordEnd,
        Indent,
        Outdent,
        Bold,
        Italic,
        Underline,
        Strike,
        Code,
        Dismiss,
    ]
);

/// Bind the editor's editing keys. Call once at startup. Bindings are scoped to
/// the editor's key context, so they don't shadow the host's shortcuts.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(key_bindings(cfg!(target_os = "macos")));
}

/// The editor's key bindings on macOS (`mac`) or Windows and Linux. Taking the
/// platform as an argument lets tests on any OS check both sets (ADR 0061).
pub fn key_bindings(mac: bool) -> Vec<KeyBinding> {
    let ctx = Some(CONTEXT);
    let mut bindings = vec![
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("up", Up, ctx),
        KeyBinding::new("down", Down, ctx),
        KeyBinding::new("home", Home, ctx),
        KeyBinding::new("end", End, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("shift-up", SelectUp, ctx),
        KeyBinding::new("shift-down", SelectDown, ctx),
        KeyBinding::new("enter", Newline, ctx),
        KeyBinding::new("shift-enter", LineBreak, ctx),
        KeyBinding::new("tab", Indent, ctx),
        KeyBinding::new("shift-tab", Outdent, ctx),
        KeyBinding::new("cmd-a", SelectAll, ctx),
        KeyBinding::new("ctrl-a", SelectAll, ctx),
        KeyBinding::new("cmd-c", Copy, ctx),
        KeyBinding::new("ctrl-c", Copy, ctx),
        KeyBinding::new("cmd-v", Paste, ctx),
        KeyBinding::new("ctrl-v", Paste, ctx),
        KeyBinding::new("cmd-x", Cut, ctx),
        KeyBinding::new("ctrl-x", Cut, ctx),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, ctx),
        KeyBinding::new("cmd-z", Undo, ctx),
        KeyBinding::new("ctrl-z", Undo, ctx),
        KeyBinding::new("cmd-shift-z", Redo, ctx),
        KeyBinding::new("ctrl-shift-z", Redo, ctx),
        KeyBinding::new("ctrl-y", Redo, ctx),
        KeyBinding::new("alt-left", WordLeft, ctx),
        KeyBinding::new("alt-right", WordRight, ctx),
        KeyBinding::new("alt-shift-left", SelectWordLeft, ctx),
        KeyBinding::new("alt-shift-right", SelectWordRight, ctx),
        KeyBinding::new("cmd-b", Bold, ctx),
        KeyBinding::new("ctrl-b", Bold, ctx),
        KeyBinding::new("cmd-i", Italic, ctx),
        KeyBinding::new("ctrl-i", Italic, ctx),
        KeyBinding::new("cmd-e", Code, ctx),
        KeyBinding::new("ctrl-e", Code, ctx),
        KeyBinding::new("cmd-shift-x", Strike, ctx),
        KeyBinding::new("ctrl-shift-x", Strike, ctx),
        KeyBinding::new("cmd-u", Underline, ctx),
        KeyBinding::new("ctrl-u", Underline, ctx),
        KeyBinding::new("escape", Dismiss, ctx),
    ];
    if mac {
        // macOS caret moves: ⌘←/→ to the visual line's ends, ⌘↑/↓ to the
        // document's; ⇧ selects. ⌥⌫/⌥⌦ delete a word, ⌘⌫ to the line start.
        bindings.extend([
            KeyBinding::new("cmd-left", LineStart, ctx),
            KeyBinding::new("cmd-right", LineEnd, ctx),
            KeyBinding::new("cmd-shift-left", SelectToLineStart, ctx),
            KeyBinding::new("cmd-shift-right", SelectToLineEnd, ctx),
            KeyBinding::new("cmd-up", DocumentStart, ctx),
            KeyBinding::new("cmd-down", DocumentEnd, ctx),
            KeyBinding::new("cmd-shift-up", SelectToDocumentStart, ctx),
            KeyBinding::new("cmd-shift-down", SelectToDocumentEnd, ctx),
            KeyBinding::new("alt-backspace", DeleteWordLeft, ctx),
            KeyBinding::new("alt-delete", DeleteToWordEnd, ctx),
            KeyBinding::new("cmd-backspace", DeleteToLineStart, ctx),
        ]);
    } else {
        // Windows and Linux caret moves; macOS keeps ⌥ for words.
        bindings.extend([
            KeyBinding::new("ctrl-left", WordLeft, ctx),
            KeyBinding::new("ctrl-right", NextWordStart, ctx),
            KeyBinding::new("ctrl-shift-left", SelectWordLeft, ctx),
            KeyBinding::new("ctrl-shift-right", SelectToNextWordStart, ctx),
            KeyBinding::new("ctrl-backspace", DeleteWordLeft, ctx),
            KeyBinding::new("ctrl-delete", DeleteWordRight, ctx),
            KeyBinding::new("ctrl-home", DocumentStart, ctx),
            KeyBinding::new("ctrl-end", DocumentEnd, ctx),
            KeyBinding::new("ctrl-shift-home", SelectToDocumentStart, ctx),
            KeyBinding::new("ctrl-shift-end", SelectToDocumentEnd, ctx),
        ]);
    }
    bindings
}

/// Line height as a multiple of the font size. Derived from the editor's own
/// font (not the ambient `window.line_height()`, which tracks the host's UI text
/// style and would leave the caret/rows mismatched against differently-sized
/// editor text). 1.45 for comfortable reading density while typing (1.25 felt
/// cramped, especially stacking several list rows). Public so a host's scroll
/// math (e.g. mdoc's click-to-edit caret prediction) can mirror row heights.
const LINE_HEIGHT_RATIO: f32 = 1.45;

/// Extra height under each list/task row in WYSIWYG, matching the reader's
/// roomier item gap (its list column uses a 4px inter-item gap) — the one
/// place the reader's look wins over the editor's (see AGENTS.md "Parity
/// direction"). Detected from the raw line, so it's caret-stable.
const LIST_ROW_GAP: f32 = 4.;
/// The gap between a painted bullet/checkbox and its item text — the reader's
/// 8px marker gap, whose roomier indent wins (AGENTS.md "Parity direction").
const LIST_TEXT_GAP: f32 = 8.;
/// Per-space width (px) of one nesting level's indent, matching the reader's
/// `list_indent` sizing (`spaces × 4.5`). A level therefore advances by
/// bullet + gap + this — noticeably wider than the raw source spaces, so the
/// display shifts on reveal-on-caret (the quote inset already set that
/// precedent, just smaller).
const LIST_LEVEL_PER_SPACE: f32 = 4.5;

/// Caret thickness (px) — thin like a native text caret, so it doesn't blend into
/// the first glyph at the start of a line/cell.
const CARET_WIDTH: f32 = 1.0;

/// Horizontal inset (px) of fenced-code-block text from the box's left edge, so
/// code sits inside the padded box rather than flush against it. Mirrors the old
/// renderer's `px(12)` left padding.
const CODE_INSET: f32 = 12.;

/// Vertical padding (px) above the first / below the last line of a fenced code
/// block. Reserved as layout space (a gap in the line tops + total height) so the
/// box doesn't overlap adjacent lines, with no blank line required.
const CODE_PAD: f32 = 8.;

/// Horizontal inset (px) of blockquote text from the editor's left edge, leaving
/// room for the left border (2px) + a gap, matching the reading view's `pl(12)`.
const QUOTE_INSET: f32 = 14.;

/// Vertical padding (px) inside a file chip (e.g. a PDF embed), above + below its
/// label, so the chip box reads as a button rather than a bare line of text.
const CHIP_PAD: f32 = 5.;

/// Total vertical breathing room (px) reserved around an inline image — split
/// above + below — so consecutive images (a bulleted photo list) don't touch.
const IMG_ROW_PAD: f32 = 12.;

/// Extra height (px) a text row gets beyond its tallest inline `$…$` formula, so a fraction
/// has a little breathing room above + below instead of touching the neighbouring rows.
const INLINE_MATH_ROW_PAD: f32 = 6.;

/// Side length (px) of the square drag-to-resize grip painted at an inline
/// image's bottom-right corner (matching the reading view's 14px handle).
const IMG_GRIP: f32 = 14.;

/// Smallest width (px) a drag may shrink an inline image to, so it can't vanish.
const IMG_MIN_W: f32 = 40.;

/// An in-progress drag of an inline image's corner grip: which logical line's
/// `![](src)` is being resized, its display width when the drag began, the
/// pointer x at grab, and the live (preview) width the drag has reached. The
/// image paints at `width` (aspect-preserved) until release writes `{width=N}`.
#[derive(Clone, Copy)]
struct ImageResize {
    line: usize,
    start_width: f32,
    start_x: Pixels,
    width: f32,
}

/// A flagged span (e.g. a misspelling) to underline. The host (e.g. a spell
/// checker) computes these and feeds them in via [`EditorState::set_diagnostics`].
/// Replacement suggestions are fetched lazily when the user right-clicks the
/// span, via the provider set with [`EditorState::on_suggest`] — so detection
/// can stay cheap and run on every edit.
#[derive(Clone)]
pub struct Diagnostic {
    /// Byte range in the document.
    pub range: Range<usize>,
}

/// An open right-click suggestions menu for a diagnostic.
#[derive(Clone)]
struct DiagMenu {
    /// Popup top-left, in window space (rendered on a deferred/anchored layer).
    anchor: Point<Pixels>,
    /// The diagnostic's byte range, replaced when a suggestion is chosen.
    range: Range<usize>,
    suggestions: Vec<SharedString>,
    /// Suggestions are still being computed by the host (off the UI thread).
    suggesting: bool,
    /// Scroll state of the (capped-height) list, so a thumb can track it.
    scroll: ScrollHandle,
    /// Whether the "Turn into" flyout is open (hover-opened; dies with the menu).
    turn_into: bool,
}

/// A block kind the right-click "Turn into" menu converts between —
/// flat-markdown natural: each conversion is a line-prefix rewrite (fenced
/// kinds wrap/unwrap the block's lines).
#[derive(Clone, Copy, PartialEq, Eq)]
enum TurnKind {
    Text,
    H1,
    H2,
    H3,
    Bullet,
    Numbered,
    Todo,
    Quote,
    Callout,
    Code,
    Math,
}

impl TurnKind {
    const ALL: [TurnKind; 11] = [
        TurnKind::Text,
        TurnKind::H1,
        TurnKind::H2,
        TurnKind::H3,
        TurnKind::Bullet,
        TurnKind::Numbered,
        TurnKind::Todo,
        TurnKind::Quote,
        TurnKind::Callout,
        TurnKind::Code,
        TurnKind::Math,
    ];

    fn label(self) -> SharedString {
        match self {
            TurnKind::Text => SharedString::new_static("Text"),
            TurnKind::H1 => SharedString::new_static("Heading 1"),
            TurnKind::H2 => SharedString::new_static("Heading 2"),
            TurnKind::H3 => SharedString::new_static("Heading 3"),
            TurnKind::Bullet => SharedString::new_static("Bulleted list"),
            TurnKind::Numbered => SharedString::new_static("Numbered list"),
            TurnKind::Todo => SharedString::new_static("To-do"),
            TurnKind::Quote => SharedString::new_static("Quote"),
            TurnKind::Callout => SharedString::new_static("Callout"),
            TurnKind::Code => SharedString::new_static("Code block"),
            TurnKind::Math => SharedString::new_static("Math block"),
        }
    }
}

/// If `offset` sits on a collapsed marker line (a table style marker or a
/// math align marker), the offset of the nearest line that reveals nothing
/// when the caret rests there: the table's header, or the line after the
/// math block. Otherwise `offset` unchanged.
fn caret_off_marker_line(content: &str, offset: usize) -> usize {
    let row = content[..offset.min(content.len())].matches('\n').count();
    let line_start = |r: usize| {
        let mut off = 0;
        for (i, l) in content.split('\n').enumerate() {
            if i == r {
                return off;
            }
            off += l.len() + 1;
        }
        content.len()
    };
    if let Some(t) = markdown_syntax::table_regions(content)
        .iter()
        .find(|t| t.marker_line == Some(row))
    {
        return line_start(t.lines.start);
    }
    if let Some(m) = markdown_syntax::math_regions(content)
        .iter()
        .find(|m| m.marker_line == Some(row))
    {
        // Anywhere inside a math region reveals it whole — land after it.
        return line_start(m.range.end);
    }
    offset
}

/// Strip a line's block dressing (heading hashes, list/todo bullet, ordered
/// number, quote `>`), leaving the text a "Turn into" conversion re-prefixes.
fn strip_block_prefix(line: &str) -> &str {
    // Composes the renderer's own recognizers so the strip grammar can't
    // drift from what WYSIWYG classifies (task/list/heading/quote).
    if let Some((p, ..)) = markdown_syntax::task_prefix(line) {
        return &line[p..];
    }
    if let Some((p, ..)) = markdown_syntax::list_prefix(line) {
        return &line[p..];
    }
    if let Some(n) = markdown_syntax::heading_level(line) {
        let after = &line[n as usize..];
        return after.strip_prefix(' ').unwrap_or(after);
    }
    if let Some(p) = markdown_syntax::blockquote_prefix(line) {
        return &line[p..];
    }
    line.trim_start()
}

/// Join `body` lines each carrying the prefix `p(index)` produces — the
/// assembly half of a "Turn into" conversion.
fn prefix_lines(body: &[String], p: impl Fn(usize) -> String) -> String {
    body.iter()
        .enumerate()
        .map(|(i, l)| format!("{}{l}", p(i)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A column edit applied to every row of a table (insert/delete a cell at index).
#[derive(Clone, Copy)]
enum ColEdit {
    Insert(usize),
    Delete(usize),
}

/// An item in the table right-click menu (Word-style table editing).
#[derive(Clone, Copy)]
enum TableMenuAction {
    InsertRowAbove,
    InsertRowBelow,
    DuplicateRow,
    InsertColLeft,
    InsertColRight,
    DeleteRow,
    DeleteColumn,
    AlignLeft,
    AlignCenter,
    AlignRight,
    /// Rewrite the table's `<!-- table:STYLE -->` marker (`None` = the
    /// default Grid, which has no marker).
    SetStyle(Option<&'static str>),
    CopyTable,
    DeleteTable,
}

impl TableMenuAction {
    fn apply(self, editor: &mut EditorState, cx: &mut Context<EditorState>) {
        match self {
            TableMenuAction::InsertRowAbove => editor.insert_table_row(false, cx),
            TableMenuAction::InsertRowBelow => editor.insert_table_row(true, cx),
            TableMenuAction::DuplicateRow => editor.duplicate_table_row(cx),
            TableMenuAction::InsertColLeft => editor.insert_table_column(false, cx),
            TableMenuAction::InsertColRight => editor.insert_table_column(true, cx),
            TableMenuAction::DeleteRow => editor.delete_table_row(cx),
            TableMenuAction::DeleteColumn => editor.delete_table_column(cx),
            TableMenuAction::AlignLeft => editor.set_caret_table_align(CellAlign::Left, cx),
            TableMenuAction::AlignCenter => editor.set_caret_table_align(CellAlign::Center, cx),
            TableMenuAction::AlignRight => editor.set_caret_table_align(CellAlign::Right, cx),
            TableMenuAction::SetStyle(name) => editor.set_table_style(name, cx),
            TableMenuAction::CopyTable => editor.copy_table(cx),
            TableMenuAction::DeleteTable => editor.delete_table(cx),
        }
    }
}

/// Events the editor emits so a host can react. Subscribe with
/// `cx.subscribe(&editor, …)` — e.g. to re-run spell-check after an edit.
/// Host-owned source annotation, independent of Markdown find. Byte ranges
/// are valid only for the revision passed to `set_annotations`.
#[derive(Clone, Debug)]
pub struct SourceAnnotation {
    pub id: u64,
    pub range: Range<usize>,
    pub color: Hsla,
    pub active_color: Hsla,
    /// Chip border; transparent paints a plain fill.
    pub border: Hsla,
    /// Glyph colour over the range; `None` keeps the syntax colour.
    pub text_color: Option<Hsla>,
    /// An underline under an unbordered fill, e.g. a category's colour.
    pub underline: Option<Hsla>,
    /// Draw `underline` as a quiet 1 px dotted line, without the fill band.
    pub dotted: bool,
}

/// A transient colour change on annotations, e.g. a replacement settling in.
/// The fill and border ease from `from` through `peak` to the annotation's own
/// colours over `duration`; `mark` (e.g. ✓ or ↶) rises and fades in beside
/// each annotation, then fades out.
#[derive(Clone, Debug)]
pub struct AnnotationFlash {
    pub ids: std::collections::HashSet<u64>,
    /// (fill, border) at the start.
    pub from: (Hsla, Hsla),
    /// (fill, border) reached at `PEAK` of the duration.
    pub peak: (Hsla, Hsla),
    pub duration: std::time::Duration,
    pub mark: Option<(SharedString, Hsla)>,
}

/// Text that just changed rolls inside its chip: the previous words drift up
/// and fade out while the current words rise into place. Display only; the
/// document already holds the new text.
#[derive(Clone, Debug)]
pub struct TextRoll {
    /// Content revision the ranges belong to; stale ranges never paint.
    pub revision: u64,
    /// (range of the new text, the text it replaced).
    pub items: Vec<(Range<usize>, SharedString)>,
    /// The page colour behind the text, used to mask it mid-roll.
    pub surface: Hsla,
    pub duration: std::time::Duration,
}

/// A chip-shaped ghost over source ranges that fades away, for text that just
/// lost its annotation (e.g. a replacement reverted to its original).
#[derive(Clone, Debug)]
pub struct RangeFlash {
    /// Content revision the ranges belong to; stale ranges never paint.
    pub revision: u64,
    /// (range, fill, border) at the start; all fade to transparent.
    pub ranges: Vec<(Range<usize>, Hsla, Hsla)>,
    pub duration: std::time::Duration,
}

/// A host action offered for one selection: a floating pill beside it and the
/// first right-click item. Shown only while `range` is exactly the selection.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionAction {
    pub range: Range<usize>,
    /// Pill label, e.g. "Replace".
    pub label: SharedString,
    /// Right-click item label, e.g. "Replace with placeholder".
    pub menu_label: SharedString,
    pub shortcut: SharedString,
    /// Why the action is unavailable; the pill and item show disabled.
    pub disabled: Option<SharedString>,
    /// Hover tint for the pill.
    pub accent: Hsla,
    /// The pill's second part, e.g. "Person ▾": what the action will use,
    /// which opens the host's chooser ([`EditorEvent::SelectionActionChoice`]).
    pub choice: Option<SelectionChoice>,
}

/// The split part of a [`SelectionAction`] pill.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionChoice {
    /// The current choice, e.g. "Person"; the pill adds "▾".
    pub label: SharedString,
    /// Its colour, as a dot before the label.
    pub accent: Hsla,
    /// The host's chooser is open: the pill stays, pressed, while the
    /// chooser holds focus (or after Escape hid it).
    pub open: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EditorEvent {
    /// Exact edits and history travel, emitted before the legacy Changed event.
    Transaction(Arc<EditorTransaction>),
    /// The document text changed via a user edit (typing, delete, paste, IME,
    /// applying a suggestion). Not emitted for programmatic `set_text`.
    Changed,
    /// A file chip (e.g. a PDF embed) or an inline `[text](url)` link was
    /// left-clicked — the host opens the `src`/url (http externally, files
    /// via its own resolution). A navigation hint; the text is untouched.
    OpenLink(SharedString),
    /// The caret / selection moved without a text change — so a host can update a
    /// caret-anchored affordance (e.g. the table-alignment toolbar). Also sent
    /// when a mouse selection drag ends.
    SelectionChanged,
    /// The host's [`SelectionAction`] was chosen from its pill or menu item.
    SelectionAction,
    /// The pill's [`SelectionChoice`] part was pressed: the host opens (or
    /// closes) its chooser at [`EditorState::selection_choice_bounds`].
    SelectionActionChoice,
    /// Explicit activation of a host annotation; no text or selection change.
    ActivateAnnotation(u64),
    /// The pointer moved onto (or off) a host annotation, e.g. for a tooltip.
    HoverAnnotation(Option<u64>),
    /// A middle click on a host annotation, with the modifiers held.
    MiddleClickAnnotation(u64, gpui::Modifiers),
    /// Several hidden fields share a gutter indicator; the host offers a chooser.
    ActivateAnnotations(Vec<u64>),
    /// One of the host's diagnostic actions (see
    /// [`EditorState::set_diagnostic_actions`]) was chosen for a flagged word:
    /// the action's index and the word. The text is untouched.
    DiagnosticAction(usize, String),
}

/// A table column's text alignment, for the host-driven alignment toolbar
/// ([`EditorState::caret_table_align`] / [`EditorState::set_caret_table_align`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CellAlign {
    Left,
    Center,
    Right,
}

/// Provides replacement suggestions for a flagged word (best first); set by the
/// host via [`EditorState::on_suggest`] and consulted on right-click. Returns
/// a task so the host can compute them off the UI thread.
type SuggestFn = Box<dyn Fn(&str, &mut App) -> Task<Vec<String>>>;

/// Resolves a standalone image line's `src` to a decoded image so the editor can
/// render it inline (W4). Set by the host via
/// [`EditorState::set_block_image_provider`]; the host owns loading + caching and
/// returns `None` while still decoding / on failure (the line shows raw source).
type BlockImageFn = Box<dyn Fn(&str) -> Option<Arc<RenderImage>>>;

/// Classifies an `![](src)` reference as a file chip (e.g. a PDF) rather than an
/// image, returning its display label. Set via
/// [`EditorState::set_block_chip_provider`]; the editor renders such a line as a
/// clickable chip (left-click emits [`EditorEvent::OpenLink`]).
type BlockChipFn = Box<dyn Fn(&str) -> Option<SharedString>>;

pub struct EditorState {
    /// Shown for reading only: no user edit changes the text. The host can
    /// still replace it (`set_text`, `replace_ranges`).
    read_only: bool,
    /// A plain-text field (see [`Self::set_plain_text`]).
    plain: bool,
    focus_handle: FocusHandle,
    /// The whole document, newline-separated. Byte offsets index into this.
    content: String,
    placeholder: SharedString,
    /// Selection as a byte range; the caret is one end (see [`Self::cursor_offset`]).
    selected_range: Range<usize>,
    selection_reversed: bool,
    /// IME composition range, if any.
    marked_range: Option<Range<usize>>,
    /// Host-supplied clipboard writer for Copy/Cut (e.g. adding an HTML
    /// flavor beside the plain text). `None` = gpui's plain-string copy.
    /// Find highlights: logical occurrences + the active index, painted behind
    /// the text like the selection. A logical occurrence can contain several
    /// source ranges when hidden Markdown syntax lies inside it.
    search: Option<(Vec<SearchMatch>, Option<usize>)>,
    search_bounds: Vec<Option<Bounds<Pixels>>>,
    annotations: Vec<SourceAnnotation>,
    annotation_revision: u64,
    /// Display-only outlined source ranges (e.g. a review scope), valid for
    /// `outline_revision`.
    outlines: Vec<(Range<usize>, Hsla)>,
    outline_revision: u64,
    annotation_ends: Vec<usize>,
    position_cache:
        std::cell::RefCell<std::collections::HashMap<usize, search_geometry::CachedLinePositions>>,
    #[cfg(test)]
    visible_link_hitboxes: usize,
    hidden_annotation_ids: std::collections::HashSet<u64>,
    annotation_bounds: Vec<(u64, Bounds<Pixels>)>,
    annotation_hover: Option<u64>,
    annotation_active: Option<u64>,
    annotation_flashes: Vec<(std::time::Instant, AnnotationFlash)>,
    range_flashes: Vec<(std::time::Instant, RangeFlash)>,
    text_rolls: Vec<(std::time::Instant, TextRoll)>,
    /// Last paint's wrapped lines (one per logical line) and each line's top
    /// offset relative to the editor's top — both used for hit-testing and
    /// cursor/IME positioning.
    wrapped: Vec<ShapedLine>,
    line_tops: Vec<Pixels>,
    /// Per-logical-line wrap-row count (from the last paint). Geometry reads
    /// this, not `wrapped[i].wrap_boundaries()` — a windowed-out line's entry
    /// in `wrapped` is an empty placeholder.
    wrap_rows: Vec<usize>,
    /// Per-logical-line wrap-row height. Variable so a heading (bigger font) gets
    /// a taller row (W2); `line_height` is the base/fallback for the empty doc
    /// and any row without a recorded height.
    line_heights: Vec<Pixels>,
    /// Per-logical-line table-grid row (from the last paint), so a click /
    /// Tab / caret hit-tests against cells instead of the raw source line.
    table_rows: Vec<Option<TableRow>>,
    /// Hover-revealed "+" add-row / add-column strips for each table (issue #16),
    /// each paired with the table row to seat the caret in before inserting. From
    /// the last paint, committed only while the table is hovered; hit-tested on
    /// mouse-down.
    table_row_add_rects: Vec<(Bounds<Pixels>, usize)>,
    table_col_add_rects: Vec<(Bounds<Pixels>, usize, usize)>,
    /// Each table's hover zone (grid + a thin margin) with its header row,
    /// committed every paint so `on_mouse_move` can repaint when the pointer's
    /// table-affordance region changes (the editor otherwise only repaints on
    /// the caret blink) and `on_scroll_wheel` can hit-test the PAINTED table.
    table_hover_zones: Vec<(Bounds<Pixels>, usize)>,
    /// The affordance region the pointer was last in — `(table index, 0 = zone /
    /// 1 = below strip / 2 = right strip)` — so the repaint fires only on change.
    table_hover_region: Option<(usize, u8)>,
    /// Committed delete-handle rects (issue #16): the hovered row's "−" `(bounds,
    /// row)` and the hovered column's "−" `(bounds, row, col)`, hit-tested on click.
    table_row_del: Option<(Bounds<Pixels>, usize)>,
    table_col_del: Option<(Bounds<Pixels>, usize, usize)>,
    /// The table cell `(row, col)` the pointer was last over, so `on_mouse_move`
    /// repaints the delete handles when it changes.
    table_hover_cell: Option<(usize, usize)>,
    /// Per-logical-line flag: this row is painted as an inline image (W4), so a
    /// click on it places the caret at the line start instead of hit-testing
    /// source text. From the last paint.
    widget_rows: Vec<bool>,
    /// Per-logical-line display→source byte map for rows with hidden markers
    /// (W6); `None` when the painted text equals the source. From the last paint.
    offset_maps: Vec<Option<std::rc::Rc<Vec<usize>>>>,
    /// Per-logical-line horizontal text inset (and so the caret/selection/hit-test
    /// inset): non-zero for fenced code blocks and gutter marks (blockquotes,
    /// lists). From the last paint.
    line_insets: Vec<Pixels>,
    /// Per-logical-line right-to-left geometry (#66), `None` on every LTR row
    /// so an LTR document pays nothing. From the last paint. See [`RtlRow`].
    rtl_rows: Vec<Option<RtlRow>>,
    last_bounds: Option<Bounds<Pixels>>,
    line_height: Pixels,
    /// The host scroll view this editor keeps its caret visible in (ADR 0054).
    autoscroll: Option<ScrollHandle>,
    /// A keyboard change moved the caret: reveal it after the next paint.
    reveal_caret: bool,
    /// Scrolls while a mouse selection is held near a viewport edge.
    drag_autoscroll: Option<Task<()>>,
    /// Font size from the last paint. Hit-testing that runs during event
    /// dispatch (e.g. table-cell clicks) must measure at this size — the
    /// window's text-style stack is unwound there, so `window.text_style()`
    /// would report the root size, not the host wrapper's.
    font_size: Pixels,
    /// The font of the last paint, for the same reason: event-time
    /// `text_style()` reports the ROOT font, whose family/metrics can differ
    /// from what the table was painted with (column auto-fit measured with
    /// the wrong glyph widths and wrapped cells).
    paint_font: Option<Font>,
    is_selecting: bool,
    /// The word a double-click selected: dragging from it extends the
    /// selection word by word, like ⌥⇧←/→, and always keeps this word.
    word_drag: Option<Range<usize>>,
    /// Host action for the current selection; see [`SelectionAction`].
    selection_action: Option<SelectionAction>,
    /// Escape hid the pill for this exact selection.
    selection_action_dismissed: Option<Range<usize>>,
    /// Window-space selection quads from the last paint (pill anchor).
    selection_bounds: Vec<Bounds<Pixels>>,
    /// Window-space bounds of the pill's choice part from the last paint.
    selection_choice_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    history_id: u64,
    next_history_id: u64,
    pending_changes: Vec<HistoryChange>,
    last_transaction: Option<Arc<EditorTransaction>>,
    last_edit: EditKind,
    /// Whether the last content edit was a single typed grapheme or a single-char
    /// backspace — the only edits auto-pairing should react to, so programmatic /
    /// structural edits (table ops, etc.) don't trip it.
    last_edit_keystroke: bool,
    /// The target x for vertical (Up/Down) movement, so the caret keeps its
    /// column across short lines. `Some` only during a run of Up/Down.
    goal_x: Option<Pixels>,
    /// Upstream caret affinity (ADR 0061): the caret offset and content
    /// generation it was set for. A caret on a soft-wrap boundary draws at
    /// the upper row's end while this matches, else at the lower row's
    /// start; any other caret move or edit drops it.
    upstream_caret: Option<(usize, u64)>,
    /// The in-cell x that Up/Down through a table cell's rows keep, with the
    /// caret offset and content generation it is valid for. Only a vertical
    /// step sets it; any other caret move, a click or an edit drops it.
    cell_goal: Option<(usize, u64, Pixels)>,
    /// Spans to underline (misspellings, etc.), set by the host via
    /// [`Self::set_diagnostics`].
    diagnostics: Vec<Diagnostic>,
    /// Inline-markdown styling palette; `Some` = the WYSIWYG (live-preview)
    /// view (W1), `None` = the raw view (plain text). Set by the host via
    /// [`Self::set_markdown_style`].
    markdown_style: Option<SyntaxStyle>,
    /// The open right-click suggestions menu, if any.
    menu: Option<DiagMenu>,
    /// The open table right-click menu's anchor (window space), if any. Its actions
    /// operate on the caret's table cell.
    table_menu: Option<Point<Pixels>>,
    /// Scroll state for the table menu, so its overflow scrolls + shows a thumb.
    table_menu_scroll: ScrollHandle,
    /// The open image right-click menu, if any: the image's logical line + the
    /// menu's anchor (window space). Offers Word-style object actions (Delete).
    image_menu: Option<(usize, Point<Pixels>)>,
    /// Supplies replacement suggestions for a flagged word, fetched lazily when
    /// the user right-clicks it. Set by the host via [`Self::on_suggest`];
    /// without it, the right-click menu has nothing to offer.
    suggest: Option<SuggestFn>,
    /// Host actions offered under a flagged word's suggestions (e.g. Ignore).
    diagnostic_actions: Vec<SharedString>,
    /// The pending suggestion lookup for the open menu; dropping it cancels.
    suggest_task: Option<Task<()>>,
    /// Resolves a standalone image line's `src` to a decoded image for inline
    /// rendering (W4); set by the host via [`Self::set_block_image_provider`].
    block_image: Option<BlockImageFn>,
    /// Classifies an `![](src)` as a file chip (e.g. a PDF) + its label; set by
    /// the host via [`Self::set_block_chip_provider`].
    block_chip: Option<BlockChipFn>,
    /// Resolves a ` ```mermaid ` block's source to a rendered diagram; set by the
    /// host via [`Self::set_block_mermaid_provider`].
    /// Resolves a `$$…$$` block's LaTeX to a typeset equation; set by the host via
    /// [`Self::set_block_math_provider`].
    /// Fenced-code syntax highlighter, see [`CodeHighlightFn`].
    /// Host auto-replace hook, see [`Self::set_auto_replace`].
    /// What the most recent keystroke edit replaced (the selected text), for
    /// the host's auto-pair logic — a text diff alone can't distinguish
    /// "typed `[` over a selection starting with `[`" from "backspaced inside
    /// a doubled pair". Consumed via [`Self::take_replaced_selection`].
    /// The em (px/font-size) the `block_math` provider rasterizes at — set via
    /// [`Self::set_block_math_em`]. Inline `$…$` formulas reuse those rasters scaled by
    /// `text_em / this`, so they sit at text size. `None` disables inline math rendering.
    /// Per-logical-line `src` for rows painted as a file chip (from the last
    /// paint), so a left-click can open it and a right-click can edit it.
    chip_rows: Vec<Option<SharedString>>,
    /// Window-space painted bounds of each inline image, with its logical line
    /// index (from the last paint), so a press near a corner can start a resize
    /// and know which `![](src)` line to rewrite. One entry per rendered image.
    image_rects: Vec<(usize, Bounds<Pixels>)>,
    /// Window-space bounds of each painted task checkbox, with its logical line —
    /// so a click on the box toggles `[ ]`↔`[x]` instead of placing the caret.
    checkbox_rects: Vec<(usize, Bounds<Pixels>)>,
    /// Painted code-card chrome bounds from the last frame (lang tag + Copy per
    /// code block, keyed by the opening-fence row) — clicks route here before
    /// caret placement.
    code_chip_rects: Vec<CodeChipHit>,
    /// Full card bounds of each code block from the last paint (`(first body
    /// line, rect)`), for hover tracking — the chrome is hover-revealed.
    code_card_rects: Vec<(usize, Bounds<Pixels>)>,
    /// The hovered code block's first body line, if any (chrome shows there).
    code_chip_hover: Option<usize>,
    /// Painted chevron bounds of foldable callouts (`(line, rect)`, from the
    /// last paint) — a click flips the marker's `-`/`+` fold char.
    alert_fold_rects: Vec<(usize, Bounds<Pixels>)>,
    /// The in-progress corner-grip drag, if any (see [`ImageResize`]). While set,
    /// that image paints at the live width and other mouse handling is suppressed.
    image_resize: Option<ImageResize>,
    /// An in-progress table column-border drag (drag-to-resize, issue #16):
    /// the column resizes live; release persists `cols=` into the table's
    /// marker line. `None` = no drag.
    table_col_resize: Option<TableColResize>,
    /// Last-paint column-resize grip bands: `(band, header row, column, width)`.
    table_col_resize_rects: Vec<(Bounds<Pixels>, usize, usize, f32)>,
    /// The band index the pointer is on (repaint-on-change for its accent line).
    table_resize_hover: Option<usize>,
    /// See [`ShapeMemo`] — `RefCell` because the measure closure holds only a
    /// read borrow of the editor.
    shape_memo: std::cell::RefCell<Option<ShapeMemo>>,
    /// See [`ScanData`].
    scan_cache: std::cell::RefCell<Option<(u64, std::rc::Rc<ScanData>)>>,
    /// See [`ScrollCompensatorFn`].
    /// Cross-frame shaping caches — line runs, table column widths, and table
    /// wrap rows (see [`ShapeCaches`]). Capacity-capped in `shape_document`.
    shape_caches: ShapeCaches,
    /// The shaping window (element-local y, quantized), set after each
    /// prepaint from the painted bounds — one frame stale by design, so the
    /// measure pass and prepaint always shape with the SAME band and the
    /// measure→prepaint memo keeps hitting.
    shape_band: std::cell::Cell<Option<(f32, f32)>>,
    /// Latch: the scroll compensator fired since the last paint. Measure can
    /// run several times before a paint commits fresh `line_tops`; without
    /// this, one async height change compensates once per measure call.
    /// An active gutter block drag (Notion/Cditor-style reorder): the grabbed
    /// block's first + last rows and the current drop boundary (a row index;
    /// `== rows` drops at the document end).
    line_drag: Option<(usize, usize, usize)>,
    /// The row whose gutter grip the pointer hovers (mirrors prepaint's
    /// computation) — tracked so hover changes repaint the grip.
    grip_hover_row: Option<usize>,
    /// Horizontal scroll of each wide table, keyed by its header row — wide
    /// tables keep natural column widths and scroll in place. Keys drift on
    /// edits above a table; entries are clamped at use, so a stale one is a
    /// harmless partial offset. (ponytail: no eviction, the map stays tiny)
    table_scroll_x: std::collections::HashMap<usize, f32>,
    /// Last-paint wide-table scroll thumbs (padded grab rects), so the thumb
    /// is mouse-draggable, not just an indicator.
    table_thumbs: Vec<TableThumb>,
    /// A live thumb drag: `(header row, grab x, scroll offset at grab)`.
    table_thumb_drag: Option<(usize, Pixels, f32)>,
    /// `content_gen` as of the last paint — a measure with the SAME generation
    /// but different heights means an async (non-edit) height change, the
    /// scroll-anchoring trigger.
    last_paint_gen: u64,
    /// Bumped on every content mutation — cheap staleness key for caches
    /// (the UTF-16 conversion anchor below; a shape cache later).
    content_gen: u64,
    /// Resume point for UTF-8↔UTF-16 conversion: `(generation, utf8, utf16)`
    /// of the last converted offset. IME composition fires conversions many
    /// times per keystroke, clustered near the caret — resuming from the
    /// anchor makes them O(distance) instead of O(document) (CJK latency
    /// grew with note size; found auditing against Cditor's per-block IME).
    utf16_anchor: std::cell::Cell<(u64, usize, usize)>,
    /// Collapsed headings, keyed by the heading's trimmed source line
    /// (`## Goals`). View-local — markdown has no heading-fold syntax (unlike
    /// callouts' `-`/`+`), so folds live for the editor's lifetime and a key
    /// self-heals by vanishing when its heading text is edited.
    folded_headings: std::collections::HashSet<String>,
    /// Painted chevron bounds of heading folds (`(line, rect)`, from the last
    /// paint) — a click toggles that heading in `folded_headings`.
    heading_fold_rects: Vec<(usize, Bounds<Pixels>)>,
    /// Window-space bounds of every heading's first visual row (from the last
    /// paint) — `on_mouse_move` hit-tests these for the hover chevron.
    heading_row_rects: Vec<(usize, Bounds<Pixels>)>,
    /// The heading line the pointer was last over — its chevron shows on hover
    /// (a fold chevron on every heading would clutter). Drives
    /// `on_mouse_move`'s repaint-on-change, like the property-row hover.
    heading_hover_row: Option<usize>,
}

impl EditorState {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            read_only: false,
            plain: false,
            focus_handle: cx.focus_handle(),
            content: String::new(),
            placeholder: SharedString::default(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            search: None,
            search_bounds: Vec::new(),
            annotations: Vec::new(),
            annotation_revision: 0,
            outlines: Vec::new(),
            outline_revision: 0,
            annotation_ends: Vec::new(),
            position_cache: Default::default(),
            #[cfg(test)]
            visible_link_hitboxes: 0,
            hidden_annotation_ids: std::collections::HashSet::new(),
            annotation_bounds: Vec::new(),
            annotation_hover: None,
            annotation_active: None,
            annotation_flashes: Vec::new(),
            range_flashes: Vec::new(),
            text_rolls: Vec::new(),
            wrapped: Vec::new(),
            line_tops: Vec::new(),
            line_heights: Vec::new(),
            wrap_rows: Vec::new(),
            widget_rows: Vec::new(),
            offset_maps: Vec::new(),
            line_insets: Vec::new(),
            rtl_rows: Vec::new(),
            table_rows: Vec::new(),
            table_row_add_rects: Vec::new(),
            table_col_add_rects: Vec::new(),
            table_hover_zones: Vec::new(),
            table_hover_region: None,
            table_row_del: None,
            table_col_del: None,
            table_hover_cell: None,
            last_bounds: None,
            line_height: px(20.),
            autoscroll: None,
            reveal_caret: false,
            drag_autoscroll: None,
            font_size: px(16.),
            paint_font: None,
            is_selecting: false,
            word_drag: None,
            selection_action: None,
            selection_action_dismissed: None,
            selection_bounds: Vec::new(),
            selection_choice_bounds: Default::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            history_id: 0,
            next_history_id: 0,
            pending_changes: Vec::new(),
            last_transaction: None,
            last_edit: EditKind::Other,
            last_edit_keystroke: false,
            goal_x: None,
            upstream_caret: None,
            cell_goal: None,
            diagnostics: Vec::new(),
            markdown_style: None,
            menu: None,
            table_menu: None,
            table_menu_scroll: ScrollHandle::new(),
            image_menu: None,
            suggest: None,
            diagnostic_actions: Vec::new(),
            suggest_task: None,
            block_image: None,
            block_chip: None,
            chip_rows: Vec::new(),
            image_rects: Vec::new(),
            checkbox_rects: Vec::new(),
            code_chip_rects: Vec::new(),
            code_card_rects: Vec::new(),
            code_chip_hover: None,
            alert_fold_rects: Vec::new(),
            image_resize: None,
            table_col_resize: None,
            table_col_resize_rects: Vec::new(),
            table_resize_hover: None,
            shape_memo: std::cell::RefCell::new(None),
            scan_cache: std::cell::RefCell::new(None),
            last_paint_gen: 0,
            shape_caches: ShapeCaches::default(),
            shape_band: std::cell::Cell::new(None),
            line_drag: None,
            grip_hover_row: None,
            table_scroll_x: std::collections::HashMap::new(),
            table_thumbs: Vec::new(),
            table_thumb_drag: None,
            content_gen: 0,
            utf16_anchor: std::cell::Cell::new((0, 0, 0)),
            folded_headings: std::collections::HashSet::new(),
            heading_fold_rects: Vec::new(),
            heading_row_rects: Vec::new(),
            heading_hover_row: None,
        }
    }

    /// Builder: start with the given text (caret at the start).
    /// Make this editor read-only (see the field).
    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        self.read_only = read_only;
        cx.notify();
    }
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Make this a plain-text field whose text is exactly what was typed or
    /// pasted: a paste inserts the clipboard text as it is (line endings
    /// normalized), Enter and ⇧Enter type one newline, and ⌫ / ⌦ remove one
    /// character. No Markdown paste or Enter rules apply. Leave Markdown
    /// styling off for such a field.
    pub fn set_plain_text(&mut self, plain: bool) {
        self.plain = plain;
    }

    /// Keep the caret visible in the host's scroll view: keyboard moves fit
    /// it with context, and a held selection drag scrolls at the edges (ADR 0054).
    pub fn set_autoscroll(&mut self, scroll: ScrollHandle) {
        self.autoscroll = Some(scroll);
    }
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.content = text.into();
        let caret = caret_off_marker_line(&self.content, 0);
        self.selected_range = caret..caret;
        self
    }

    /// Builder: placeholder shown when empty.
    pub fn with_placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// The current document text.
    pub fn text(&self) -> &str {
        &self.content
    }

    /// Monotonic content revision, including load, edit, undo and redo.
    pub fn revision(&self) -> u64 {
        self.content_gen
    }

    /// Selected UTF-8 source range, including source revealed by caret editing.
    pub fn selection(&self) -> Range<usize> {
        self.selected_range.clone()
    }

    /// A mouse selection drag is in progress.
    pub fn is_selecting(&self) -> bool {
        self.is_selecting
    }

    /// Offer (or withdraw) a host action for the current selection.
    pub fn set_selection_action(
        &mut self,
        action: Option<SelectionAction>,
        cx: &mut Context<Self>,
    ) {
        if self.selection_action != action {
            self.selection_action = action;
            cx.notify();
        }
    }

    /// The host action, when it matches the live selection.
    fn live_selection_action(&self) -> Option<&SelectionAction> {
        self.selection_action
            .as_ref()
            .filter(|a| !a.range.is_empty() && a.range == self.selected_range)
    }

    pub fn set_annotations(
        &mut self,
        revision: u64,
        mut annotations: Vec<SourceAnnotation>,
        cx: &mut Context<Self>,
    ) {
        self.annotation_bounds.clear();
        if !annotations.is_sorted_by_key(|a| (a.range.start, a.range.end)) {
            annotations.sort_by_key(|a| (a.range.start, a.range.end));
        }
        let mut end = 0;
        self.annotation_ends = annotations
            .iter()
            .map(|a| {
                end = end.max(a.range.end);
                end
            })
            .collect();
        self.annotations = annotations;
        self.annotation_revision = revision;
        self.annotation_hover = None;
        cx.notify();
    }

    /// Current annotations' glyph colours, in source order.
    fn annotation_text_colors(&self) -> Vec<(Range<usize>, Hsla)> {
        if self.annotation_revision != self.content_gen {
            return Vec::new();
        }
        self.annotations
            .iter()
            .filter_map(|a| Some((a.range.clone(), a.text_color?)))
            .collect()
    }

    /// Outline source ranges for content `revision`, over any annotation fill.
    /// Display only: no text, history or annotation state changes.
    pub fn set_outlines(
        &mut self,
        revision: u64,
        outlines: Vec<(Range<usize>, Hsla)>,
        cx: &mut Context<Self>,
    ) {
        if self.outline_revision != revision || self.outlines != outlines {
            self.outlines = outlines;
            self.outline_revision = revision;
            cx.notify();
        }
    }

    /// The outlined source ranges, when current.
    pub fn outlines(&self) -> &[(Range<usize>, Hsla)] {
        if self.outline_revision == self.content_gen {
            &self.outlines
        } else {
            &[]
        }
    }

    /// Play a transient colour change on annotations by id. Display only.
    pub fn flash_annotations(&mut self, flash: AnnotationFlash, cx: &mut Context<Self>) {
        if flash.ids.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        // Finished flashes (✓ included) no longer paint anything.
        self.annotation_flashes
            .retain(|(start, f)| now < *start + f.duration * 2 + std::time::Duration::from_secs(2));
        self.annotation_flashes.push((now, flash));
        cx.notify();
    }

    /// Fade a chip-shaped ghost out of source ranges. Display only.
    pub fn flash_ranges(&mut self, flash: RangeFlash, cx: &mut Context<Self>) {
        if flash.ranges.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        self.range_flashes
            .retain(|(start, f)| now < *start + f.duration && f.revision == self.content_gen);
        self.range_flashes.push((now, flash));
        cx.notify();
    }

    /// Roll changed text inside its chip. Display only.
    pub fn roll_text(&mut self, roll: TextRoll, cx: &mut Context<Self>) {
        if roll.items.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        self.text_rolls
            .retain(|(start, r)| now < *start + r.duration && r.revision == self.content_gen);
        self.text_rolls.push((now, roll));
        cx.notify();
    }

    /// Whether changed text is still rolling.
    pub fn is_rolling_text(&self) -> bool {
        let now = std::time::Instant::now();
        self.text_rolls
            .iter()
            .any(|(start, r)| now < *start + r.duration && r.revision == self.content_gen)
    }

    /// Whether a range ghost is still fading.
    pub fn is_fading_ranges(&self) -> bool {
        let now = std::time::Instant::now();
        self.range_flashes
            .iter()
            .any(|(start, f)| now < *start + f.duration && f.revision == self.content_gen)
    }

    /// Whether annotation `id` has a colour change still playing.
    pub fn is_flashing(&self, id: u64) -> bool {
        let now = std::time::Instant::now();
        self.annotation_flashes
            .iter()
            .any(|(start, f)| f.ids.contains(&id) && now < *start + f.duration)
    }

    /// Host-selected occurrence remains distinct while keyboard focus is in its popup.
    pub fn set_active_annotation(&mut self, id: Option<u64>, cx: &mut Context<Self>) {
        self.annotation_active = id;
        cx.notify();
    }

    pub fn annotation_bounds(&self, id: u64) -> Option<Bounds<Pixels>> {
        (self.annotation_revision == self.content_gen)
            .then(|| {
                self.annotation_bounds
                    .iter()
                    .find(|(key, _)| *key == id)
                    .map(|(_, bounds)| *bounds)
            })
            .flatten()
    }

    /// Where annotation `id` sits relative to the editor's own top-left after
    /// the last paint: the union of its painted rows, or the text at `offset`
    /// when it was not painted. Unlike window positions this stays valid while
    /// a host scrolls the editor, so the host can anchor to it inside the
    /// scrolled content. `None` once the text changed since that paint.
    pub fn local_annotation_bounds(&self, id: u64, offset: usize) -> Option<Bounds<Pixels>> {
        let origin = self.last_bounds?.origin;
        if self.last_paint_gen != self.content_gen {
            return None;
        }
        let painted = (self.annotation_revision == self.content_gen)
            .then(|| {
                self.annotation_bounds
                    .iter()
                    .filter(|(key, _)| *key == id)
                    .map(|(_, bounds)| *bounds)
                    .reduce(|a, b| a.union(&b))
            })
            .flatten();
        let bounds = painted.or_else(|| self.bounds_for_offset(offset))?;
        Some(Bounds::new(bounds.origin - origin, bounds.size))
    }

    /// The base text line height of the last paint.
    pub fn base_line_height(&self) -> Pixels {
        self.line_height
    }

    /// Whether the painted annotation represents hidden source in the gutter.
    /// The host annotation under the pointer, if any.
    pub fn hovered_annotation(&self) -> Option<u64> {
        self.annotation_hover
    }

    pub fn annotation_is_hidden(&self, id: u64) -> bool {
        self.annotation_revision == self.content_gen && self.hidden_annotation_ids.contains(&id)
    }

    /// Replace the set of diagnostics (underlined spans). The host computes these
    /// (e.g. spell-check) and refreshes them as the text changes.
    pub fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>, cx: &mut Context<Self>) {
        self.diagnostics = diagnostics;
        // Diagnostics feed the per-row run keys (they underline spans) — drop
        // the memo so the next shape re-keys affected lines.
        *self.shape_caches.row_keys.borrow_mut() = (None, Vec::new());
        cx.notify();
    }

    /// The current diagnostics, remapped through edits since they were set.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Turn on WYSIWYG (live-preview) markdown styling with the given
    /// color/font palette (call once at setup). Inline formatting then renders
    /// as you type — markers stay in the text, dimmed. Without it the editor is
    /// the raw view: plain text, spell-check underlines only.
    pub fn set_markdown_style(&mut self, style: SyntaxStyle, cx: &mut Context<Self>) {
        self.markdown_style = Some(style);
        cx.notify();
    }

    /// Install the provider consulted when the user right-clicks a flagged word.
    /// It's handed the offending word and returns a task resolving to
    /// replacements (best first). Kept lazy by design — suggestion lookups can
    /// be slow, so they run only on right-click, never in the per-edit
    /// detection pass; the menu shows a pending row until the task resolves.
    pub fn on_suggest(&mut self, provider: impl Fn(&str, &mut App) -> Task<Vec<String>> + 'static) {
        self.suggest = Some(Box::new(provider));
    }

    /// Actions offered for a flagged word in the right-click menu, below its
    /// suggestions; choosing one emits [`EditorEvent::DiagnosticAction`].
    pub fn set_diagnostic_actions(&mut self, actions: Vec<SharedString>) {
        self.diagnostic_actions = actions;
    }

    /// Install the provider that resolves a standalone image line's `src` to a
    /// decoded image; with it, such lines render inline (W4) when the caret is
    /// elsewhere. Without it (or while an image is still loading), the line shows
    /// its raw `![](src)` source.
    pub fn set_block_image_provider(
        &mut self,
        provider: impl Fn(&str) -> Option<Arc<RenderImage>> + 'static,
    ) {
        self.block_image = Some(Box::new(provider));
    }

    /// Install the provider that classifies an `![](src)` reference as a file chip
    /// (e.g. a PDF) and supplies its label. With it, such lines render as a
    /// clickable chip when the caret is elsewhere; a left-click emits
    /// [`EditorEvent::OpenLink`] and a right-click places the caret to edit.
    pub fn set_block_chip_provider(
        &mut self,
        provider: impl Fn(&str) -> Option<SharedString> + 'static,
    ) {
        self.block_chip = Some(Box::new(provider));
    }

    /// Highlight semantic search occurrences behind the text. Every source
    /// range belonging to one occurrence receives the same active/inactive
    /// color. Empty clears; this never changes the caret or selection.
    pub fn set_search_matches(
        &mut self,
        matches: Vec<SearchMatch>,
        active: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.search_bounds.clear();
        self.search = (!matches.is_empty()).then_some((matches, active));
        cx.notify();
    }

    /// Change only the active occurrence, retaining the matches and their geometry.
    /// Navigation must not clone every source range in a dense result set.
    pub fn set_active_search_match(&mut self, active: Option<usize>, cx: &mut Context<Self>) {
        if let Some((matches, current)) = &mut self.search {
            *current = active.filter(|&index| index < matches.len());
            cx.notify();
        }
    }

    /// Compatibility adapter for hosts that still provide one contiguous range
    /// per match. New Markdown search callers should use [`Self::set_search_matches`].
    pub fn set_search(
        &mut self,
        matches: Vec<Range<usize>>,
        active: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.set_search_matches(
            matches
                .into_iter()
                .map(|range| SearchMatch {
                    source: vec![range],
                    replaceable: true,
                })
                .collect(),
            active,
            cx,
        );
    }

    /// Actual width used by the last painted layout, for host measurements.
    pub fn layout_width(&self) -> Option<Pixels> {
        self.last_bounds.map(|bounds| bounds.size.width)
    }

    /// Window-space bounds from the painted highlight quads. Includes wrapped
    /// prose and table cells; unavailable until the next paint.
    pub fn search_match_bounds(&self, index: usize) -> Option<Bounds<Pixels>> {
        self.search_bounds.get(index).copied().flatten()
    }

    /// The window-space top of the row containing byte `offset` (from the
    /// last layout) — for a host scrolling a find match into view. `None`
    /// before first paint or for an out-of-range offset.
    pub fn offset_screen_top(&self, offset: usize) -> Option<Pixels> {
        if let Some(bounds) = self.bounds_for_offset(offset) {
            return Some(bounds.top());
        }
        let bounds = self.last_bounds?;
        let (row, _) = self.row_col(offset.min(self.content.len()));
        Some(bounds.top() + self.line_tops.get(row).copied()?)
    }

    /// The caret's byte offset into [`Self::text`] (the moving end of any
    /// selection). For hosts that drive a menu/completion off the caret position.
    pub fn cursor(&self) -> usize {
        self.cursor_offset()
    }

    /// Place the caret at `offset` (a byte offset into the document), collapsing
    /// any selection. Clamped to the document and snapped down to a char
    /// boundary, so a host can pass a raw click offset safely — e.g. to enter
    /// edit mode where rendered text was clicked.
    pub fn set_cursor(&mut self, offset: usize, cx: &mut Context<Self>) {
        let mut offset = offset.min(self.content.len());
        while !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        self.move_to(offset, cx);
    }

    /// The caret's last painted bounds, if it was drawn.
    pub fn caret_bounds(&self) -> Option<Bounds<Pixels>> {
        self.bounds_for_offset(self.cursor_offset())
    }

    /// Put the caret at `offset` and scroll it into view after the next
    /// paint, as a keyboard move would. The undo history is untouched.
    pub fn reveal_offset(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.set_cursor(offset, cx);
        self.reveal_caret = true;
        cx.notify();
    }

    /// Select `range` (clamped to char boundaries), as a keyboard selection would.
    pub fn set_selection(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let clamp = |mut offset: usize| {
            offset = offset.min(self.content.len());
            while !self.content.is_char_boundary(offset) {
                offset -= 1;
            }
            offset
        };
        let (start, end) = (clamp(range.start), clamp(range.end));
        self.move_to(start, cx);
        self.selection_reversed = false;
        self.select_to(end, cx);
    }

    /// The host action currently offered for the selection.
    pub fn selection_action(&self) -> Option<&SelectionAction> {
        self.live_selection_action()
    }

    /// Where the pill's [`SelectionChoice`] part was last painted, in window
    /// coordinates; `None` while the pill is hidden. A host chooser anchors here.
    pub fn selection_choice_bounds(&self) -> Option<Bounds<Pixels>> {
        self.selection_choice_bounds.get()
    }

    /// The cached [`ScanData`] for the current content, rebuilding on a
    /// generation mismatch.
    fn scan_data(&self) -> std::rc::Rc<ScanData> {
        if let Some((generation, data)) = self.scan_cache.borrow().as_ref()
            && *generation == self.content_gen
        {
            return data.clone();
        }
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut fence_odd = Vec::with_capacity(lines.len());
        let mut odd = false;
        for l in &lines {
            fence_odd.push(odd);
            if l.trim_start().starts_with("```") {
                odd = !odd;
            }
        }
        let data = std::rc::Rc::new(ScanData {
            generation: self.content_gen,
            ordered: markdown_syntax::ordered_numbers(&lines),
            tables: markdown_syntax::table_regions(&self.content),
            mermaid: markdown_syntax::mermaid_blocks(&self.content),
            math: markdown_syntax::math_regions(&self.content),
            alert_folds: markdown_syntax::alert_fold_regions(&self.content),
            fence_odd,
        });
        *self.scan_cache.borrow_mut() = Some((self.content_gen, data.clone()));
        data
    }

    /// The wrap-row count of logical line `row` (1 when unrecorded).
    fn row_span(&self, row: usize) -> usize {
        self.wrap_rows.get(row).copied().unwrap_or(1).max(1)
    }

    /// The wrap-row height of logical line `row` (a heading is taller). Falls
    /// back to the base `line_height` for unrecorded rows / the empty document.
    fn line_h(&self, row: usize) -> Pixels {
        self.line_heights
            .get(row)
            .copied()
            .unwrap_or(self.line_height)
    }

    /// Horizontal text inset for logical line `row` (from the last paint): non-zero
    /// for fenced code blocks + gutter marks. Applied to the caret, selection,
    /// hit-test, and text paint so they all stay aligned.
    fn line_inset(&self, row: usize) -> Pixels {
        self.line_insets.get(row).copied().unwrap_or(px(0.))
    }

    /// The right-align shift of an RTL row (zero everywhere else) — see
    /// [`RtlRow::shift`].
    fn rtl_shift(&self, row: usize) -> Pixels {
        self.rtl_rows
            .get(row)
            .and_then(Option::as_ref)
            .and_then(|r| r.shifts.first().copied())
            .unwrap_or(px(0.))
    }

    /// Where logical line `row`'s painted text actually starts: its inset plus
    /// the right-align shift of an RTL row (#66). Everything that positions
    /// against a row's text — caret, click, selection, link boxes — goes
    /// through this, so the two can never drift apart.
    fn row_origin_x(&self, row: usize) -> Pixels {
        self.line_inset(row) + self.rtl_shift(row)
    }

    /// Does the caret's TABLE row read right-to-left? Cells step through their
    /// own stepper, which walks in logical order — so on an RTL table the
    /// visual arrows map to the opposite step.
    fn caret_table_is_rtl(&self) -> bool {
        let (row, _) = self.row_col(self.cursor_offset());
        self.table_rows
            .get(row)
            .and_then(Option::as_ref)
            .is_some_and(|t| t.rtl)
    }

    /// Does the caret's line read right-to-left?
    ///
    /// Arrow keys move VISUALLY — Right steps to the character on the right of
    /// the screen, which every platform does in bidi text and which readers of
    /// Persian expect. On an RTL row that character is the logically PREVIOUS
    /// one, so the two step functions swap.
    fn caret_row_is_rtl(&self) -> bool {
        let (row, _) = self.row_col(self.cursor_offset());
        self.rtl_rows
            .get(row)
            .and_then(Option::as_ref)
            .is_some_and(|r| r.base_rtl)
    }

    /// The offset one step to the visual left/right of the caret, taking the
    /// row's direction into account.
    fn horizontal_step(&self, visual_right: bool) -> usize {
        let off = self.cursor_offset();
        let (row, col) = self.row_col(off);
        // Inside a bidi row, "one step right" is not "one byte forward, maybe
        // flipped". A Latin word or URL embedded in Persian runs the other way,
        // and the caret has to flow THROUGH it rather than jump to its far end
        // — so the step comes from the glyph order, via the row's map.
        if let Some(r) = self.bidi_map(row) {
            let dcol = self.display_col(row, col);
            let (k, local) = r.row_of(dcol);
            if let Some(rr) = r.rows.get(k)
                && let Some(next) = rr.map.step_visual(local, visual_right)
            {
                let target = self.line_starts()[row] + self.source_col(row, rr.start + next);
                // A visual step that doesn't move the caret in the DOCUMENT has
                // landed inside something atomic (an inline image's spacer, whose
                // display bytes map back to the span's start). The logical
                // stepper knows how to cross those, so defer to it rather than
                // sitting still.
                if target != off {
                    return target;
                }
            }
            // Off the end of this row: fall through to the logical neighbour,
            // which is what carries the caret onto the next row or line.
            return if visual_right != self.caret_row_is_rtl() {
                self.next_visible_boundary(off)
            } else {
                self.prev_visible_boundary(off)
            };
        }
        if visual_right {
            self.next_visible_boundary(off)
        } else {
            self.prev_visible_boundary(off)
        }
    }

    /// The RTL layout for `row`, if it has one (see [`RtlRow`]).
    fn bidi_map(&self, row: usize) -> Option<&RtlRow> {
        self.rtl_rows.get(row).and_then(Option::as_ref)
    }

    /// Window-space bounds of the caret at `offset`, from the last paint's
    /// layout — for anchoring a popup (e.g. a slash menu) at a document offset.
    /// `None` before the first paint or if `offset`'s row isn't laid out.
    pub(crate) fn bounds_for_offset(&self, offset: usize) -> Option<Bounds<Pixels>> {
        let bounds = self.last_bounds?;
        let (row, col) = self.row_col(offset);
        let lh = self.line_h(row);
        let line = self.wrapped.get(row)?;
        let upstream = offset == self.cursor_offset() && self.caret_upstream();
        let p = caret_line_pos(
            line,
            self.bidi_map(row),
            self.display_col(row, col),
            lh,
            upstream,
        )?;
        let top = bounds.top() + self.line_tops.get(row).copied().unwrap_or(px(0.)) + p.y;
        let x = bounds.left() + p.x + self.row_origin_x(row);
        Some(Bounds::from_corners(point(x, top), point(x, top + lh)))
    }

    /// Focus the editor so it receives keyboard input. (`set_cursor` only moves
    /// the caret; call this to enter edit mode, e.g. on a click into rendered text.)
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
    }

    // --- Cursor movement -----------------------------------------------------

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            // Collapse to the selection's VISUALLY left edge, which on an RTL
            // row is its logical end.
            let to = if self.caret_row_is_rtl() {
                self.selected_range.end
            } else {
                self.selected_range.start
            };
            self.move_to(to, cx);
            return;
        }
        if self.caret_in_table()
            && let Some(off) =
                self.table_move_horizontal(if self.caret_table_is_rtl() { 1 } else { -1 })
        {
            self.move_to(off, cx);
            return;
        }
        let off = self.horizontal_step(false);
        self.move_to(off, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let to = if self.caret_row_is_rtl() {
                self.selected_range.start
            } else {
                self.selected_range.end
            };
            self.move_to(to, cx);
            return;
        }
        if self.caret_in_table()
            && let Some(off) =
                self.table_move_horizontal(if self.caret_table_is_rtl() { -1 } else { 1 })
        {
            self.move_to(off, cx);
            return;
        }
        let off = self.horizontal_step(true);
        self.move_to(off, cx);
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        self.caret_vertical(-1, window, cx);
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        self.caret_vertical(1, window, cx);
    }

    /// Up/Down. In a table, first through the caret cell's own visual rows
    /// (wrapped text, `<br>` lines), then cell-to-cell keeping the column; at
    /// the table's edge `table_move_vertical` returns `None` and a normal move
    /// exits the table.
    fn caret_vertical(&mut self, dir: i32, window: &mut Window, cx: &mut Context<Self>) {
        let step = self.vertical_step(dir, window);
        if step.goal.is_none() && !step.keeps_goal {
            self.move_to(step.offset, cx);
            return;
        }
        // Set the caret directly (not via `move_to`) to keep the goal column.
        self.selected_range = step.offset..step.offset;
        self.finish_vertical_step(&step);
        self.last_edit = EditKind::Other;
        cx.emit(EditorEvent::SelectionChanged);
        cx.notify();
    }

    /// Where Up/Down take the caret (or a selection's head): through a table
    /// cell's own rows, then the next table row, else the next visual row.
    fn vertical_step(&mut self, dir: i32, window: &mut Window) -> VerticalStep {
        if self.caret_in_table() {
            let goal = self
                .cell_goal
                .filter(|&(at, generation, _)| {
                    at == self.cursor_offset() && generation == self.content_gen
                })
                .map(|(.., x)| x);
            if let Some((offset, upstream, x)) = self.table_move_visual(dir, goal, window) {
                self.goal_x = None;
                return VerticalStep {
                    offset,
                    upstream,
                    goal: Some(x),
                    keeps_goal: false,
                };
            }
            if let Some(offset) = self.table_move_vertical(dir) {
                return VerticalStep {
                    offset,
                    upstream: false,
                    goal: None,
                    keeps_goal: false,
                };
            }
        }
        let (offset, upstream) = self.move_vertical(dir);
        VerticalStep {
            offset,
            upstream,
            goal: None,
            keeps_goal: true,
        }
    }

    /// After the caret has moved to `step.offset`: its affinity and cell goal.
    fn finish_vertical_step(&mut self, step: &VerticalStep) {
        self.set_upstream(step.upstream);
        self.cell_goal = step
            .goal
            .map(|x| (self.cursor_offset(), self.content_gen, x));
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let off = self.horizontal_step(false);
        self.select_to(off, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let off = self.horizontal_step(true);
        self.select_to(off, cx);
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.select_vertical(-1, window, cx);
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        self.select_vertical(1, window, cx);
    }

    /// ⇧Up/Down: the selection's head moves as the caret would.
    fn select_vertical(&mut self, dir: i32, window: &mut Window, cx: &mut Context<Self>) {
        let step = self.vertical_step(dir, window);
        if step.goal.is_none() && !step.keeps_goal {
            self.goal_x = None;
        }
        self.select_to(step.offset, cx);
        self.finish_vertical_step(&step);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let (row, col) = self.row_col(self.cursor_offset());
        let starts = self.line_starts();
        // Smart Home on a gutter line (list/task/quote): the marker is hidden
        // behind a painted bullet, so land on the first content character —
        // the raw line start would reveal the marker and let typing break it.
        // A second Home (at or inside the prefix) goes to the true start.
        let plen = self.hidden_prefix_len(row);
        let target = if plen > 0 && col > plen {
            starts[row] + plen
        } else {
            starts[row]
        };
        self.move_to(target, cx);
    }

    /// The hidden marker prefix length of logical `row` — list/task/quote
    /// lines draw their marker as a painted gutter and hide the source chars.
    /// 0 when the line has no gutter or markdown styling is off.
    fn hidden_prefix_len(&self, row: usize) -> usize {
        if self.markdown_style.is_none() {
            return 0;
        }
        let Some(&start) = self.line_starts().get(row) else {
            return 0;
        };
        let line = &self.content[start..self.line_end(row)];
        markdown_syntax::task_prefix(line)
            .map(|(l, ..)| l)
            .or_else(|| markdown_syntax::list_prefix(line).map(|(l, ..)| l))
            .or_else(|| markdown_syntax::blockquote_prefix(line))
            .unwrap_or(0)
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let (row, _) = self.row_col(self.cursor_offset());
        self.move_to(self.line_end(row), cx);
    }

    /// The display-column span of the visual (wrapped) row holding display
    /// column `dcol` of logical line `row`; the whole line before a layout.
    /// A column on a wrap boundary belongs to the row below, or with
    /// `upstream` affinity to the row above.
    fn visual_row_span(&self, row: usize, dcol: usize, upstream: bool) -> (usize, usize) {
        let full = self.display_col(row, self.line_end(row) - self.line_starts()[row]);
        let Some(line) = self.wrapped.get(row) else {
            return (0, full);
        };
        let bounds: Vec<usize> = wrap_indices(line).collect();
        let k = bounds.partition_point(|b| *b < dcol || (!upstream && *b == dcol));
        let start = k.checked_sub(1).map_or(0, |i| bounds[i]);
        (start, bounds.get(k).copied().unwrap_or(full))
    }

    /// On a bidi row, the source offsets drawn at the row's visual left and
    /// right edges (an RTL row starts on the right).
    fn bidi_row_edges(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        let r = self.bidi_map(row)?;
        let (k, _) = r.row_of(self.display_col(row, col));
        let rr = r.rows.get(k)?;
        // The caret stops drawn furthest left and right, as the caret paints.
        let text = self.wrapped.get(row)?.text.clone();
        let last = if k + 1 < r.rows.len() {
            rr.len.saturating_sub(1)
        } else {
            rr.len
        };
        let stops = (0..=last).filter(|i| text.is_char_boundary(rr.start + i));
        let x = |i: &usize| rr.map.x_for_index(*i);
        let left = stops.clone().min_by(|a, b| x(a).total_cmp(&x(b)))?;
        let right = stops.max_by(|a, b| x(a).total_cmp(&x(b)))?;
        let start = self.line_starts()[row];
        let source = |i: usize| start + self.source_col(row, rr.start + i);
        Some((source(left), source(right)))
    }

    /// A table cell holding the caret: its source range (ADR 0061 keeps the
    /// line moves inside it).
    fn caret_table_cell(&self) -> Option<Range<usize>> {
        let (row, col) = self.row_col(self.cursor_offset());
        let t = self.table_rows.get(row).and_then(Option::as_ref)?;
        if t.is_separator || t.cell_ranges.is_empty() {
            return None;
        }
        let cell = t
            .cell_ranges
            .iter()
            .position(|r| col <= r.end)
            .unwrap_or(t.cell_ranges.len() - 1);
        let start = self.line_starts()[row];
        let range = &t.cell_ranges[cell];
        Some(start + range.start..start + range.end)
    }

    /// ⌘← target: the caret's visual row start. On a first row with a hidden
    /// list, task or quote marker it lands after the marker, then at the true
    /// start, like Home. In a table, the cell's start; on a bidi row, its
    /// visual left edge.
    fn visual_line_start(&self) -> usize {
        if let Some(cell) = self.caret_table_cell() {
            return cell.start;
        }
        let (row, col) = self.row_col(self.cursor_offset());
        let start = self.line_starts()[row];
        let plen = self.hidden_prefix_len(row);
        if let Some((left, _)) = self.bidi_row_edges(row, col) {
            // At the body's start after a hidden marker: the second press
            // goes to the true start, like Home.
            // Compared as drawn: hidden formatting after the marker takes
            // no columns.
            let body = self.display_col(row, plen);
            if plen > 0
                && self.display_col(row, left - start) == body
                && self.display_col(row, col) <= body
            {
                return start;
            }
            return left;
        }
        let (vstart, _) =
            self.visual_row_span(row, self.display_col(row, col), self.caret_upstream());
        if vstart == 0 {
            // The first row; display column 0 can sit after a hidden marker.
            return start + if plen > 0 && col > plen { plen } else { 0 };
        }
        start + self.source_col(row, vstart)
    }

    /// ⌘→ target and its affinity: the caret's visual row end. A row that
    /// soft-wrapped at a space ends before the space; one that broke inside a
    /// word ends after its last character, on the wrap boundary with
    /// upstream affinity so the caret stays on that row. In a table, the
    /// cell's end; on a bidi row, its visual right edge.
    fn visual_line_end(&self) -> (usize, bool) {
        if let Some(cell) = self.caret_table_cell() {
            return (cell.end, false);
        }
        let (row, col) = self.row_col(self.cursor_offset());
        if let Some((_, right)) = self.bidi_row_edges(row, col) {
            return (right, false);
        }
        let start = self.line_starts()[row];
        let line_end = self.line_end(row);
        let (vstart, vend) =
            self.visual_row_span(row, self.display_col(row, col), self.caret_upstream());
        let full = self.display_col(row, line_end - start);
        let mut dend = vend;
        let mut upstream = false;
        if vend < full
            && let Some(line) = self.wrapped.get(row)
            && let Some(c) = line.text[..vend.min(line.text.len())].chars().next_back()
        {
            if c.is_whitespace() {
                // Step back over the break space (past hidden markers,
                // which take no display columns).
                dend = (vend - c.len_utf8()).max(vstart);
            } else {
                upstream = true;
            }
        }
        let end = (start + self.source_col(row, dend)).clamp(start + col, line_end);
        if end == start + col {
            // Already there (also after a click past a space-broken row's
            // end): the caret keeps its row.
            return (end, self.caret_upstream());
        }
        (end, upstream && end < line_end)
    }

    fn line_start(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        let off = self.visual_line_start();
        self.move_to(off, cx);
    }

    fn line_end_action(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        let (off, upstream) = self.visual_line_end();
        self.move_to(off, cx);
        self.set_upstream(upstream);
    }

    fn select_to_line_start(
        &mut self,
        _: &SelectToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.goal_x = None;
        let off = self.visual_line_start();
        self.select_to(off, cx);
    }

    fn select_to_line_end(&mut self, _: &SelectToLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let (off, upstream) = self.visual_line_end();
        self.select_to(off, cx);
        self.set_upstream(upstream);
    }

    /// ⌘⌫: delete back to the visual line's start, or the selection. At the
    /// start already it joins with the line above, like ⌫.
    fn delete_to_line_start(
        &mut self,
        _: &DeleteToLineStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let off = self.cursor_offset();
            // Deletion goes to the row's start in reading order, also on an
            // RTL row, whose visual left edge is its logical end.
            let (row, col) = self.row_col(off);
            let start = match self.bidi_map(row) {
                Some(r) if self.caret_table_cell().is_none() => {
                    let (k, _) = r.row_of(self.display_col(row, col));
                    let first = r.rows.get(k).map_or(0, |rr| rr.start);
                    self.line_starts()[row] + self.source_col(row, first)
                }
                _ => self.visual_line_start(),
            };
            if start >= off {
                // At a cell's start, stop: ⌫ would merge the cells.
                if self.caret_table_cell().is_none() {
                    self.backspace(&Backspace, window, cx);
                }
                return;
            }
            self.select_to(start, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    /// ⌥⌦ on macOS: delete forward to the end of the word, or the selection.
    fn delete_to_word_end(
        &mut self,
        _: &DeleteToWordEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let off = self.cursor_offset();
            let end = self.next_word(off);
            if end == off {
                return;
            }
            self.select_to(end, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    // --- Mouse ---------------------------------------------------------------

    fn on_middle_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.annotation_revision == self.content_gen
            && let Some((id, _)) = self
                .annotation_bounds
                .iter()
                .find(|(_, bounds)| bounds.contains(&event.position))
        {
            cx.emit(EditorEvent::MiddleClickAnnotation(*id, event.modifiers));
            cx.stop_propagation();
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cell_goal = None;
        if self.annotation_revision == self.content_gen
            && !event.modifiers.shift
            && event.click_count == 1
            && let Some((id, _)) = self
                .annotation_bounds
                .iter()
                .find(|(_, bounds)| bounds.contains(&event.position))
        {
            cx.emit(EditorEvent::ActivateAnnotation(*id));
            cx.stop_propagation();
            return;
        }
        // A press on an image's corner grip starts a resize drag — this takes
        // precedence over placing the caret on the image row (which the press
        // would otherwise do). The image keeps its bounds; the drag previews a new
        // width and release writes `{width=N}` (see on_mouse_move / on_mouse_up).
        if !self.read_only
            && let Some((line, width)) = self.grip_at(event.position)
        {
            self.image_resize = Some(ImageResize {
                line,
                start_width: width,
                start_x: event.position.x,
                width,
            });
            self.is_selecting = false;
            self.menu = None;
            self.table_menu = None;
            self.image_menu = None;
            cx.notify();
            return;
        }
        // A press on a task checkbox toggles it (☐↔☑) instead of placing the
        // caret — the box sits in the gutter, so this never competes with editing
        // the body text. Same length swap, so the caret/selection stay valid.
        if !self.read_only
            && let Some(row) = self.checkbox_at(event.position)
        {
            let range = self.line_starts()[row]..self.line_end(row);
            if let Some(new_line) =
                markdown_syntax::toggle_task_checkbox(&self.content[range.clone()])
            {
                self.record_edit(&range, &new_line);
                self.content =
                    self.content[..range.start].to_owned() + &new_line + &self.content[range.end..];
                self.remap_diagnostics(&range, new_line.len());
                self.emit_changed(cx);
                cx.notify();
            }
            return;
        }
        // A press on a code card's chrome: Copy writes the block's body to the
        // clipboard; neither it nor the language tag places the caret.
        if let Some((on_copy, fence_row)) = self.code_chip_at(event.position) {
            if on_copy && let Some((_, body)) = self.code_block_at(fence_row) {
                let text = self.content[body].to_string();
                self.write_clipboard(text, cx);
            }
            return;
        }
        // A press on a foldable callout's chevron flips its `-`/`+` fold char
        // (folding/unfolding the body) instead of placing the caret — the same
        // toggle-in-source model as the task checkbox.
        if !self.read_only
            && let Some(row) = self.alert_fold_at(event.position)
        {
            let start = self.line_starts()[row];
            let line = &self.content[start..self.line_end(row)];
            if let Some((at, folded)) = crate::syntax::alert_fold_char(line) {
                let range = start + at..start + at + 1;
                let repl = if folded { "+" } else { "-" };
                self.record_edit(&range, repl);
                self.content.replace_range(range.clone(), repl);
                self.remap_diagnostics(&range, 1);
                self.emit_changed(cx);
                cx.notify();
            }
            return;
        }
        // A press on a heading's fold chevron toggles its section collapsed —
        // view-local state, not an edit (markdown has no heading-fold syntax).
        if let Some(row) = self.heading_fold_at(event.position) {
            let start = self.line_starts()[row];
            let end = self.line_end(row);
            let key = self.content[start..end].trim().to_string();
            if !self.folded_headings.remove(&key) {
                // Folding with the caret inside the section would no-op
                // (reveal-on-caret keeps it open) — seat the caret at the
                // heading's end first.
                let single = std::collections::HashSet::from([key.clone()]);
                let crow = self.row_col(self.cursor_offset()).0;
                if markdown_syntax::heading_fold_regions(&self.content, &single)
                    .iter()
                    .any(|r| crow > r.start && crow < r.end)
                {
                    self.move_to(end, cx);
                }
                self.folded_headings.insert(key);
            }
            cx.notify();
            return;
        }
        // Left-click a file chip (e.g. a PDF embed) opens it rather than editing —
        // the host handles the link. Right-click edits (see on_right_mouse_down).
        if let Some(src) = self.chip_at(event.position) {
            cx.emit(EditorEvent::OpenLink(src));
            return;
        }
        // Left-click a link opens its url — consistent with chips above. Only a plain single click: a
        // double-click still selects the word, shift still extends the
        // selection, and the caret goes anywhere else as usual (to edit a
        // link's own text, click beside it and arrow in — reveal-on-caret).
        if event.click_count == 1
            && !event.modifiers.shift
            && !event.modifiers.control
            && self.markdown_style.is_some()
        {
            let offset = self.index_for_mouse_position(event.position);
            let (row, _) = self.row_col(offset);
            let start = self.line_starts()[row];
            let line = &self.content[start..self.line_end(row)];
            if let Some(url) = markdown_syntax::link_at(line, offset - start) {
                cx.emit(EditorEvent::OpenLink(url.into()));
                return;
            }
        }
        // A press on a table's hover "+" strip adds a row (below) or column (right).
        // The insert APIs are caret-driven, so seat the caret in the table to target
        // them — but capture the user's cell first and restore it after, so the
        // caret stays put instead of following the new row/column.
        if let Some(row) = self.table_add_row_at(event.position) {
            let keep = self.caret_table_cell_pos();
            if let Some(off) = self.cell_start_offset(row, 0) {
                self.selected_range = off..off;
                self.insert_table_row(true, cx);
            }
            if let Some((r, c, ic)) = keep {
                let caret = self.caret_pos_for_cell(r, c, ic);
                self.selected_range = caret..caret;
                cx.notify();
            }
            return;
        }
        if let Some((row, col)) = self.table_add_col_at(event.position) {
            let keep = self.caret_table_cell_pos();
            if let Some(off) = self.cell_start_offset(row, col) {
                self.selected_range = off..off;
                self.insert_table_column(true, cx);
            }
            if let Some((r, c, ic)) = keep {
                let caret = self.caret_pos_for_cell(r, c, ic);
                self.selected_range = caret..caret;
                cx.notify();
            }
            return;
        }
        // A press on a row/column delete "−" handle removes that row/column (seat
        // the caret in it, then reuse the caret-driven delete APIs).
        if let Some((rect, row)) = self.table_row_del
            && rect.contains(&event.position)
        {
            if let Some(off) = self.cell_start_offset(row, 0) {
                self.selected_range = off..off;
                self.delete_table_row(cx);
            }
            return;
        }
        if let Some((rect, row, col)) = self.table_col_del
            && rect.contains(&event.position)
        {
            if let Some(off) = self.cell_start_offset(row, col) {
                self.selected_range = off..off;
                self.delete_table_column(cx);
            }
            return;
        }
        // A press on a wide table's scroll thumb starts a thumb drag — the
        // table scrolls with the pointer (see `on_mouse_move`).
        if let Some(&TableThumb { header, .. }) = self
            .table_thumbs
            .iter()
            .find(|t| t.grab.contains(&event.position))
        {
            let sx = self.table_scroll_x.get(&header).copied().unwrap_or(0.);
            self.table_thumb_drag = Some((header, event.position.x, sx));
            self.is_selecting = false;
            cx.notify();
            return;
        }
        // A press on a column border's resize band starts a drag — the column
        // resizes live; release persists the width (issue #16). A DOUBLE-click
        // auto-fits the column to its content instead (the Excel/Sheets
        // convention for a column border).
        if let Some(&(_, header_row, col, width)) = self
            .table_col_resize_rects
            .iter()
            .find(|(band, ..)| band.contains(&event.position))
        {
            if event.click_count == 2 {
                self.autofit_table_col(header_row, col, window, cx);
                return;
            }
            let Some(table) = self.table_rows.get(header_row).and_then(Option::as_ref) else {
                return;
            };
            self.table_col_resize = Some(TableColResize::start(
                header_row,
                col,
                event.position.x,
                width,
                std::rc::Rc::new(table.col_widths.clone()),
            ));
            self.is_selecting = false;
            cx.notify();
            return;
        }
        // A click on a table cell drops the caret inside the cell, not in the raw
        // `| … |` source.
        let (offset, upstream) = self.caret_hit(event.position, window);
        self.menu = None;
        self.table_menu = None;
        self.image_menu = None;
        self.goal_x = None;
        self.last_edit = EditKind::Other;
        match event.click_count {
            // Double-click selects the word under the cursor.
            // Double-click selects the word under the cursor; holding and
            // dragging extends the selection by whole words.
            2 => {
                let word = self.word_range_at(offset).unwrap_or(offset..offset);
                self.selected_range = word.clone();
                self.selection_reversed = false;
                self.word_drag = Some(word);
                self.is_selecting = true;
                self.start_drag_autoscroll(window, cx);
                cx.emit(EditorEvent::SelectionChanged);
                cx.notify();
            }
            // Triple-click (or more): select the whole logical line.
            n if n >= 3 => {
                let (row, _) = self.row_col(offset);
                self.word_drag = None;
                self.is_selecting = false;
                let start = self.line_starts()[row];
                self.selected_range = start..self.line_end(row);
                self.selection_reversed = false;
                cx.emit(EditorEvent::SelectionChanged);
                cx.notify();
            }
            // Single click: place the caret, or extend the selection with Shift.
            _ => {
                self.word_drag = None;
                self.is_selecting = true;
                if event.modifiers.shift {
                    self.select_to(offset, cx);
                } else {
                    self.move_to(offset, cx);
                }
                self.set_upstream(upstream);
                self.start_drag_autoscroll(window, cx);
            }
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        // End a gutter block drag: splice the block at the drop boundary.
        if let Some((bs, be, t)) = self.line_drag.take() {
            self.apply_line_drag(bs, be, t, cx);
            cx.notify();
            return;
        }
        // End an image-resize drag by persisting the rounded width as `{width=N}`
        // in that image's source line (through the normal mutation path, so it
        // joins the undo history + emits Changed); the next paint shows the saved
        // size and the live override clears.
        if let Some(resize) = self.image_resize.take() {
            self.commit_image_resize(resize, cx);
            cx.notify();
            return;
        }
        // End a table scroll-thumb drag (the live offsets are already stored).
        if self.table_thumb_drag.take().is_some() {
            cx.notify();
            return;
        }
        // End a column-border drag by persisting every column's width into the
        // table marker's `cols=` list (one undo step, emits Changed).
        if let Some(resize) = self.table_col_resize.take() {
            self.commit_table_col_widths(resize, cx);
            cx.notify();
            return;
        }
        self.word_drag = None;
        if std::mem::take(&mut self.is_selecting) {
            cx.emit(EditorEvent::SelectionChanged);
            cx.notify();
        }
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let annotation = if self.annotation_revision == self.content_gen {
            self.annotation_bounds
                .iter()
                .find(|(_, bounds)| bounds.contains(&event.position))
                .map(|(id, _)| *id)
        } else {
            None
        };
        if annotation != self.annotation_hover {
            self.annotation_hover = annotation;
            cx.emit(EditorEvent::HoverAnnotation(annotation));
            cx.notify();
        }

        // While dragging an image's grip, track the pointer: the new width is the
        // grab width plus the horizontal travel, floored at `IMG_MIN_W` and capped
        // to the content width left of the image's inset (so a bulleted image's cap
        // matches `block_img`, no snap-back on release, and it can't run off the
        // page). The paint reads this live width for the dragged image (aspect
        // preserved).
        // While dragging a block by its gutter grip, track the drop boundary
        // (snapped out of rendered regions); paint draws the indicator there.
        if let Some((bs, be, t)) = self.line_drag {
            let b = self.snap_drop_boundary(self.drop_boundary_at(event.position));
            if b != t {
                self.line_drag = Some((bs, be, b));
                cx.notify();
            }
            return;
        }
        // While dragging a wide table's scroll thumb, track the pointer: thumb
        // travel maps to content scroll through the committed factor.
        if let Some((header, grab_x, start_sx)) = self.table_thumb_drag {
            let factor = self
                .table_thumbs
                .iter()
                .find(|t| t.header == header)
                .map_or(0., |t| t.factor);
            let sx = start_sx + f32::from(event.position.x - grab_x) * factor;
            // Clamp at use like the wheel path — the paint clamps too, but a
            // sane stored value keeps other consumers simple.
            let sx = sx.max(0.);
            if self.table_scroll_x.get(&header).copied().unwrap_or(0.) != sx {
                self.table_scroll_x.insert(header, sx);
                cx.notify();
            }
            return;
        }
        // While dragging a table column's border, track the pointer: the new
        // width is the grab width plus the travel, floored so the column can't
        // vanish. Shaping applies it live (see `table_column_widths`).
        if let Some(resize) = self.table_col_resize.as_mut() {
            resize.track(event.position.x);
            cx.notify();
            return;
        }
        if let Some(resize) = self.image_resize {
            let avail = self
                .last_bounds
                .map_or(f32::MAX, |b| f32::from(b.size.width))
                - f32::from(self.line_inset(resize.line));
            let max_w = avail.max(IMG_MIN_W);
            let dx = f32::from(event.position.x - resize.start_x);
            let width = (resize.start_width + dx).clamp(IMG_MIN_W, max_w);
            if let Some(r) = self.image_resize.as_mut() {
                r.width = width;
            }
            cx.notify();
            return;
        }
        if self.is_selecting {
            let (offset, upstream) = self.caret_hit(event.position, window);
            self.drag_select_to(offset, cx);
            if self.word_drag.is_none() {
                self.set_upstream(upstream);
            }
            return;
        }
        // While the right-click menu is open it owns the pointer — don't let the
        // table hover (highlight/handles) track the mouse behind it.
        if self.table_menu.is_some() {
            return;
        }
        // Repaint table "+" affordances when the pointer's region changes, so the
        // hover fill + cursor track the mouse live (the editor otherwise only
        // repaints on the caret blink).
        let region = self.table_hover_region_at(event.position);
        let cell = self.hovered_table_cell(event.position);
        if region != self.table_hover_region || cell != self.table_hover_cell {
            self.table_hover_region = region;
            self.table_hover_cell = cell;
            cx.notify();
        }
        // Repaint the column-resize border accent as the pointer crosses a band
        // (the cursor comes from the hitbox; the painted line needs a frame).
        let on_band = self
            .table_col_resize_rects
            .iter()
            .position(|(b, ..)| b.contains(&event.position));
        if on_band != self.table_resize_hover {
            self.table_resize_hover = on_band;
            cx.notify();
        }
        // Repaint the heading fold chevron when the pointer enters/leaves a
        // heading row (the chevron is hover-revealed).
        let hrow = self
            .heading_row_rects
            .iter()
            .find_map(|(row, b)| b.contains(&event.position).then_some(*row));
        if hrow != self.heading_hover_row {
            self.heading_hover_row = hrow;
            cx.notify();
        }
        // Repaint the code card's chrome (lang tag + Copy) when the pointer
        // enters/leaves a card — it's hover-revealed.
        let ccard = self
            .code_card_rects
            .iter()
            .find_map(|(row, b)| b.contains(&event.position).then_some(*row));
        if ccard != self.code_chip_hover {
            self.code_chip_hover = ccard;
            cx.notify();
        }
    }

    /// Persist a finished grip drag: replace the resized image's source line with
    /// one carrying the rounded `{width=N}`, going through `record_edit` so it's
    /// one undoable edit and emits `Changed`. A no-op if the line vanished or
    /// isn't an image any more (it shaped to an image last paint, but guard
    /// anyway), or if the width didn't actually change.
    fn commit_image_resize(&mut self, resize: ImageResize, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let starts = self.line_starts();
        let Some(&start) = starts.get(resize.line) else {
            return;
        };
        let end = self.line_end(resize.line);
        let line = &self.content[start..end];
        let new_line = set_image_width(line, resize.width.round().max(IMG_MIN_W) as u32);
        if new_line == line {
            return;
        }
        let range = start..end;
        let delta = new_line.len() as isize - (end - start) as isize;
        self.record_edit(&range, &new_line);
        self.content = self.content[..start].to_owned() + &new_line + &self.content[end..];
        self.remap_diagnostics(&range, new_line.len());
        // The line just grew/shrank by `delta` — shift a caret at/after its old
        // end with the text (the drop path parks it on the line below), else its
        // stale offset lands inside the new `{width=N}` tail and reveal-on-caret
        // swaps the freshly resized image for raw source. An offset inside the
        // line clamps to the new line end.
        let remap = |o: usize| {
            if o >= end {
                o.saturating_add_signed(delta)
            } else {
                o.min(start + new_line.len())
            }
        };
        self.selected_range = remap(self.selected_range.start)..remap(self.selected_range.end);
        self.emit_changed(cx);
        cx.notify();
    }

    /// If logical line `row` renders as an inline image (a standalone `![](src)`
    /// or list-item image in markdown mode — not a file chip), the byte range of
    /// the whole line plus its trailing newline: the atomic unit Word-style
    /// deletion removes. `None` with styling off (raw mode edits as plain text).
    fn image_row_range(&self, row: usize) -> Option<Range<usize>> {
        self.markdown_style.as_ref()?;
        let start = *self.line_starts().get(row)?;
        let end = self.line_end(row);
        let (src, ..) = markdown_syntax::image_row(&self.content[start..end])?;
        if let Some(chip) = &self.block_chip
            && chip(src).is_some()
        {
            return None; // a chip's line edits as text (reveal-on-caret)
        }
        Some(start..(end + 1).min(self.content.len()))
    }

    /// Delete the image occupying logical line `row` — line + trailing newline,
    /// one undoable edit. Backs the right-click "Delete image" and the
    /// Word-style Backspace/Delete on an image row.
    fn delete_image_row(&mut self, row: usize, cx: &mut Context<Self>) {
        if let Some(range) = self.image_row_range(row) {
            self.replace_range(range, "", cx);
            self.emit_changed(cx);
        }
    }

    /// Right-click: if the click lands on a flagged word, fetch its suggestions
    /// (lazily, via the provider) and open a menu anchored there; otherwise close
    /// any open menu.
    fn on_right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Right-click on an inline image: Word-style object menu (Delete) — the
        // row renders as a picture, there's no text under the pointer to edit.
        // Only for real `![](src)` rows: mermaid/math rasters share the widget
        // type but delete as text (their menu would silently no-op).
        if let Some(&(line, _)) = self
            .image_rects
            .iter()
            .find(|(_, rect)| rect.contains(&event.position))
            .filter(|&&(line, _)| self.image_row_range(line).is_some())
        {
            self.menu = None;
            self.table_menu = None;
            self.focus(window, cx);
            self.image_menu = Some((line, event.position));
            cx.notify();
            return;
        }
        // Right-click a file chip places the caret to edit its source (the line
        // then reveals raw `![](src)`), instead of opening the spell menu.
        if self.chip_at(event.position).is_some() {
            self.menu = None;
            self.focus(window, cx);
            let offset = self.index_for_mouse_position(event.position);
            self.move_to(offset, cx);
            return;
        }
        // Right-click in a table cell: place the caret there + open the table menu
        // (insert/delete rows + columns), instead of the spell menu. INSIDE a
        // selection, keep the selection and show the clipboard menu instead —
        // Cut/Copy act on it, like prose; the structure menu stays a
        // selection-free right-click away.
        if let Some(offset) = self.table_offset_at(event.position, window) {
            self.menu = None;
            self.focus(window, cx);
            let sel = self.selected_range.clone();
            if !sel.is_empty() && offset >= sel.start && offset <= sel.end {
                self.menu = Some(DiagMenu {
                    anchor: event.position,
                    range: offset..offset,
                    suggestions: Vec::new(),
                    suggesting: false,
                    scroll: ScrollHandle::new(),
                    turn_into: false,
                });
            } else {
                self.move_to(offset, cx);
                self.table_menu = Some(event.position);
            }
            cx.notify();
            return;
        }
        let offset = self.index_for_mouse_position(event.position);
        // Window-space — the popup renders on a `deferred`/`anchored` layer.
        let anchor = event.position;
        // A right-click outside the selection moves the caret there (so Paste
        // lands under the pointer); inside it, the selection stays put — it's
        // what Cut/Copy act on.
        let sel = self.selected_range.clone();
        let in_selection = !sel.is_empty() && offset >= sel.start && offset <= sel.end;
        if !in_selection {
            self.move_to(offset, cx);
        }
        self.focus(window, cx);
        // Suggestions when the click lands on a flagged word; the clipboard
        // verbs (Cut / Copy / Paste) ride along either way.
        let diagnostic = self.diagnostic_at(offset).map(|d| d.range.clone());
        self.suggest_task = None;
        let mut suggesting = false;
        if let (Some(range), Some(provider)) = (diagnostic.clone(), self.suggest.as_ref()) {
            let word = self.content[range.clone()].to_string();
            let lookup = provider(&word, cx);
            suggesting = true;
            self.suggest_task = Some(cx.spawn(async move |editor, cx| {
                let suggestions = lookup.await;
                let _ = editor.update(cx, |editor, cx| {
                    // The menu may have closed or moved to another word meanwhile.
                    if let Some(menu) = editor.menu.as_mut().filter(|m| m.range == range) {
                        menu.suggestions =
                            suggestions.into_iter().map(SharedString::from).collect();
                        menu.suggesting = false;
                        cx.notify();
                    }
                });
            }));
        }
        let range = diagnostic.unwrap_or(offset..offset);
        self.menu = Some(DiagMenu {
            anchor,
            range,
            suggestions: Vec::new(),
            suggesting,
            scroll: ScrollHandle::new(),
            turn_into: false,
        });
        cx.notify();
    }

    /// The diagnostic whose range contains `offset`, if any.
    fn diagnostic_at(&self, offset: usize) -> Option<&Diagnostic> {
        self.diagnostics
            .iter()
            .find(|d| d.range.start <= offset && offset < d.range.end)
    }

    /// The host's selection action as the first right-click item.
    fn selection_action_item(
        &self,
        fg: Hsla,
        hover: Hsla,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        use gpui::prelude::FluentBuilder as _;
        let action = self.live_selection_action()?.clone();
        let muted = Hsla {
            a: fg.a * 0.55,
            ..fg
        };
        Some(
            div()
                .id("menu-selection-action")
                .flex_shrink_0()
                .px(px(10.))
                .py(px(3.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap(px(16.))
                        .child(action.menu_label.clone())
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child(action.shortcut.clone()),
                        ),
                )
                .when_some(action.disabled.clone(), |v, reason| {
                    v.text_color(muted)
                        .child(div().text_size(px(11.)).child(reason))
                })
                .when(action.disabled.is_none(), |v| {
                    v.hover(move |s| s.bg(hover)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|editor, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            editor.menu = None;
                            cx.emit(EditorEvent::SelectionAction);
                            cx.notify();
                        }),
                    )
                }),
        )
    }

    /// A compact pill just above the settled selection's end (below it near
    /// the window top) offering the host's selection action. It lives only
    /// while the selection is held, so it may cover the line above (ADR 0055).
    /// An action the selection cannot take shows no pill; the right-click
    /// menu keeps the item and its reason. A [`SelectionChoice`] splits it:
    /// "Replace · Person ▾" (ADR 0024).
    fn selection_pill(&self, window: &Window, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        use gpui::prelude::FluentBuilder as _;
        self.selection_choice_bounds.set(None);
        let action = self.live_selection_action()?.clone();
        // The host's open chooser holds focus; the pill stays beside it.
        let open = action.choice.as_ref().is_some_and(|c| c.open);
        if self.is_selecting
            || self.menu.is_some()
            || action.disabled.is_some()
            || !open
                && (self.selection_action_dismissed.as_ref() == Some(&self.selected_range)
                    || !self.focus_handle.is_focused(window))
        {
            return None;
        }
        // The selection's last visual quad: its end, wherever it was painted
        // (table cells, wrapped rows); none while it is scrolled out of view.
        let end = *self.selection_bounds.iter().max_by(|a, b| {
            a.bottom()
                .partial_cmp(&b.bottom())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    a.right()
                        .partial_cmp(&b.right())
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        })?;
        let st = self.markdown_style.as_ref();
        let bg = st.map_or(rgb(0x26262b).into(), |s| s.popover_bg);
        let border = st.map_or(rgb(0x45454c).into(), |s| s.popover_border);
        let fg = st.map_or(rgb(0xe6e6e6).into(), |s| s.popover_fg);
        let accent = action.accent;
        let (corner, at) = if end.top() > px(44.) {
            (
                gpui::Anchor::BottomRight,
                point(end.right(), end.top() - px(3.)),
            )
        } else {
            (
                gpui::Anchor::TopRight,
                point(end.right(), end.bottom() + px(3.)),
            )
        };
        let muted = Hsla {
            a: fg.a * 0.55,
            ..fg
        };
        let tint = Hsla { a: 0.14, ..accent };
        // A compact action chip: a tiny proposal-coloured token (what the
        // selection becomes), the label, and its shortcut.
        let main = div()
            .id("selection-action-main")
            .h_full()
            .pl(px(6.))
            .pr(px(7.))
            .flex()
            .items_center()
            .gap(px(5.))
            .child(
                div()
                    .size(px(9.))
                    .rounded(px(3.))
                    .border_1()
                    .border_color(accent)
                    .bg(Hsla { a: 0.25, ..accent }),
            )
            .child(action.label.clone())
            .when(!action.shortcut.is_empty(), |v| {
                v.child(
                    div()
                        .text_size(px(11.))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(muted)
                        .child(action.shortcut.clone()),
                )
            })
            // Keep the editor from moving the caret under the pill.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(EditorEvent::SelectionAction);
                }),
            )
            .when(action.choice.is_some(), |v| v.hover(move |s| s.bg(tint)));
        // "Person ▾": the host's chooser opens below or above this part.
        let choice = action.choice.clone().map(|choice| {
            let bounds = self.selection_choice_bounds.clone();
            div()
                .id("selection-action-choice")
                .relative()
                .h_full()
                .pl(px(6.))
                .pr(px(7.))
                .flex()
                .items_center()
                .gap(px(5.))
                .border_l_1()
                .border_color(border)
                .when(choice.open, |v| v.bg(tint))
                .hover(move |s| s.bg(tint))
                .child(div().size(px(7.)).rounded_full().bg(choice.accent))
                .child(format!("{} ▾", choice.label))
                .child(
                    gpui::canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                        .absolute()
                        .inset_0(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |editor, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        // The press is the editor's, so an open chooser
                        // treats it as its own toggle, not an outside click.
                        window.focus(&editor.focus_handle, cx);
                        cx.emit(EditorEvent::SelectionActionChoice);
                    }),
                )
        });
        let pill = div()
            .id("selection-action-pill")
            .occlude()
            .cursor(CursorStyle::PointingHand)
            .h(px(22.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .overflow_hidden()
            .bg(bg)
            .border_1()
            .border_color(border)
            .shadow_md()
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(fg)
            .child(main)
            .children(choice)
            .hover(move |s| s.border_color(Hsla { a: 0.7, ..accent }));
        Some(
            gpui::deferred(
                gpui::anchored()
                    .anchor(corner)
                    .position(at)
                    .snap_to_window()
                    .child(pill),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    /// Close the suggestions menu (Escape, or a click elsewhere).
    fn dismiss(&mut self, _: &Dismiss, _: &mut Window, cx: &mut Context<Self>) {
        if self.menu.take().is_some()
            || self.table_menu.take().is_some()
            || self.image_menu.take().is_some()
        {
            cx.notify();
        } else if self.live_selection_action().is_some()
            && self.selection_action_dismissed.as_ref() != Some(&self.selected_range)
        {
            // Escape hides the selection pill until the selection changes.
            self.selection_action_dismissed = Some(self.selected_range.clone());
            cx.notify();
        }
    }

    /// Replace `range` with a chosen suggestion and close the menu.
    fn apply_suggestion(
        &mut self,
        range: Range<usize>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        self.selected_range = range;
        self.selection_reversed = false;
        self.replace_text_in_range(None, text, window, cx);
    }

    /// The fenced code block containing `row`: its opening fence row plus the
    /// closing fence row (`None` when the block runs unclosed to the end).
    /// The single source for turn-into, block drag, and drop snapping.
    fn fence_block_rows(&self, row: usize) -> (usize, Option<usize>) {
        let scan = self.scan_data();
        let starts = self.line_starts();
        let last_row = starts.len().saturating_sub(1);
        let open = (0..=row)
            .rev()
            .find(|&r| !scan.fence_odd.get(r).copied().unwrap_or(false))
            .unwrap_or(0);
        let close = ((open + 1)..=last_row).find(|&r| {
            self.content[starts[r]..self.line_end(r)]
                .trim_start()
                .starts_with("```")
        });
        (open, close)
    }

    /// The contiguous blockquote run containing `row` (first row..=last row),
    /// by the renderer's `blockquote_prefix` test.
    fn quote_run_rows(&self, row: usize) -> (usize, usize) {
        let starts = self.line_starts();
        let last_row = starts.len().saturating_sub(1);
        let is_q = |r: usize| {
            markdown_syntax::blockquote_prefix(&self.content[starts[r]..self.line_end(r)]).is_some()
        };
        let mut first = row;
        while first > 0 && is_q(first - 1) {
            first -= 1;
        }
        let mut last = row;
        while last < last_row && is_q(last + 1) {
            last += 1;
        }
        (first, last)
    }

    /// The "Turn into" kind of the block containing `row` — what the flyout
    /// shows checked.
    fn block_kind_at(&self, row: usize) -> TurnKind {
        let scan = self.scan_data();
        if scan.math.iter().any(|m| m.range.contains(&row)) {
            return TurnKind::Math;
        }
        let starts = self.line_starts();
        let Some(&start) = starts.get(row) else {
            return TurnKind::Text;
        };
        // The renderer's own recognizers (task/list/heading/quote/alert), so
        // the checked kind always agrees with what WYSIWYG actually renders.
        let line = &self.content[start..self.line_end(row)];
        if scan.fence_odd.get(row).copied().unwrap_or(false) || line.trim_start().starts_with("```")
        {
            return TurnKind::Code;
        }
        if markdown_syntax::task_prefix(line).is_some() {
            return TurnKind::Todo;
        }
        match markdown_syntax::heading_level(line) {
            Some(1) => return TurnKind::H1,
            Some(2) => return TurnKind::H2,
            Some(3) => return TurnKind::H3,
            _ => {}
        }
        if let Some((_, _, ordered, _)) = markdown_syntax::list_prefix(line) {
            return if ordered {
                TurnKind::Numbered
            } else {
                TurnKind::Bullet
            };
        }
        if markdown_syntax::blockquote_prefix(line).is_some() {
            // Callout if the caret's contiguous `>`-run carries a VALID
            // `[!KIND]` marker anywhere (marker line or body line).
            let (first, last) = self.quote_run_rows(row);
            for (r, &start) in starts.iter().enumerate().take(last + 1).skip(first) {
                let l = &self.content[start..self.line_end(r)];
                let p = markdown_syntax::blockquote_prefix(l).unwrap_or(0);
                if markdown_syntax::alert_kind(&l[p..]).is_some() {
                    return TurnKind::Callout;
                }
            }
            return TurnKind::Quote;
        }
        TurnKind::Text
    }

    /// Convert the caret's block to `kind` — the "Turn into" menu action.
    /// One undoable edit rewriting the block's lines; fenced kinds (code,
    /// math) and quote runs convert as whole blocks.
    fn turn_into(&mut self, kind: TurnKind, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.row_col(self.selected_range.start).0;
        let cur = self.block_kind_at(row);
        if cur == kind {
            return;
        }
        let scan = self.scan_data();
        let starts = self.line_starts();
        let last_row = starts.len().saturating_sub(1);
        let line_at = |r: usize| self.content[starts[r]..self.line_end(r)].to_string();
        // The block's line span + its content with the current dressing removed.
        let (first, last, mut body): (usize, usize, Vec<String>) = match cur {
            TurnKind::Code => {
                let (open, close) = self.fence_block_rows(row);
                let last = close.unwrap_or(last_row);
                let body_end = close.map(|c| c.saturating_sub(1)).unwrap_or(last_row);
                let body = ((open + 1)..=body_end).map(line_at).collect();
                (open, last, body)
            }
            TurnKind::Math => {
                let Some(reg) = scan.math.iter().find(|m| m.range.contains(&row)) else {
                    return;
                };
                let first = reg.range.start;
                let last = reg.range.end.saturating_sub(1).max(first);
                let body = ((first + 1)..last).map(line_at).collect();
                (first, last, body)
            }
            TurnKind::Quote | TurnKind::Callout => {
                let (first, last) = self.quote_run_rows(row);
                let body = (first..=last)
                    .filter_map(|r| {
                        let line = line_at(r);
                        let stripped = strip_block_prefix(&line);
                        // Drop the `[!KIND]` marker token; keep any title text
                        // after it (and skip the line if that leaves nothing).
                        if let Some(rest) = stripped.trim_start().strip_prefix("[!") {
                            let after = rest.split_once(']').map(|(_, a)| a).unwrap_or("");
                            let after = after.trim_start_matches(['-', '+']).trim();
                            return (!after.is_empty()).then(|| after.to_string());
                        }
                        Some(stripped.to_string())
                    })
                    .collect();
                (first, last, body)
            }
            _ => (
                row,
                row,
                vec![strip_block_prefix(&line_at(row)).to_string()],
            ),
        };
        if body.is_empty() {
            body.push(String::new());
        }
        let text = match kind {
            TurnKind::Text => body.join("\n"),
            TurnKind::H1 => prefix_lines(&body, |_| "# ".into()),
            TurnKind::H2 => prefix_lines(&body, |_| "## ".into()),
            TurnKind::H3 => prefix_lines(&body, |_| "### ".into()),
            TurnKind::Bullet => prefix_lines(&body, |_| "- ".into()),
            TurnKind::Numbered => prefix_lines(&body, |i| format!("{}. ", i + 1)),
            TurnKind::Todo => prefix_lines(&body, |_| "- [ ] ".into()),
            TurnKind::Quote => prefix_lines(&body, |_| "> ".into()),
            TurnKind::Callout => format!("> [!NOTE]\n{}", prefix_lines(&body, |_| "> ".into())),
            TurnKind::Code => format!("```\n{}\n```", body.join("\n")),
            TurnKind::Math => format!("$$\n{}\n$$", body.join("\n")),
        };
        let range = starts[first]..self.line_end(last);
        let range_start = range.start;
        self.selected_range = range;
        self.selection_reversed = false;
        self.replace_text_in_range(None, &text, window, cx);
        // Fenced kinds: the caret parks after the closing fence, revealing the
        // raw markers (reveal-on-caret) — seat it on the body's first line
        // instead, like `set_code_lang`.
        if matches!(kind, TurnKind::Code | TurnKind::Math) {
            let caret =
                (range_start + text.find('\n').map_or(0, |i| i + 1)).min(self.content.len());
            self.selected_range = caret..caret;
        }
    }

    /// The line span the gutter grip drags as one unit: whole fenced/rendered
    /// regions (code, math, mermaid, tables incl. their style marker,
    /// property panels), a quote/callout run, a list item with its
    /// deeper-indented children — otherwise the single line.
    fn drag_block_rows(&self, row: usize) -> (usize, usize) {
        let scan = self.scan_data();
        let starts = self.line_starts();
        let last_row = starts.len().saturating_sub(1);
        let line_at = |r: usize| &self.content[starts[r]..self.line_end(r)];
        if let Some(m) = scan
            .math
            .iter()
            .find(|m| m.range.contains(&row) || m.marker_line == Some(row))
        {
            let first = m.marker_line.unwrap_or(m.range.start).min(m.range.start);
            return (first, m.range.end.saturating_sub(1).max(m.range.start));
        }
        if let Some((r, _)) = scan.mermaid.iter().find(|(r, _)| r.contains(&row)) {
            return (r.start, r.end.saturating_sub(1).max(r.start));
        }
        if let Some(t) = scan
            .tables
            .iter()
            .find(|t| t.lines.contains(&row) || t.marker_line == Some(row))
        {
            let first = t.marker_line.unwrap_or(t.lines.start).min(t.lines.start);
            return (first, t.lines.end.saturating_sub(1).max(t.lines.start));
        }
        if scan.fence_odd.get(row).copied().unwrap_or(false)
            || line_at(row).trim_start().starts_with("```")
        {
            let (open, close) = self.fence_block_rows(row);
            return (open, close.unwrap_or(last_row));
        }
        if markdown_syntax::blockquote_prefix(line_at(row)).is_some() {
            return self.quote_run_rows(row);
        }
        if markdown_syntax::list_prefix(line_at(row)).is_some() {
            let indent = |r: usize| {
                let l = line_at(r);
                l.len() - l.trim_start().len()
            };
            let base = indent(row);
            let mut last = row;
            while last < last_row && !line_at(last + 1).trim().is_empty() && indent(last + 1) > base
            {
                last += 1;
            }
            return (row, last);
        }
        (row, row)
    }

    /// The row whose gutter grip the pointer would hover (the event-time
    /// mirror of prepaint's grip computation, for repaint change-detection).
    fn grip_hover_row_at(&self, position: Point<Pixels>) -> Option<usize> {
        let bounds = self.last_bounds?;
        if self.markdown_style.is_none()
            || self.line_drag.is_some()
            || position.x < grip_left(bounds.origin.x) - px(4.)
            || position.x > bounds.origin.x + bounds.size.width
            || position.y < bounds.origin.y
            || position.y > bounds.origin.y + bounds.size.height
        {
            return None;
        }
        let y = position.y - bounds.origin.y;
        (0..self.line_tops.len()).find(|&i| {
            let h = self.line_h(i) * self.row_span(i) as f32;
            h > px(0.5) && y >= self.line_tops[i] && y < self.line_tops[i] + h
        })
    }

    /// The drop boundary (a between-rows index) nearest the pointer, from the
    /// last paint's committed geometry.
    fn drop_boundary_at(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        let y = position.y - bounds.origin.y;
        for i in 0..self.line_tops.len() {
            let mid = self.line_tops[i] + self.line_h(i) * self.row_span(i) as f32 / 2.;
            if y < mid {
                return i;
            }
        }
        self.line_tops.len()
    }

    /// Clamp a drop boundary out of the interior of any rendered region — a
    /// block can't land inside a table, fence, math block, or property panel.
    fn snap_drop_boundary(&self, mut b: usize) -> usize {
        let scan = self.scan_data();
        let snap = |b: usize, s: usize, e: usize| {
            if b > s && b < e {
                if b - s <= e - b { s } else { e }
            } else {
                b
            }
        };
        for m in scan.math.iter() {
            let s = m.marker_line.unwrap_or(m.range.start).min(m.range.start);
            b = snap(b, s, m.range.end);
        }
        for (r, _) in scan.mermaid.iter() {
            b = snap(b, r.start, r.end);
        }
        for t in scan.tables.iter() {
            let s = t.marker_line.unwrap_or(t.lines.start).min(t.lines.start);
            b = snap(b, s, t.lines.end);
        }
        // Inside a code fence: snap to the opening fence or past the close.
        if scan.fence_odd.get(b).copied().unwrap_or(false) {
            let (open, close) = self.fence_block_rows(b);
            let close = close.map(|c| c + 1).unwrap_or(self.line_starts().len());
            b = if b - open <= close - b { open } else { close };
        }
        b
    }

    /// Land the grabbed block at boundary `t` — one undoable splice of the
    /// span between the block and the target, caret seated on the block.
    fn apply_line_drag(&mut self, bs: usize, be: usize, t: usize, cx: &mut Context<Self>) {
        let starts = self.line_starts();
        let n = starts.len();
        if bs >= n || be >= n || (t >= bs && t <= be + 1) {
            return; // dropped onto itself (or stale rows) — no-op
        }
        let block_start = starts[bs];
        let block_end = self.line_end(be);
        let block_text = self.content[block_start..block_end].to_string();
        if t > be {
            // Down: [block \n between...] → [between... block (\n)]
            let target_off = if t >= n {
                self.content.len()
            } else {
                starts[t]
            };
            let after_block = (block_end + 1).min(self.content.len());
            let mut new = self.content[after_block..target_off].to_string();
            if !new.is_empty() && !new.ends_with('\n') {
                new.push('\n');
            }
            let rest_len = new.len();
            new.push_str(&block_text);
            if t < n {
                new.push('\n');
            }
            self.replace_range(block_start..target_off, &new, cx);
            let caret = block_start + rest_len;
            self.selected_range = caret..caret;
        } else {
            // Up: [between... block] → [block \n between...]
            let target_off = starts[t];
            let between = &self.content[target_off..block_start];
            let mut new = block_text.clone();
            new.push('\n');
            new.push_str(&between[..between.len().saturating_sub(1)]);
            self.replace_range(target_off..block_end, &new, cx);
            self.selected_range = target_off..target_off;
        }
        self.emit_changed(cx);
    }

    // --- Selection helpers ---------------------------------------------------

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.upstream_caret = None;
        self.cell_goal = None;
        // A deliberate caret move ends the current typing/deleting run and the
        // vertical-movement goal column.
        self.last_edit = EditKind::Other;
        self.goal_x = None;
        cx.emit(EditorEvent::SelectionChanged);
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    /// Whether the caret has upstream affinity: on a soft-wrap boundary it
    /// belongs to the row above (ADR 0061).
    pub(crate) fn caret_upstream(&self) -> bool {
        self.upstream_caret == Some((self.cursor_offset(), self.content_gen))
    }

    /// Give the caret, where it is now, upstream (`true`) or the default
    /// downstream affinity. Drops the cell goal: a vertical step restores it.
    fn set_upstream(&mut self, upstream: bool) {
        self.upstream_caret = upstream.then_some((self.cursor_offset(), self.content_gen));
        self.cell_goal = None;
    }

    /// Extend a mouse selection to `offset`: by characters, or after a
    /// double-click by whole words, keeping the double-clicked word.
    fn drag_select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let Some(anchor) = self.word_drag.clone() else {
            self.select_to(offset, cx);
            return;
        };
        let word = self.word_range_at(offset).unwrap_or(offset..offset);
        let (range, reversed) = if word.start < anchor.start {
            (word.start..anchor.end, true)
        } else {
            (anchor.start..word.end.max(anchor.end), false)
        };
        if range != self.selected_range || reversed != self.selection_reversed {
            self.selected_range = range;
            self.selection_reversed = reversed;
            cx.emit(EditorEvent::SelectionChanged);
            cx.notify();
        }
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.upstream_caret = None;
        self.cell_goal = None;
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.emit(EditorEvent::SelectionChanged);
        cx.notify();
    }

    // --- Autoscroll (ADR 0054) ----------------------------------------------

    /// Scroll the host view the least that shows the caret row with up to three
    /// base line heights of context (Zed's `Fit`). Runs after a paint, so the
    /// layout is the one the keyboard change produced.
    fn fit_caret(&mut self, window: &mut Window) {
        let Some(scroll) = self.autoscroll.clone() else {
            return;
        };
        let Some(caret) = self
            .table_caret_bounds(window)
            .or_else(|| self.bounds_for_offset(self.cursor_offset()))
        else {
            return;
        };
        let viewport = scroll.bounds();
        let margin = (self.line_height * 3.)
            .min((viewport.size.height - caret.size.height) / 2.)
            .max(px(0.));
        let mut offset = scroll.offset();
        let top = caret.top() - margin;
        let bottom = caret.bottom() + margin;
        if top < viewport.top() {
            offset.y += viewport.top() - top;
        } else if bottom > viewport.bottom() {
            offset.y -= bottom - viewport.bottom();
        }
        offset.y = offset.y.clamp(-scroll.max_offset().y, px(0.));
        if offset != scroll.offset() {
            scroll.set_offset(offset);
            window.refresh();
        }
    }

    /// Scroll while a mouse selection is held near a viewport edge, every frame
    /// even with a still pointer, until the button is released.
    fn start_drag_autoscroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.autoscroll.is_none() {
            return;
        }
        self.drag_autoscroll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
                let held = this
                    .update_in(cx, |editor, window, cx| {
                        editor.drag_autoscroll_step(window, cx)
                    })
                    .unwrap_or(false);
                if !held {
                    break;
                }
            }
        }));
    }

    /// One drag-autoscroll step; `false` once the selection drag has ended.
    fn drag_autoscroll_step(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(scroll) = self.autoscroll.clone().filter(|_| self.is_selecting) else {
            return false;
        };
        let viewport = scroll.bounds();
        // Zed: a one-line band inside each edge, so a window flush with the
        // screen edge still scrolls; speed grows with depth, capped at 3 lines.
        let band = self.line_height.min(viewport.size.height / 3.);
        let mouse = window.mouse_position();
        let speed = |depth: Pixels| self.line_height * (f32::from(depth).powf(1.2) / 100.).min(3.);
        let delta = if mouse.y < viewport.top() + band {
            -speed(viewport.top() + band - mouse.y)
        } else if mouse.y > viewport.bottom() - band {
            speed(mouse.y - (viewport.bottom() - band))
        } else {
            return true;
        };
        let mut offset = scroll.offset();
        let before = offset.y;
        offset.y = (offset.y - delta).clamp(-scroll.max_offset().y, px(0.));
        if offset.y != before {
            scroll.set_offset(offset);
            window.refresh();
        }
        // Hit-test against the last paint, shifted by this step's scroll.
        let at = point(mouse.x, mouse.y - (offset.y - before));
        let (target, upstream) = self.caret_hit(at, window);
        if target != self.cursor_offset() {
            self.drag_select_to(target, cx);
            if self.word_drag.is_none() {
                self.set_upstream(upstream);
            }
        }
        true
    }

    // --- Line / row-col mapping ---------------------------------------------

    /// Byte offset where each visual line starts (line 0 starts at 0; each line
    /// after a `\n`). Always has at least one entry.
    fn line_starts(&self) -> Vec<usize> {
        let mut starts = vec![0];
        for (i, b) in self.content.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    /// The `(row, byte-column)` of a byte offset.
    fn row_col(&self, offset: usize) -> (usize, usize) {
        // Counts newlines before `offset` without building the whole-document
        // `line_starts` table: layout asks for the caret row on every frame.
        let before = &self.content.as_bytes()[..offset.min(self.content.len())];
        let row = before.iter().filter(|&&b| b == b'\n').count();
        let start = before
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        (row, offset - start)
    }

    /// Byte offset of the end of a row's text (before its `\n`, or the document
    /// end for the last row).
    fn line_end(&self, row: usize) -> usize {
        let starts = self.line_starts();
        starts
            .get(row + 1)
            .map(|&s| s - 1)
            .unwrap_or(self.content.len())
    }

    /// Offset one row up/down from the caret, preserving the byte column where
    /// possible. At the top/bottom edge, jumps to the document start/end.
    fn vertical_offset(&self, dir: i32) -> usize {
        let cursor = self.cursor_offset();
        let starts = self.line_starts();
        let (row, col) = self.row_col(cursor);
        let target = row as i32 + dir;
        if target < 0 {
            return 0;
        }
        if target as usize >= starts.len() {
            return self.content.len();
        }
        let target = target as usize;
        let target_start = starts[target];
        let target_len = self.line_end(target) - target_start;
        let mut new_col = col.min(target_len);
        while new_col > 0 && !self.content.is_char_boundary(target_start + new_col) {
            new_col -= 1;
        }
        target_start + new_col
    }

    /// Offset one *visual* row up/down from the caret, preserving the goal column
    /// (x) across the run, and whether it ends its row (upstream affinity: the
    /// goal is past the end of a row that broke inside a word). Falls back to
    /// logical-line movement before the first paint (when no wrapped layout is
    /// cached yet).
    fn move_vertical(&mut self, dir: i32) -> (usize, bool) {
        if self.wrapped.is_empty() {
            return (self.vertical_offset(dir), false);
        }
        let (row, col) = self.row_col(self.cursor_offset());
        let cur_lh = self.line_h(row);
        if cur_lh <= px(0.) {
            return (self.vertical_offset(dir), false);
        }
        let upstream = self.caret_upstream();
        let Some(cur) = self.wrapped.get(row).and_then(|l| {
            caret_line_pos(
                l,
                self.bidi_map(row),
                self.display_col(row, col),
                cur_lh,
                upstream,
            )
        }) else {
            return (self.vertical_offset(dir), false);
        };
        let global_y = self.line_tops[row] + cur.y;
        // The goal column is the caret's *visual* x, so it carries an RTL row's
        // right-align shift — the target row's shift comes off again below.
        // (Row insets stay out of it, as they always have.)
        let goal = self.goal_x.unwrap_or(cur.x + self.rtl_shift(row));
        self.goal_x = Some(goal);
        // Step to the adjacent visual row. Down: just below the current row's
        // bottom (= the top of the next one). Up: just above the current row's
        // top — robust to the row above having a different height (e.g. a
        // heading), since it doesn't depend on the current row's height. Both
        // stay a pixel clear of the boundary: deep in a document, f32 rounding
        // of the exact bottom resolves back onto the current wrap row, and the
        // caret (or a Shift+Down selection) stops dead on that line.
        let target_y = if dir >= 0 {
            global_y + cur_lh + px(1.)
        } else {
            global_y - px(1.)
        };
        if target_y < px(0.) {
            return (0, false);
        }
        let last = self.wrapped.len() - 1;
        let total = self.line_tops[last] + self.line_h(last) * self.row_span(last) as f32;
        if target_y >= total {
            return (self.visible_end(), false);
        }
        let mut trow = last;
        for i in 0..self.wrapped.len() {
            let h = self.line_h(i) * self.row_span(i) as f32;
            if target_y < self.line_tops[i] + h {
                trow = i;
                break;
            }
        }
        // A reserved gutter gap (a table's top/bottom, a code block's pads) belongs
        // to no row, and the loop assigns it to the row *after* it — right going
        // down, but going up that strands the caret on the far side of the gap (e.g.
        // just below a table). Going up, target the row before the gap instead.
        if dir < 0 && trow > 0 && target_y < self.line_tops[trow] {
            trow -= 1;
        }
        // A table separator (`|---|`) row isn't editable — skip past it (in the
        // direction of travel) so the caret lands on the header/body row rather
        // than dropping the whole table to raw source.
        if self
            .table_rows
            .get(trow)
            .and_then(Option::as_ref)
            .is_some_and(|t| t.is_separator)
        {
            let skip = if dir >= 0 {
                trow + 1
            } else {
                trow.wrapping_sub(1)
            };
            if skip < self.wrapped.len() {
                trow = skip;
            }
        }
        // A collapsed row (a hidden ``` fence, a folded body line) has no visual
        // height — landing there reveals it. Keep stepping in the direction of
        // travel so the caret skips over it; if the document runs out that way,
        // stay put.
        if self.line_h(trow) <= px(0.) {
            let step = |r: usize| {
                if dir >= 0 {
                    (r + 1 < self.wrapped.len()).then_some(r + 1)
                } else {
                    r.checked_sub(1)
                }
            };
            let mut r = trow;
            loop {
                match step(r) {
                    Some(n) if self.line_h(n) <= px(0.) => r = n,
                    Some(n) => {
                        trow = n;
                        break;
                    }
                    None => return (self.cursor_offset(), upstream),
                }
            }
        }
        let rel = point(
            (goal - self.rtl_shift(trow)).max(px(0.)),
            (target_y - self.line_tops[trow]).max(px(0.)),
        );
        let lh = self.line_h(trow);
        let col = line_index_at(&self.wrapped[trow], self.bidi_map(trow), rel, lh);
        let ends_row =
            self.bidi_map(trow).is_none() && ends_wrapped_row(&self.wrapped[trow], col, rel.y, lh);
        (
            self.line_starts()[trow] + self.source_col(trow, col),
            ends_row,
        )
    }

    /// The end of the next word at/after `offset` (⌥→ on macOS).
    fn next_word(&self, offset: usize) -> usize {
        self.content
            .unicode_word_indices()
            .map(|(i, w)| i + w.len())
            .find(|&end| end > offset)
            .unwrap_or(self.content.len())
    }

    /// The start of the next word after `offset`, or the end of its line if
    /// that comes first (Ctrl+→ and Ctrl+Delete on Windows).
    fn next_word_start(&self, offset: usize) -> usize {
        let word = self
            .content
            .unicode_word_indices()
            .map(|(i, _)| i)
            .find(|&start| start > offset)
            .unwrap_or(self.content.len());
        let line_end = self.content[offset.min(self.content.len())..]
            .char_indices()
            .skip(1)
            .find(|&(_, c)| c == '\n')
            .map_or(self.content.len(), |(i, _)| offset + i);
        word.min(line_end)
    }

    /// The document's last caret position that reveals nothing: the end of
    /// the last visible row, not a trailing collapsed one (a hidden closing
    /// code fence).
    fn visible_end(&self) -> usize {
        let Some(last) = self.wrapped.len().checked_sub(1) else {
            return self.content.len();
        };
        let mut r = last;
        while r > 0 && self.line_h(r) <= px(0.) {
            r -= 1;
        }
        if r == last {
            self.content.len()
        } else {
            self.line_end(r)
        }
    }

    /// The start of the previous word before `offset` (⌥← on macOS).
    fn prev_word(&self, offset: usize) -> usize {
        self.content
            .unicode_word_indices()
            .map(|(i, _)| i)
            .rfind(|&start| start < offset)
            .unwrap_or(0)
    }

    /// The byte range of the word at `offset` (double-click); `None` in whitespace.
    fn word_range_at(&self, offset: usize) -> Option<Range<usize>> {
        let mut ends_at = None;
        for (i, w) in self.content.unicode_word_indices() {
            let range = i..i + w.len();
            if range.start <= offset && offset < range.end {
                return Some(range);
            }
            if range.end == offset {
                ends_at = Some(range);
            }
        }
        ends_at
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let off = self.prev_word(self.cursor_offset());
        self.move_to(off, cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        let off = self.next_word(self.cursor_offset());
        self.move_to(off, cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.select_to(self.prev_word(self.cursor_offset()), cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.select_to(self.next_word(self.cursor_offset()), cx);
    }

    fn next_word_start_action(
        &mut self,
        _: &NextWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let off = self.next_word_start(self.cursor_offset());
        self.move_to(off, cx);
    }

    fn select_to_next_word_start(
        &mut self,
        _: &SelectToNextWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.goal_x = None;
        self.select_to(self.next_word_start(self.cursor_offset()), cx);
    }

    /// Ctrl+Backspace: delete back to the previous word start, or the selection.
    fn delete_word_left(
        &mut self,
        _: &DeleteWordLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let off = self.cursor_offset();
            let start = self.prev_word(off);
            if start == off {
                return;
            }
            self.select_to(start, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    /// Ctrl+Delete: delete forward to the next word start, or the selection.
    fn delete_word_right(
        &mut self,
        _: &DeleteWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let off = self.cursor_offset();
            let end = self.next_word_start(off);
            if end == off {
                return;
            }
            self.select_to(end, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    /// Ctrl+Home: the first position that reveals nothing, as when a
    /// document opens.
    fn document_start(&mut self, _: &DocumentStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(caret_off_marker_line(&self.content, 0), cx);
    }

    fn document_end(&mut self, _: &DocumentEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.visible_end(), cx);
    }

    fn select_to_document_start(
        &mut self,
        _: &SelectToDocumentStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.goal_x = None;
        self.select_to(0, cx);
    }

    fn select_to_document_end(
        &mut self,
        _: &SelectToDocumentEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.goal_x = None;
        self.select_to(self.visible_end(), cx);
    }

    /// The `src` of a file chip on the row at window `position`, if that row is a
    /// chip (from the last paint) — left-click opens it, right-click edits.
    fn chip_at(&self, position: Point<Pixels>) -> Option<SharedString> {
        if self.wrapped.is_empty() || self.chip_rows.iter().all(Option::is_none) {
            return None;
        }
        let bounds = self.last_bounds.as_ref()?;
        let rel_y = position.y - bounds.top();
        let mut row = self.wrapped.len() - 1;
        for i in 0..self.wrapped.len() {
            let h = self.line_h(i) * self.row_span(i) as f32;
            if rel_y < self.line_tops[i] + h {
                row = i;
                break;
            }
        }
        self.chip_rows.get(row).and_then(Option::clone)
    }

    /// If `position` lands on an inline image's bottom-right resize grip, the
    /// `(logical line, current display width)` of that image — so a press can
    /// start a corner-grip drag. The grip is the `IMG_GRIP`-side square pinned to
    /// each image's painted corner (see [`Self::image_grip`]); checked against the
    /// last paint's window-space `image_rects`.
    fn grip_at(&self, position: Point<Pixels>) -> Option<(usize, f32)> {
        self.image_rects.iter().find_map(|&(line, rect)| {
            Self::image_grip(rect)
                .contains(&position)
                .then_some((line, f32::from(rect.size.width)))
        })
    }

    /// The window-space bounds of an image's corner grip, given the image's
    /// painted `rect`. A small square overhanging the bottom-right corner (its
    /// center on the corner, like the reading view's), so it's easy to grab
    /// without covering much of the image.
    fn image_grip(rect: Bounds<Pixels>) -> Bounds<Pixels> {
        let s = px(IMG_GRIP);
        Bounds::new(
            point(rect.right() - s / 2., rect.bottom() - s / 2.),
            size(s, s),
        )
    }

    /// If `position` lands on a task checkbox painted last frame, the logical line
    /// of that task — so a click can toggle it. The hit area is the box padded a
    /// little, to stay easy to tap without swallowing the body text beside it.
    /// The code block whose opening fence is `fence_row`: its language token
    /// (empty for none) and the byte range of the body between the fences.
    fn code_block_at(&self, fence_row: usize) -> Option<(String, Range<usize>)> {
        let starts = self.line_starts();
        let &start = starts.get(fence_row)?;
        let fence_line = &self.content[start..self.line_end(fence_row)];
        let trimmed = fence_line.trim_start();
        let lang = trimmed.strip_prefix("```")?.trim().to_string();
        let body_start = (self.line_end(fence_row) + 1).min(self.content.len());
        let mut body_end = self.content.len();
        for (row, &row_start) in starts.iter().enumerate().skip(fence_row + 1) {
            if self.content[row_start..self.line_end(row)]
                .trim_start()
                .starts_with("```")
            {
                body_end = row_start.saturating_sub(1).max(body_start);
                break;
            }
        }
        Some((lang, body_start..body_end))
    }

    /// If `position` lands on a code card's chrome painted last frame:
    /// `(on_copy, fence_row)` — `true` = the Copy button, `false` = the
    /// language tag.
    fn code_chip_at(&self, position: Point<Pixels>) -> Option<(bool, usize)> {
        self.code_chip_rects.iter().find_map(|c| {
            if c.copy.contains(&position) {
                Some((true, c.fence_row))
            } else if c.lang.contains(&position) {
                Some((false, c.fence_row))
            } else {
                None
            }
        })
    }

    fn checkbox_at(&self, position: Point<Pixels>) -> Option<usize> {
        let pad = px(4.);
        self.checkbox_rects.iter().find_map(|&(line, rect)| {
            Bounds::new(
                point(rect.origin.x - pad, rect.origin.y - pad),
                size(rect.size.width + pad * 2., rect.size.height + pad * 2.),
            )
            .contains(&position)
            .then_some(line)
        })
    }

    /// If `position` lands on a foldable callout's chevron painted last frame,
    /// that marker's logical line — so a click can flip its fold char. Padded
    /// like the task checkbox to stay easy to hit.
    fn alert_fold_at(&self, position: Point<Pixels>) -> Option<usize> {
        let pad = px(4.);
        self.alert_fold_rects.iter().find_map(|&(line, rect)| {
            Bounds::new(
                point(rect.origin.x - pad, rect.origin.y - pad),
                size(rect.size.width + pad * 2., rect.size.height + pad * 2.),
            )
            .contains(&position)
            .then_some(line)
        })
    }

    /// If `position` lands on a heading's fold chevron painted last frame,
    /// that heading's logical line — so a click can toggle its fold. Padded
    /// like the callout chevron.
    fn heading_fold_at(&self, position: Point<Pixels>) -> Option<usize> {
        let pad = px(4.);
        self.heading_fold_rects.iter().find_map(|&(line, rect)| {
            Bounds::new(
                point(rect.origin.x - pad, rect.origin.y - pad),
                size(rect.size.width + pad * 2., rect.size.height + pad * 2.),
            )
            .contains(&position)
            .then_some(line)
        })
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        self.text_hit(position).0
    }

    /// The offset a click at `position` names, and whether it is the end of
    /// a soft-wrapped row (a click past a row's last character gives the
    /// caret upstream affinity there).
    fn text_hit(&self, position: Point<Pixels>) -> (usize, bool) {
        if self.content.is_empty() || self.wrapped.is_empty() {
            return (0, false);
        }
        let Some(bounds) = self.last_bounds.as_ref() else {
            return (0, false);
        };
        let rel = point(position.x - bounds.left(), position.y - bounds.top());
        // Which logical line, by the vertical band each occupies (variable height).
        let mut row = self.wrapped.len() - 1;
        for i in 0..self.wrapped.len() {
            let height = self.line_h(i) * self.row_span(i) as f32;
            if rel.y < self.line_tops[i] + height {
                row = i;
                break;
            }
        }
        // An inline-image row: clicking it puts the caret at the line start (the
        // line then shows its source — "raw on caret"), not a text column.
        if self.widget_rows.get(row).copied().unwrap_or(false) {
            return (self.line_starts()[row], false);
        }
        let x = (rel.x - self.row_origin_x(row)).max(px(0.));
        let line_rel = point(x, rel.y - self.line_tops[row]);
        let lh = self.line_h(row);
        let col = line_index_at(&self.wrapped[row], self.bidi_map(row), line_rel, lh);
        let ends_row = self.bidi_map(row).is_none()
            && ends_wrapped_row(&self.wrapped[row], col, line_rel.y, lh);
        (
            self.line_starts()[row] + self.source_col(row, col),
            ends_row,
        )
    }

    /// [`Self::text_hit`] through a table cell first, as clicks resolve.
    fn caret_hit(&self, position: Point<Pixels>, window: &mut Window) -> (usize, bool) {
        self.table_hit(position, window)
            .unwrap_or_else(|| self.text_hit(position))
    }

    /// Map a display byte column on `row` back to its source column. Identity
    /// unless the row's markers are hidden (W6), where the painted text is
    /// shorter than the source.
    fn source_col(&self, row: usize, display_col: usize) -> usize {
        match self.offset_maps.get(row).and_then(Option::as_ref) {
            Some(map) => map.get(display_col).copied().unwrap_or(display_col),
            None => display_col,
        }
    }

    /// Map a source byte column on `row` to its display column — the inverse of
    /// [`Self::source_col`], for positioning the caret/selection on a row whose
    /// markers are hidden (W6/#5). Uses the last painted map; in-paint code that
    /// has this frame's fresh map should call [`display_col_in`] directly.
    fn display_col(&self, row: usize, source_col: usize) -> usize {
        display_col_in(
            self.offset_maps.get(row).and_then(Option::as_ref),
            source_col,
        )
    }

    /// One VISIBLE position left of `offset`: a grapheme step, extended while
    /// the display column doesn't change — always-hidden formatting markers
    /// (`**`, `~~`, …) are zero-width on screen, so crossing one must not cost
    /// extra keypresses. Rows without a display map (raw/code) step plainly.
    fn prev_visible_boundary(&self, offset: usize) -> usize {
        let (row0, col0) = self.row_col(offset);
        let d0 = self.display_col(row0, col0);
        let mut off = self.previous_boundary(offset);
        loop {
            if off == 0 {
                return off;
            }
            let (row, col) = self.row_col(off);
            if row != row0
                || self.offset_maps.get(row).and_then(Option::as_ref).is_none()
                || self.display_col(row, col) != d0
            {
                return off;
            }
            off = self.previous_boundary(off);
        }
    }

    /// One VISIBLE position right of `offset` — see [`Self::prev_visible_boundary`].
    fn next_visible_boundary(&self, offset: usize) -> usize {
        let (row0, col0) = self.row_col(offset);
        let d0 = self.display_col(row0, col0);
        let mut off = self.next_boundary(offset);
        loop {
            if off >= self.content.len() {
                return self.content.len();
            }
            let (row, col) = self.row_col(off);
            if row != row0
                || self.offset_maps.get(row).and_then(Option::as_ref).is_none()
                || self.display_col(row, col) != d0
            {
                return off;
            }
            off = self.next_boundary(off);
        }
    }

    /// Cditor-style deletion planning around hidden formatting markers: the
    /// range a backspace (`back`) / forward delete should remove at `off`.
    /// Skips the invisible marker bytes to the adjacent VISIBLE character —
    /// and when removing it would empty its construct, the now-empty marker
    /// pair goes too (deleting bold's last char deletes the bold). `None` =
    /// no hidden markers involved (the plain grapheme deletion applies).
    fn fmt_delete_range(&self, off: usize, back: bool) -> Option<Range<usize>> {
        let st = self.markdown_style.as_ref()?;
        let (row, col) = self.row_col(off);
        let line_start = self.line_starts()[row];
        let line_end = self.line_end(row);
        let line = &self.content[line_start..line_end];
        // Inside a fenced code block the text is verbatim — no markers there.
        if *self.scan_data().fence_odd.get(row).unwrap_or(&false)
            || line.trim_start().starts_with("```")
        {
            return None;
        }
        let pairs = markdown_syntax::fmt_marker_pairs(line, st);
        if pairs.is_empty() {
            return None;
        }
        let markers: Vec<&Range<usize>> = pairs.iter().flat_map(|(o, c)| [o, c]).collect();
        // Skip marker bytes in the deletion direction to the visible char.
        let mut edge = col;
        if back {
            while let Some(sp) = markers.iter().find(|sp| sp.end == edge) {
                edge = sp.start;
            }
        } else {
            while let Some(sp) = markers.iter().find(|sp| sp.start == edge) {
                edge = sp.end;
            }
        }
        // The visible grapheme adjacent to the (possibly skipped-to) edge —
        // staying on this line; a line join takes the default path.
        let (del_start, del_end) = if back {
            if edge == 0 {
                return None;
            }
            let p = self.previous_boundary(line_start + edge) - line_start;
            (p, edge)
        } else {
            if edge >= line.len() {
                return None;
            }
            let n = self.next_boundary(line_start + edge).min(line_end) - line_start;
            (edge, n)
        };
        // Would this empty a construct? Only a real PAIR collapses: ITS opener
        // ending at the deletion's start and ITS closer starting at the end
        // (adjacent different constructs must not fuse).
        let emptied = pairs
            .iter()
            .find(|(o, c)| o.end == del_start && c.start == del_end);
        let range = match emptied {
            Some((o, c)) => o.start..c.end,
            None if edge == col => return None, // no markers were involved
            None => del_start..del_end,
        };
        Some(line_start + range.start..line_start + range.end)
    }
}

/// Wrap a keyboard action so the caret it moves is kept in view (ADR 0054).
fn revealing<A: 'static>(
    action: fn(&mut EditorState, &A, &mut Window, &mut Context<EditorState>),
) -> impl Fn(&mut EditorState, &A, &mut Window, &mut Context<EditorState>) + 'static {
    move |editor, a, window, cx| {
        action(editor, a, window, cx);
        editor.reveal_caret = true;
        cx.notify();
    }
}

impl Focusable for EditorState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<EditorEvent> for EditorState {}

impl Render for EditorState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(revealing(Self::backspace)))
            .on_action(cx.listener(revealing(Self::delete)))
            .on_action(cx.listener(revealing(Self::left)))
            .on_action(cx.listener(revealing(Self::right)))
            .on_action(cx.listener(revealing(Self::up)))
            .on_action(cx.listener(revealing(Self::down)))
            .on_action(cx.listener(revealing(Self::home)))
            .on_action(cx.listener(revealing(Self::end)))
            .on_action(cx.listener(revealing(Self::select_left)))
            .on_action(cx.listener(revealing(Self::select_right)))
            .on_action(cx.listener(revealing(Self::select_up)))
            .on_action(cx.listener(revealing(Self::select_down)))
            .on_action(cx.listener(revealing(Self::word_left)))
            .on_action(cx.listener(revealing(Self::word_right)))
            .on_action(cx.listener(revealing(Self::select_word_left)))
            .on_action(cx.listener(revealing(Self::select_word_right)))
            .on_action(cx.listener(revealing(Self::next_word_start_action)))
            .on_action(cx.listener(revealing(Self::select_to_next_word_start)))
            .on_action(cx.listener(revealing(Self::delete_word_left)))
            .on_action(cx.listener(revealing(Self::delete_word_right)))
            .on_action(cx.listener(revealing(Self::document_start)))
            .on_action(cx.listener(revealing(Self::document_end)))
            .on_action(cx.listener(revealing(Self::select_to_document_start)))
            .on_action(cx.listener(revealing(Self::select_to_document_end)))
            .on_action(cx.listener(revealing(Self::line_start)))
            .on_action(cx.listener(revealing(Self::line_end_action)))
            .on_action(cx.listener(revealing(Self::select_to_line_start)))
            .on_action(cx.listener(revealing(Self::select_to_line_end)))
            .on_action(cx.listener(revealing(Self::delete_to_line_start)))
            .on_action(cx.listener(revealing(Self::delete_to_word_end)))
            .on_action(cx.listener(revealing(Self::select_all)))
            .on_action(cx.listener(revealing(Self::newline)))
            .on_action(cx.listener(revealing(Self::line_break)))
            .on_action(cx.listener(revealing(Self::bold)))
            .on_action(cx.listener(revealing(Self::italic)))
            .on_action(cx.listener(revealing(Self::underline)))
            .on_action(cx.listener(revealing(Self::strike)))
            .on_action(cx.listener(revealing(Self::code)))
            .on_action(cx.listener(revealing(Self::indent)))
            .on_action(cx.listener(revealing(Self::outdent)))
            .on_action(cx.listener(revealing(Self::paste)))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(revealing(Self::cut)))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(revealing(Self::undo)))
            .on_action(cx.listener(revealing(Self::redo)))
            .on_action(cx.listener(Self::dismiss))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_middle_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .child(EditorElement {
                editor: cx.entity(),
            })
            // Right-click suggestions menu, absolutely positioned over the
            // editor (anchored at the click). `Option`'s `IntoIterator` renders
            // zero or one popup; clicking a row replaces the misspelled span.
            .children(self.menu.clone().map(|menu| {
                let DiagMenu {
                    anchor,
                    range,
                    suggestions,
                    suggesting,
                    scroll,
                    turn_into: menu_turn_into,
                } = menu;
                let count = suggestions.len();
                // A flagged word without rows yet says why: still looking, or none.
                let status = (count == 0 && !range.is_empty()).then_some(if suggesting {
                    "Finding suggestions…"
                } else {
                    "No suggestions"
                });
                // Menu chrome from the host's theme (fallbacks match the former
                // hardcoded dark menu when no markdown style is set).
                let st = self.markdown_style.as_ref();
                let menu_bg = st.map_or(rgb(0x26262b).into(), |s| s.popover_bg);
                let menu_border = st.map_or(rgb(0x45454c).into(), |s| s.popover_border);
                let menu_fg = st.map_or(rgb(0xe6e6e6).into(), |s| s.popover_fg);
                let hover = st.map_or(rgba(0x2f6fd628).into(), |s| s.popover_hover);
                let mut thumb_c = st.map_or(rgba(0xffffff66).into(), |s| s.marker);
                thumb_c.a = 0.5;
                // Host actions for the flagged word (Ignore, Add to dictionary).
                let has_actions = !range.is_empty() && !self.diagnostic_actions.is_empty();
                let diagnostic_actions = has_actions.then(|| {
                    let word = self.content[range.clone()].to_string();
                    self.diagnostic_actions
                        .iter()
                        .enumerate()
                        .map(|(i, label)| {
                            let word = word.clone();
                            div()
                                .id(("diagnostic-action", i))
                                .flex_shrink_0()
                                .px(px(10.))
                                .py(px(3.))
                                .hover(move |s| s.bg(hover))
                                .child(label.clone())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        cx.emit(EditorEvent::DiagnosticAction(i, word.clone()));
                                        cx.notify();
                                    }),
                                )
                        })
                        .collect::<Vec<_>>()
                });
                // Collected eagerly (not a lazy iterator) so `cx` is only
                // borrowed here and stays free for the menu's own listeners below.
                let rows: Vec<_> = suggestions
                    .into_iter()
                    .enumerate()
                    .map(|(i, sugg)| {
                        let range = range.clone();
                        let replacement = sugg.to_string();
                        div()
                            // A stable per-row id so gpui tracks hover state and
                            // repaints as the pointer moves between rows. Without
                            // an id, the hover style only shows on a forced
                            // repaint (e.g. while scrolling).
                            .id(("suggestion-row", i))
                            // Don't let the scroll container's max-height squeeze
                            // the rows; they keep their height and overflow.
                            .flex_shrink_0()
                            .px(px(10.))
                            .py(px(3.))
                            // Highlight the row under the pointer.
                            .hover(move |s| s.bg(hover))
                            .child(sugg)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |editor, _: &MouseDownEvent, window, cx| {
                                    // Keep the editor's own mouse-down from clearing
                                    // the menu / moving the caret out from under us.
                                    cx.stop_propagation();
                                    editor.apply_suggestion(
                                        range.clone(),
                                        &replacement,
                                        window,
                                        cx,
                                    );
                                }),
                            )
                    })
                    .collect();
                // A thin scrollbar thumb, shown when the list overflows ~6 rows
                // so the scroll affordance is visible. Sized from the row count
                // (known now) and positioned from the live scroll offset — a
                // wheel scroll calls window.refresh(), which re-renders this.
                const ROW_H: f32 = 24.0;
                const PAD: f32 = 4.0;
                const MAX_H: f32 = 180.0;
                let rows_h = count as f32 * ROW_H;
                let view_h = MAX_H - 2.0 * PAD;
                let thumb = (rows_h > view_h).then(|| {
                    let scrolled = (-f32::from(scroll.offset().y)).clamp(0.0, rows_h - view_h);
                    let thumb_h = (view_h * view_h / rows_h).max(24.0);
                    let thumb_top = PAD + scrolled / (rows_h - view_h) * (view_h - thumb_h);
                    div()
                        .absolute()
                        .top(px(thumb_top))
                        .right(px(2.))
                        .w(px(6.))
                        .h(px(thumb_h))
                        .rounded(px(3.))
                        .bg(thumb_c)
                });

                // Cut / Copy need a selection; Paste always applies (the caret
                // was seated at the click when it landed outside the selection).
                let has_sel = !self.selected_range.is_empty();
                let clip_item = |id: &'static str, label: SharedString| {
                    div()
                        .id(id)
                        .flex_shrink_0()
                        .px(px(10.))
                        .py(px(3.))
                        .hover(move |s| s.bg(hover))
                        .child(label)
                };
                let mut clipboard = div().flex().flex_col().py(px(4.));
                if has_sel {
                    clipboard = clipboard
                        .child(
                            clip_item("menu-cut", SharedString::new_static("Cut")).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    editor.menu = None;
                                    editor.cut(&Cut, window, cx);
                                }),
                            ),
                        )
                        .child(
                            clip_item("menu-copy", SharedString::new_static("Copy")).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    editor.menu = None;
                                    editor.copy(&Copy, window, cx);
                                }),
                            ),
                        )
                        // Plain-only: the raw markdown with no host flavors —
                        // for pasting literal source into rich surfaces
                        // (email, chat) where Copy's HTML flavor would win.
                        .child(
                            clip_item("menu-copy-md", SharedString::new_static("Copy as Markdown"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.copy_plain(window, cx);
                                    }),
                                ),
                        );
                }
                let clipboard = clipboard.child(
                    clip_item("menu-paste", SharedString::new_static("Paste")).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            editor.menu = None;
                            editor.paste(&Paste, window, cx);
                        }),
                    ),
                );

                let host_item = self.selection_action_item(menu_fg, hover, cx);

                // Inline-format bar (Cditor-style): with a selection, a strip of
                // B / I / S / <> buttons across the menu's top — each toggles its
                // markdown wrap on the selection and closes the menu.
                let fmt_btn = |id: &'static str| {
                    div()
                        .id(id)
                        .w(px(28.))
                        .h(px(24.))
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(move |s| s.bg(hover))
                };
                let format_bar = has_sel.then(|| {
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(2.))
                        .px(px(6.))
                        .py(px(4.))
                        .child(
                            fmt_btn("menu-fmt-bold")
                                .font_weight(FontWeight::BOLD)
                                .child("B")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.bold(&Bold, window, cx);
                                    }),
                                ),
                        )
                        .child(
                            fmt_btn("menu-fmt-italic")
                                .italic()
                                .child("I")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.italic(&Italic, window, cx);
                                    }),
                                ),
                        )
                        .child(
                            fmt_btn("menu-fmt-underline")
                                .underline()
                                .child("U")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.underline(&Underline, window, cx);
                                    }),
                                ),
                        )
                        .child(
                            fmt_btn("menu-fmt-strike")
                                .line_through()
                                .child("S")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.strike(&Strike, window, cx);
                                    }),
                                ),
                        )
                        .child(
                            fmt_btn("menu-fmt-code")
                                .font_family(
                                    self.markdown_style
                                        .as_ref()
                                        .map(|s| s.mono.family.clone())
                                        .unwrap_or_else(|| "monospace".into()),
                                )
                                .text_size(px(12.))
                                .child("<>")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.code(&Code, window, cx);
                                    }),
                                ),
                        )
                });

                // "Turn into" block conversion (Cditor-style): a row whose
                // hover opens a kind-list flyout beside the menu, the caret
                // block's current kind checked.
                let cur_kind = self.block_kind_at(self.row_col(self.selected_range.start).0);
                let turn_row = div()
                    .id("menu-turn-into")
                    .flex_shrink_0()
                    .px(px(10.))
                    .py(px(3.))
                    .hover(move |s| s.bg(hover))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(16.))
                    .child(SharedString::new_static("Turn into"))
                    .child(div().text_size(px(10.)).child("\u{25b8}"))
                    .on_hover(cx.listener(|editor, hovered: &bool, _, cx| {
                        if *hovered
                            && let Some(m) = editor.menu.as_mut()
                            && !m.turn_into
                        {
                            m.turn_into = true;
                            cx.notify();
                        }
                    }));
                let turn_flyout = menu_turn_into.then(|| {
                    let rows: Vec<_> = TurnKind::ALL
                        .iter()
                        .enumerate()
                        .map(|(i, &k)| {
                            let checked = k == cur_kind;
                            div()
                                .id(("turn-kind", i))
                                .flex_shrink_0()
                                .px(px(10.))
                                .py(px(3.))
                                .hover(move |s| s.bg(hover))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(6.))
                                .child(div().w(px(12.)).flex_shrink_0().child(if checked {
                                    "\u{2713}"
                                } else {
                                    ""
                                }))
                                .child(k.label())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.turn_into(k, window, cx);
                                    }),
                                )
                        })
                        .collect();
                    // An absolute sibling of the (overflow-clipped) menu box —
                    // out of flow, so it can't inflate the anchored bounds and
                    // re-trigger the window snap (the slash-flyout lesson).
                    div()
                        .absolute()
                        .left(gpui::relative(1.))
                        .bottom(px(0.))
                        .ml(px(2.))
                        .occlude()
                        .min_w(px(140.))
                        .cursor(CursorStyle::Arrow)
                        .bg(menu_bg)
                        .border_1()
                        .border_color(menu_border)
                        .rounded(px(6.))
                        .shadow_md()
                        .text_color(menu_fg)
                        .text_size(px(13.))
                        .flex()
                        .flex_col()
                        .py(px(4.))
                        .children(rows)
                });

                // Deferred + anchored to a window-space top layer with `.occlude()`,
                // so it renders above the page chrome and captures the wheel — else a
                // scroll over the popup scrolls the page behind it.
                gpui::deferred(
                    gpui::anchored().position(anchor).snap_to_window().child(
                        div().relative().children(turn_flyout).child(
                            div()
                                .relative()
                                .occlude()
                                .min_w(px(150.))
                                // Override the editor's I-beam — the menu is a normal
                                // pointer surface (children inherit this hitbox's cursor).
                                .cursor(CursorStyle::Arrow)
                                .bg(menu_bg)
                                .border_1()
                                .border_color(menu_border)
                                .rounded(px(6.))
                                .shadow_md()
                                // Clip rows + thumb to the rounded box.
                                .overflow_hidden()
                                .text_color(menu_fg)
                                .text_size(px(13.))
                                // A click anywhere outside the menu dismisses it.
                                .on_mouse_down_out(cx.listener(
                                    |editor, _: &MouseDownEvent, _, cx| {
                                        editor.menu = None;
                                        cx.notify();
                                    },
                                ))
                                .children(
                                    host_item
                                        .map(|item| div().flex().flex_col().py(px(4.)).child(item)),
                                )
                                .children(
                                    self.live_selection_action()
                                        .map(|_| div().h(px(1.)).bg(menu_border)),
                                )
                                .children(format_bar)
                                .children(has_sel.then(|| div().h(px(1.)).bg(menu_border)))
                                .children((count > 0).then(|| {
                                    // The scroll viewport: shows ~6 rows, the rest scroll.
                                    div()
                                        .id("suggestion-menu")
                                        .max_h(px(MAX_H))
                                        .overflow_y_scroll()
                                        .track_scroll(&scroll)
                                        .flex()
                                        .flex_col()
                                        .py(px(PAD))
                                        .children(rows)
                                }))
                                .children(status.map(|label| {
                                    div()
                                        .px(px(10.))
                                        .py(px(7.))
                                        .text_color(Hsla {
                                            a: menu_fg.a * 0.55,
                                            ..menu_fg
                                        })
                                        .child(label)
                                }))
                                .children(
                                    (count > 0 || status.is_some())
                                        .then(|| div().h(px(1.)).bg(menu_border)),
                                )
                                .children(
                                    diagnostic_actions.map(|rows| {
                                        div().flex().flex_col().py(px(4.)).children(rows)
                                    }),
                                )
                                .children(has_actions.then(|| div().h(px(1.)).bg(menu_border)))
                                .child(clipboard)
                                .child(div().h(px(1.)).bg(menu_border))
                                .child(div().flex().flex_col().py(px(4.)).child(turn_row))
                                .children(thumb),
                        ),
                    ),
                )
            }))
            .children(self.selection_pill(window, cx))
            // The table right-click menu (Word-style row/column editing), anchored
            // at the click; each row runs its action on the caret's table cell.
            .children(self.table_menu.map(|anchor| {
                // Menu chrome from the host's theme (fallbacks match the former
                // hardcoded dark menu when no markdown style is set).
                let st = self.markdown_style.as_ref();
                let menu_bg = st.map_or(rgb(0x26262b).into(), |s| s.popover_bg);
                let menu_border = st.map_or(rgb(0x45454c).into(), |s| s.popover_border);
                let menu_fg = st.map_or(rgb(0xe6e6e6).into(), |s| s.popover_fg);
                let hover = st.map_or(rgba(0x2f6fd628).into(), |s| s.popover_hover);
                let divider = st.map_or(rgba(0xffffff2e).into(), |s| s.popover_divider);
                let mut thumb_c = st.map_or(rgba(0xffffff66).into(), |s| s.marker);
                thumb_c.a = 0.5;
                const ROW_H: f32 = 24.0;
                const DIV_H: f32 = 9.0;
                const PAD: f32 = 4.0;
                const MAX_H: f32 = 480.0;
                // Cditor-style grouped rows: a glyph column, a checkmark on the
                // current align/style, and the destructive group in red.
                let danger = st.map_or(rgb(0xE5484D).into(), |s| s.popover_danger);
                let cur_align = self.caret_table_align();
                let cur_style = self
                    .caret_table_region()
                    .map(|r| r.style)
                    .unwrap_or_default();
                use markdown_syntax::TableStyle as TS;
                enum Row {
                    Div,
                    Item {
                        glyph: &'static str,
                        label: SharedString,
                        action: TableMenuAction,
                        red: bool,
                        checked: bool,
                    },
                }
                let item = |glyph, label, action| Row::Item {
                    glyph,
                    label,
                    action,
                    red: false,
                    checked: false,
                };
                let specs = [
                    item(
                        "↑",
                        SharedString::new_static("Insert row above"),
                        TableMenuAction::InsertRowAbove,
                    ),
                    item(
                        "↓",
                        SharedString::new_static("Insert row below"),
                        TableMenuAction::InsertRowBelow,
                    ),
                    item(
                        "⧉",
                        SharedString::new_static("Duplicate row"),
                        TableMenuAction::DuplicateRow,
                    ),
                    Row::Div,
                    item(
                        "←",
                        SharedString::new_static("Insert column left"),
                        TableMenuAction::InsertColLeft,
                    ),
                    item(
                        "→",
                        SharedString::new_static("Insert column right"),
                        TableMenuAction::InsertColRight,
                    ),
                    Row::Div,
                    Row::Item {
                        glyph: "",
                        label: SharedString::new_static("Align left"),
                        action: TableMenuAction::AlignLeft,
                        red: false,
                        checked: cur_align == Some(CellAlign::Left),
                    },
                    Row::Item {
                        glyph: "",
                        label: SharedString::new_static("Align center"),
                        action: TableMenuAction::AlignCenter,
                        red: false,
                        checked: cur_align == Some(CellAlign::Center),
                    },
                    Row::Item {
                        glyph: "",
                        label: SharedString::new_static("Align right"),
                        action: TableMenuAction::AlignRight,
                        red: false,
                        checked: cur_align == Some(CellAlign::Right),
                    },
                    Row::Div,
                    Row::Item {
                        glyph: "▦",
                        label: SharedString::new_static("Grid style"),
                        action: TableMenuAction::SetStyle(None),
                        red: false,
                        checked: cur_style == TS::Grid,
                    },
                    Row::Item {
                        glyph: "▤",
                        label: SharedString::new_static("Striped style"),
                        action: TableMenuAction::SetStyle(Some("striped")),
                        red: false,
                        checked: cur_style == TS::Striped,
                    },
                    Row::Item {
                        glyph: "▥",
                        label: SharedString::new_static("Header style"),
                        action: TableMenuAction::SetStyle(Some("header")),
                        red: false,
                        checked: cur_style == TS::Header,
                    },
                    Row::Item {
                        glyph: "─",
                        label: SharedString::new_static("Minimal style"),
                        action: TableMenuAction::SetStyle(Some("minimal")),
                        red: false,
                        checked: cur_style == TS::Minimal,
                    },
                    Row::Div,
                    item(
                        "⊞",
                        SharedString::new_static("Copy as Markdown"),
                        TableMenuAction::CopyTable,
                    ),
                    Row::Div,
                    Row::Item {
                        glyph: "✕",
                        label: SharedString::new_static("Delete row"),
                        action: TableMenuAction::DeleteRow,
                        red: true,
                        checked: false,
                    },
                    Row::Item {
                        glyph: "✕",
                        label: SharedString::new_static("Delete column"),
                        action: TableMenuAction::DeleteColumn,
                        red: true,
                        checked: false,
                    },
                    Row::Item {
                        glyph: "✕",
                        label: SharedString::new_static("Delete table"),
                        action: TableMenuAction::DeleteTable,
                        red: true,
                        checked: false,
                    },
                ];
                let n_items = specs
                    .iter()
                    .filter(|r| matches!(r, Row::Item { .. }))
                    .count();
                let n_divs = specs.len() - n_items;
                let mut rows: Vec<gpui::AnyElement> = Vec::new();
                for (i, spec) in specs.into_iter().enumerate() {
                    match spec {
                        Row::Div => rows.push(
                            div()
                                .flex_shrink_0()
                                .h(px(1.))
                                .my(px(4.))
                                .mx(px(8.))
                                .bg(divider)
                                .into_any_element(),
                        ),
                        Row::Item {
                            glyph,
                            label,
                            action,
                            red,
                            checked,
                        } => {
                            let fg = if red { danger } else { menu_fg };
                            let mut glyph_c = fg;
                            glyph_c.a *= 0.7;
                            rows.push(
                                div()
                                    .id(("table-menu-row", i))
                                    .flex_shrink_0()
                                    .px(px(10.))
                                    .py(px(3.))
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_color(fg)
                                    .hover(move |s| s.bg(hover))
                                    .child(
                                        div()
                                            .w(px(16.))
                                            .flex_none()
                                            .text_color(glyph_c)
                                            .child(glyph),
                                    )
                                    .child(div().flex_1().child(label))
                                    .children(checked.then(|| div().text_color(glyph_c).child("✓")))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            action.apply(editor, cx);
                                        }),
                                    )
                                    .into_any_element(),
                            );
                        }
                    }
                }
                // Scrollbar thumb, shown when the items overflow the cap — sized from
                // the content height + positioned from the live scroll offset.
                let rows_h = n_items as f32 * ROW_H + n_divs as f32 * DIV_H;
                let view_h = MAX_H - 2.0 * PAD;
                let thumb = (rows_h > view_h).then(|| {
                    let scrolled =
                        (-f32::from(self.table_menu_scroll.offset().y)).clamp(0.0, rows_h - view_h);
                    let thumb_h = (view_h * view_h / rows_h).max(24.0);
                    let thumb_top = PAD + scrolled / (rows_h - view_h) * (view_h - thumb_h);
                    div()
                        .absolute()
                        .top(px(thumb_top))
                        .right(px(2.))
                        .w(px(6.))
                        .h(px(thumb_h))
                        .rounded(px(3.))
                        .bg(thumb_c)
                });
                gpui::deferred(
                    gpui::anchored().position(anchor).snap_to_window().child(
                        div()
                            .relative()
                            .occlude()
                            .min_w(px(190.))
                            .cursor(CursorStyle::Arrow)
                            .bg(menu_bg)
                            .border_1()
                            .border_color(menu_border)
                            .rounded(px(6.))
                            .shadow_md()
                            .overflow_hidden()
                            .text_color(menu_fg)
                            .text_size(px(13.))
                            .on_mouse_down_out(cx.listener(|editor, _: &MouseDownEvent, _, cx| {
                                editor.table_menu = None;
                                cx.notify();
                            }))
                            .child(
                                // Inner scroll viewport: caps the height + scrolls the
                                // overflow (max_h on a separate flex-col div, like the
                                // suggestion menu — combining it with the styled box
                                // above doesn't cap).
                                div()
                                    .id("table-menu")
                                    .max_h(px(MAX_H))
                                    .overflow_y_scroll()
                                    .track_scroll(&self.table_menu_scroll)
                                    .flex()
                                    .flex_col()
                                    .py(px(PAD))
                                    .children(rows),
                            )
                            .children(thumb),
                    ),
                )
            }))
            // The image right-click menu: Word-style object actions on an inline
            // image (Delete), anchored at the click. Chrome matches the table menu.
            .children(self.image_menu.map(|(line, anchor)| {
                let st = self.markdown_style.as_ref();
                let menu_bg = st.map_or(rgb(0x26262b).into(), |s| s.popover_bg);
                let menu_border = st.map_or(rgb(0x45454c).into(), |s| s.popover_border);
                let menu_fg = st.map_or(rgb(0xe6e6e6).into(), |s| s.popover_fg);
                let hover = st.map_or(rgba(0x2f6fd628).into(), |s| s.popover_hover);
                gpui::deferred(
                    gpui::anchored().position(anchor).snap_to_window().child(
                        div()
                            .occlude()
                            .min_w(px(140.))
                            .cursor(CursorStyle::Arrow)
                            .bg(menu_bg)
                            .border_1()
                            .border_color(menu_border)
                            .rounded(px(6.))
                            .shadow_md()
                            .overflow_hidden()
                            .text_color(menu_fg)
                            .text_size(px(13.))
                            .py(px(4.))
                            .on_mouse_down_out(cx.listener(|editor, _: &MouseDownEvent, _, cx| {
                                editor.image_menu = None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .id("image-menu-delete")
                                    .px(px(10.))
                                    .py(px(3.))
                                    .hover(move |s| s.bg(hover))
                                    .child(SharedString::new_static("Delete image"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            editor.image_menu = None;
                                            editor.delete_image_row(line, cx);
                                        }),
                                    ),
                            ),
                    ),
                )
            }))
    }
}

/// The shaped width of `text` at `font_size` — used to inset a gutter line's body
/// to exactly where its (hidden) source prefix ends, so the rendered + raw views
/// line up (and tab/space nesting matches the actual whitespace width).
fn measure_width(window: &mut Window, text: &str, font: &Font, font_size: Pixels) -> Pixels {
    if text.is_empty() {
        return px(0.);
    }
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(
            SharedString::from(text.to_string()),
            font_size,
            &[run],
            None,
        )
        .width()
}

/// Shape `text` with pre-built `runs`, so diagnostics can underline specific
/// spans. The plain-run [`shape_all`] is used for the placeholder + measurement.
fn shape_runs(
    window: &mut Window,
    text: &SharedString,
    font_size: Pixels,
    runs: &[TextRun],
    wrap_width: Option<Pixels>,
) -> Vec<WrappedLine> {
    window
        .text_system()
        .shape_text(text.clone(), font_size, runs, wrap_width, None)
        .map(|lines| lines.into_vec())
        .unwrap_or_default()
}

/// One logical line's shaped layout. Shared, so offscreen placeholder rows
/// can reuse a single empty layout.
type ShapedLine = std::rc::Rc<WrappedLine>;

/// A line currently rendered as an inline image (W4) instead of its source text:
/// the decoded image plus its fit-to-width display size (logical px).
#[derive(Clone)]
struct BlockImg {
    img: Arc<RenderImage>,
    width: Pixels,
    height: Pixels,
}

/// One inline `![](src)` image painted within a text line. `display_off` is the byte offset
/// of its invisible spacer in the shaped DISPLAY string (resolved to an x via the wrapped line
/// at paint); `img`/`width`/`height` are the raster scaled to text size.
#[derive(Clone)]
struct InlineImage {
    display_off: usize,
    /// Byte length of the spacer this raster sits over. On an RTL row
    /// `display_off` is its RIGHT edge, so the whole span is needed to find the
    /// left one — see where it is painted.
    len: usize,
    img: Arc<RenderImage>,
    width: Pixels,
    height: Pixels,
}

/// A line rendered as a block widget instead of its source text: a standalone
/// image, or a clickable file chip (e.g. a PDF — left-click opens it, right-click
/// edits). Shown only while the caret is off the line ("raw on caret").
#[derive(Clone)]
enum Block {
    Image(BlockImg),
    Chip {
        src: SharedString,
        label: SharedString,
        /// Label color (accent, signalling clickable), box fill, box border.
        link: Hsla,
        bg: Hsla,
        border: Hsla,
        height: Pixels,
    },
}

impl Block {
    fn height(&self) -> Pixels {
        match self {
            Block::Image(i) => i.height,
            Block::Chip { height, .. } => *height,
        }
    }
}

/// A fenced-code-block line's background (W4b/refinement): the block reads as one
/// rounded, content-fit box (sized to its widest line, like a table — not the
/// full editor width). Each line carries the block color, the shared box width
/// (back-patched once the block's extent is known), and whether it's the
/// first/last visible line (to round the box's top/bottom corners).
/// Last-frame hit rects for one code card's chrome (see `code_chip_rects`).
#[derive(Clone)]
struct CodeChipHit {
    lang: Bounds<Pixels>,
    copy: Bounds<Pixels>,
    fence_row: usize,
}

/// A code card's top-right chrome, laid out in prepaint: the language tag and
/// Copy button (Cditor-inspired, issue #16). Geometry is window-space; paint
/// draws at these bounds and the hitboxes flip the cursor.
struct CodeChip {
    lang_text: SharedString,
    copy_text: SharedString,
    lang_bounds: Bounds<Pixels>,
    copy_bounds: Bounds<Pixels>,
    fence_row: usize,
    /// Card background — the labels sit on an opaque pill of it so they stay
    /// readable over a long first line.
    bg: Hsla,
    fg: Hsla,
    lang_hb: Hitbox,
    copy_hb: Hitbox,
}

#[derive(Clone, Copy)]
struct CodeBg {
    color: Hsla,
    width: Pixels,
    top: bool,
    bottom: bool,
}

/// A table row rendered as a grid (W4c): its cells, per-column alignment, the
/// content-fit per-column widths (shared across the table), header/separator/
/// last-row flags, and the border color. Built only when the caret is outside
/// the table — the caret's table shows source instead ("raw on caret").
#[derive(Clone)]
struct TableRow {
    cells: Vec<SharedString>,
    /// Byte range of each cell's trimmed content within its source line — for
    /// placing the caret inside a cell + hit-testing a click back to a source
    /// offset (in-cell editing).
    cell_ranges: Vec<Range<usize>>,
    aligns: Vec<markdown_syntax::Align>,
    col_widths: Vec<Pixels>,
    is_header: bool,
    is_separator: bool,
    is_last: bool,
    /// 0-based position among the body rows (`None` for header/separator) — drives
    /// striping (shade odd indices) + the rule-under-header (index 0).
    body_index: Option<usize>,
    /// The table's visual style (from its `<!-- table:STYLE -->` marker).
    style: markdown_syntax::TableStyle,
    border: Hsla,
    /// Row-shade color for striped / header-shaded styles (a faint tint).
    shade: Hsla,
    /// The table reads right-to-left (#66) — from its region's
    /// [`markdown_syntax::TableRegion::rtl`], so every row of one table agrees.
    rtl: bool,
}

/// A per-line "gutter" decoration: a left-margin treatment that hides its source
/// marker and renders something in its place, with the body text inset to make
/// Task checkbox edge, as a fraction of the line's font size — shared by the
/// shaping (body inset), the pointer hitbox, and the paint so they agree.
const CHECKBOX_SCALE: f32 = 0.9;

/// room. Covers blockquotes now; list bullets + task checkboxes reuse it.
#[derive(Clone, Copy)]
enum LineMark {
    /// Blockquote: a left border (`bar`); the `>` markers are hidden. `text`
    /// colors the body — `Some` = muted quote tone, `None` = the editor's
    /// normal text color (alert bodies).
    Quote { bar: Hsla, text: Option<Hsla> },
    /// A GitHub alert's marker line (`> [!NOTE]` …): the marker is hidden and
    /// a bold `label` paints in the alert color; any same-line body insets to
    /// `text_inset` (QUOTE_INSET + the label's measured width + a gap) — the
    /// list-bullet pattern. Continuation lines are `Quote` marks with the
    /// alert's bar color.
    Alert {
        bar: Hsla,
        label: &'static str,
        text_inset: Pixels,
        /// Foldable callout (`[!NOTE]-`/`+`): `Some(true)` = folded. A chevron
        /// paints at `chevron_x` (after the label) and clicking it flips the
        /// fold char in the source.
        fold: Option<bool>,
        chevron_x: Pixels,
    },
    /// List item: a painted bullet (`•`) or number (`N.`) at `bullet_x` (where the
    /// hidden source marker began), muted; the body sits at `text_inset` — the
    /// measured width of the whole source prefix, so the rendered + raw views
    /// line up exactly and tab/space nesting stays in sync.
    List {
        bullet_x: Pixels,
        text_inset: Pixels,
        ordered: bool,
        num: u32,
        /// Structural nesting level (0 = top), for the Word-style marker
        /// scheme (`1.` -> `a.` -> `i.`).
        level: usize,
        color: Hsla,
    },
    /// GFM task item: a painted ☐/☑ box at `bullet_x`, muted; the body sits at
    /// `text_inset` (measured prefix width) like a list item.
    Check {
        bullet_x: Pixels,
        text_inset: Pixels,
        checked: bool,
        color: Hsla,
        /// Fill for a done box (the host's link/accent color; white check on top).
        accent: Hsla,
    },
    /// Thematic break (`---`): a full-width muted divider painted in place of the
    /// source; the line has no body text (reveal-on-caret shows the raw `---`).
    Rule(Hsla),
}

impl LineMark {
    /// Horizontal inset (px) applied to the body text + caret for this mark.
    fn inset(self) -> Pixels {
        match self {
            LineMark::Quote { .. } => px(QUOTE_INSET),
            LineMark::Alert { text_inset, .. } => text_inset,
            LineMark::List { text_inset, .. } | LineMark::Check { text_inset, .. } => text_inset,
            LineMark::Rule(_) => px(0.),
        }
    }
}

/// Per-logical-line shaping output — parallel vecs of equal length: the shaped
/// source line, its row height, an optional inline-image widget, an optional
/// fenced-code-block background, an optional table-row grid, the display→source
/// map, and an optional gutter decoration (blockquote / list / checkbox).
/// One shaped document: per-line parallel channels, all the same length —
/// the per-line loop's normal push and [`ShapedDoc::push_placeholder`] are
/// the only writers, so the lockstep invariant lives here.
#[derive(Default)]
struct ShapedDoc {
    wrapped: Vec<ShapedLine>,
    heights: Vec<Pixels>,
    widgets: Vec<Option<Block>>,
    backgrounds: Vec<Option<CodeBg>>,
    tables: Vec<Option<TableRow>>,
    /// Per-line display→source byte map for lines with markers hidden (W6);
    /// `None` when the displayed text equals the source. Shared with the
    /// line-run cache (a hit re-uses the same allocation across frames).
    maps: Vec<Option<std::rc::Rc<Vec<usize>>>>,
    marks: Vec<Option<LineMark>>,
    /// Per-line inline `$…$` formulas painted over spacers (empty when none).
    inline_images: Vec<Vec<InlineImage>>,
    /// Per-line wrap-row count. Geometry (line tops, total height) reads THIS,
    /// not `wrap_boundaries()` — a windowed-out line's `WrappedLine` is an
    /// empty placeholder, but its cached count keeps the layout exact. For an
    /// RTL line it is OUR row count, which is not gpui's (#66).
    wrap_rows: Vec<usize>,
    /// Per-line bidi layout: whether the line READS right-to-left, plus the
    /// rows we broke in logical order. `None` for lines with no RTL at all,
    /// which keep gpui's own wrapping. Drives that line's paint, caret,
    /// selection and hit-testing.
    rtl_rows: Vec<Option<(bool, Vec<gpui_bidi::Row>)>>,
    /// The empty line every placeholder shares: one document has a single
    /// base size and wrap width, so it is shaped once per pass rather than
    /// once per offscreen line.
    blank: Option<ShapedLine>,
}

impl ShapedDoc {
    /// Push one line that renders as something other than shaped text — a
    /// widget/collapsed/windowed-out line: an empty placeholder `WrappedLine`,
    /// the given height/widget/mark, and `rows` wrap rows.
    #[allow(clippy::too_many_arguments)]
    fn push_placeholder(
        &mut self,
        window: &mut Window,
        base_font_size: Pixels,
        wrap_width: Option<Pixels>,
        h: Pixels,
        widget: Option<Block>,
        mark: Option<LineMark>,
        rows: usize,
    ) {
        let wl = self
            .blank
            .get_or_insert_with(|| {
                std::rc::Rc::new(
                    shape_runs(
                        window,
                        &SharedString::default(),
                        base_font_size,
                        &[],
                        wrap_width,
                    )
                    .into_iter()
                    .next()
                    .expect("a line always shapes to one wrapped line"),
                )
            })
            .clone();
        self.wrapped.push(wl);
        self.heights.push(h);
        self.widgets.push(widget);
        self.backgrounds.push(None);
        self.tables.push(None);
        self.maps.push(None);
        self.marks.push(mark);
        self.inline_images.push(Vec::new());
        self.wrap_rows.push(rows);
        self.rtl_rows.push(None);
    }
}

/// Rewrite an image source `line` to carry an explicit `{width=N}` after the
/// `![alt](src)` (replacing any existing `{width=...}`), preserving a leading
/// list marker and any trailing whitespace. Used to persist a corner-grip resize
/// back into the document. Returns `line` unchanged if it isn't an image row.
fn set_image_width(line: &str, width: u32) -> String {
    let Some((_, _, marker_len)) = markdown_syntax::image_row(line) else {
        return line.to_string();
    };
    // Split off any trailing whitespace so the attr lands right after `)` (or the
    // existing `{width=…}`), with the original trailing run re-appended.
    let trimmed_end = line.trim_end_matches([' ', '\t']);
    let trailing_ws = &line[trimmed_end.len()..];
    // The image body always ends at the first `)` after the list marker; an
    // existing `{width=…}` (only valid right after it) is dropped.
    let close = marker_len + line[marker_len..].find(')').map_or(0, |i| i + 1);
    let body = trimmed_end[..close.min(trimmed_end.len())].trim_end();
    format!("{body}{{width={width}}}{trailing_ws}")
}

/// Invert a display→source offset map: the display column for `source_col`. The
/// map is ascending, so a source column that is hidden (a collapsed marker)
/// snaps to the next visible display column. `None` map → identity (a row shown
/// as full source). The prepaint cursor/selection pass this frame's fresh map
/// (the committed `EditorState::offset_maps` lags a frame); event handlers go
/// through [`EditorState::display_col`], which uses the committed map.
fn display_col_in(map: Option<&std::rc::Rc<Vec<usize>>>, source_col: usize) -> usize {
    match map {
        // The first display byte whose source ≥ `source_col` (a leftmost lower-bound). Unlike
        // `binary_search`, this is deterministic when several display bytes share one source
        // offset — an inline `$…$` spacer maps its whole width to the span start, so the caret
        // just before the formula must land at the spacer's LEFT edge, not somewhere inside it.
        Some(m) => m.partition_point(|&s| s < source_col),
        None => source_col,
    }
}

/// The painted position of display column `dcol` on a shaped line: gpui's own
/// lookup, with the x taken from the row's bidi map when it has one (#66).
///
/// gpui's `x_for_index` returns the first glyph whose `index >= dcol`, and the
/// first glyph of an RTL line carries the HIGHEST index — so every offset in
/// the line collapses onto x = 0. The y (which wrap row) stays gpui's; a row
/// only gets a map while it is ONE visual row, so it is always 0 there.
/// Excludes the row's insets — callers add [`EditorState::row_origin_x`].
fn line_pos(
    line: &WrappedLine,
    rtl: Option<&RtlRow>,
    dcol: usize,
    lh: Pixels,
) -> Option<Point<Pixels>> {
    match rtl {
        // Our own rows: which one holds the offset decides the y, and the
        // row's map the x. gpui's layout is not consulted at all — its rows
        // are not ours.
        Some(r) => {
            let (row, local) = r.row_of(dcol);
            let x = px(r.rows.get(row)?.map.x_for_index(local)) + r.shift_delta(row);
            Some(point(x, lh * row))
        }
        None => line.position_for_index(dcol, lh),
    }
}

/// Where an Up/Down move goes ([`EditorState::vertical_step`]).
struct VerticalStep {
    offset: usize,
    upstream: bool,
    /// The in-cell x kept through a table cell's rows.
    goal: Option<Pixels>,
    /// Whether the editor's goal column carries on (a visual-row move).
    keeps_goal: bool,
}

/// [`line_pos`] for a caret with affinity: gpui draws a soft-wrap boundary
/// at the end of the row above, which is upstream; downstream (the default)
/// it draws at the start of the row below. Bidi rows keep their own rows.
fn caret_line_pos(
    line: &WrappedLine,
    rtl: Option<&RtlRow>,
    dcol: usize,
    lh: Pixels,
    upstream: bool,
) -> Option<Point<Pixels>> {
    if rtl.is_none()
        && !upstream
        && let Some(k) = wrap_indices(line).position(|b| b == dcol)
    {
        return Some(point(px(0.), lh * (k + 1) as f32));
    }
    line_pos(line, rtl, dcol, lh)
}

/// The display columns where a wrapped line's rows after the first start.
fn wrap_indices(line: &WrappedLine) -> impl Iterator<Item = usize> + '_ {
    line.wrap_boundaries
        .iter()
        .map(|w| line.unwrapped_layout.runs[w.run_ix].glyphs[w.glyph_ix].index)
}

/// Whether display column `dcol`, hit at row-local `y`, is the end of the
/// row `y` is on: the wrap boundary below it, which only upstream affinity
/// keeps on that row.
fn ends_wrapped_row(line: &WrappedLine, dcol: usize, y: Pixels, lh: Pixels) -> bool {
    let k = (f32::from(y) / f32::from(lh).max(1.0)).floor().max(0.) as usize;
    wrap_indices(line).nth(k) == Some(dcol)
}

/// The display column a click at row-local `p` names — the inverse of
/// [`line_pos`], through the bidi map when the row has one (gpui's
/// `closest_index_for_x` fails on RTL the same way `x_for_index` does).
fn line_index_at(line: &WrappedLine, rtl: Option<&RtlRow>, p: Point<Pixels>, lh: Pixels) -> usize {
    match rtl {
        Some(r) if !r.rows.is_empty() => {
            let row = ((f32::from(p.y) / f32::from(lh).max(1.0)).floor().max(0.) as usize)
                .min(r.rows.len() - 1);
            let x = p.x - r.shift_delta(row);
            let Some(rr) = r.rows.get(row) else {
                return 0;
            };
            rr.start + rr.map.index_for_x(f32::from(x))
        }
        _ => match line.closest_index_for_position(p, lh) {
            Ok(i) | Err(i) => i,
        },
    }
}

/// A right-to-left row's editor-side geometry, built in prepaint (#66).
///
/// Only rows whose source reads RTL ([`crate::syntax::base_direction`])
/// get one — the flag *is* `Option::is_some`, so an LTR document allocates
/// nothing and keeps taking gpui's own (cheaper) lookups.
pub(crate) struct RtlRow {
    /// The line's visual rows, broken in LOGICAL order (gpui's own wrapping
    /// slices the reordered glyph run and gets them backwards). Each carries
    /// the map that turns an offset inside it into an x and back.
    rows: Vec<gpui_bidi::Row>,
    /// Each row's right-align shift, parallel to `rows`: added to the text
    /// origin so the row right-aligns in the content width (see [`rtl_shift`]).
    /// Per ROW, not per line — a short last row shifts further than a full one.
    /// Kept OUT of `line_insets` on purpose: the list-marker + gutter math
    /// reads that, and must not move with the text.
    shifts: Vec<Pixels>,
    /// Does the line READ right-to-left? A left-to-right line containing a
    /// Persian phrase gets rows and maps too, but stays left-aligned and keeps
    /// left-to-right arrow keys.
    base_rtl: bool,
}

impl RtlRow {
    /// The row holding display column `dcol`, and the column's offset within
    /// it. Rows are in reading order and contiguous, so the last row wins for
    /// an offset at the very end of the line.
    fn row_of(&self, dcol: usize) -> (usize, usize) {
        row_of_spans(self.rows.iter().map(|r| (r.start, r.len)), dcol)
    }

    /// A row's horizontal extent (left, right) relative to the FIRST row's
    /// origin — the coordinate space callers already work in, since they add
    /// `row_origin_x`. Used to band a selection across a row: an RTL row does
    /// not span the content width, so "the whole row" is this, not 0..width.
    pub(crate) fn row_extent(&self, row: usize) -> (Pixels, Pixels) {
        let d = self.shift_delta(row);
        (d, d + self.rows.get(row).map_or(px(0.), |r| r.width))
    }

    /// How many visual rows this line broke into.
    pub(crate) fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// A row's shift relative to the FIRST row's. Callers already add
    /// `row_origin_x`, which carries row 0's shift, so this is the remainder —
    /// that keeps every existing caller correct without threading a wrap-row
    /// index through all of them.
    fn shift_delta(&self, row: usize) -> Pixels {
        self.shifts.get(row).copied().unwrap_or(px(0.))
            - self.shifts.first().copied().unwrap_or(px(0.))
    }
}

/// Which row holds display column `dcol`, and its offset within that row.
///
/// Split out from [`RtlRow::row_of`] so it can be tested without a window: a
/// `Row` carries a shaped line, which needs one. Rows are contiguous and in
/// reading order, so an offset at the very end of the line lands on the last
/// row rather than falling off the end — that is where the caret sits after
/// typing at the end of a paragraph.
fn row_of_spans(rows: impl Iterator<Item = (usize, usize)>, dcol: usize) -> (usize, usize) {
    let mut last = (0, dcol);
    let mut seen = false;
    for (i, (start, len)) in rows.enumerate() {
        seen = true;
        last = (i, dcol.saturating_sub(start));
        if dcol < start + len {
            return (i, dcol - start);
        }
    }
    if seen { last } else { (0, dcol) }
}

/// The x a right-to-left row's text starts at within `content_width`, so its
/// *trailing* edge sits `inset` in from the right — the mirror of the leading
/// `inset` an LTR row gets. Zero once the text no longer fits (it wraps, and
/// every wrap row starts at the left edge).
///
/// Callers add this to the origin they already inset, hence the doubled
/// `inset`: `origin + inset + shift` lands the text `inset` from the right.
fn rtl_shift(content_width: Pixels, inset: Pixels, line_width: Pixels) -> Pixels {
    (content_width - inset * 2. - line_width).max(px(0.))
}

/// Mirror a gutter marker's x (a bullet, a number, a checkbox) to the right
/// edge for an RTL row, so it sits on the side the text now starts at —
/// matching the reader's `flex_row_reverse` list items. Nesting is preserved:
/// a deeper level's larger `marker_x` indents further from the right.
fn rtl_marker_x(content_width: Pixels, marker_x: Pixels, marker_width: Pixels) -> Pixels {
    (content_width - marker_x - marker_width).max(px(0.))
}

/// A task row's checkbox x within the content width, mirrored on an RTL row.
/// One function so the prepaint hitbox (the hand cursor) and the paint (the
/// box, and the rects a click hit-tests) can never land on different sides.
fn checkbox_x(bullet_x: Pixels, size: Pixels, content_width: Pixels, rtl: bool) -> Pixels {
    if rtl {
        rtl_marker_x(content_width, bullet_x, size)
    } else {
        bullet_x
    }
}

/// Paint a flat, line-art document glyph (a page with a folded top-right corner +
/// two text lines) in `color`, the chip's file icon. Drawn with strokes — not a
/// font emoji — so it reads flat and on-theme at the text's size. Public so a
/// host's read-only view can draw the identical icon on its own file chips
/// (cross-view parity).
pub(crate) fn paint_doc_icon(
    x: Pixels,
    y: Pixels,
    w: Pixels,
    h: Pixels,
    color: Hsla,
    window: &mut Window,
) {
    let f = w * 0.33; // folded-corner size
    // Page silhouette, with the top-right corner cut away for the fold.
    let mut outline = PathBuilder::stroke(px(1.3));
    outline.move_to(point(x, y));
    outline.line_to(point(x + w - f, y));
    outline.line_to(point(x + w, y + f));
    outline.line_to(point(x + w, y + h));
    outline.line_to(point(x, y + h));
    outline.line_to(point(x, y));
    if let Ok(p) = outline.build() {
        window.paint_path(p, color);
    }
    // The folded corner (dog-ear).
    let mut fold = PathBuilder::stroke(px(1.3));
    fold.move_to(point(x + w - f, y));
    fold.line_to(point(x + w - f, y + f));
    fold.line_to(point(x + w, y + f));
    if let Ok(p) = fold.build() {
        window.paint_path(p, color);
    }
    // Two short text lines below the fold.
    for fy in [0.6_f32, 0.78] {
        let mut ln = PathBuilder::stroke(px(1.));
        ln.move_to(point(x + w * 0.26, y + h * fy));
        ln.line_to(point(x + w * 0.74, y + h * fy));
        if let Ok(p) = ln.build() {
            window.paint_path(p, color);
        }
    }
}

/// How many wrap rows table row `cells` need at `col_widths` — 1 for content
/// that fits; more once a drag-narrowed column forces its text to wrap.
/// Spaces inserted per Tab / list-nesting level.
const TAB_INDENT: usize = 4;

/// The gutter grip's left edge for an editor whose content starts at
/// `bounds_left` — THE grip x formula, shared by prepaint (fresh geometry)
/// and the event-time hover mirror so the two can't drift.
fn grip_left(bounds_left: Pixels) -> Pixels {
    bounds_left - px(22.)
}

/// One markdown line's built display + runs, cached across frames (see
/// `EditorState::line_run_cache`). `src` and `line_base` verify a hash hit —
/// the rest of the inputs are folded into the key's hash.
struct CachedLineRuns {
    src: String,
    line_base: Hsla,
    /// Shared payloads: a cache HIT is three refcount bumps, not three deep
    /// clones (this runs per visible line per frame).
    disp: SharedString,
    runs: std::rc::Rc<Vec<TextRun>>,
    map: std::rc::Rc<Vec<usize>>,
}

/// The validity marker + per-row content keys of the run-key memo.
type RowKeys = (Option<u64>, Vec<Option<u64>>);

/// The measured table column widths cache: one keyed entry for the doc.
type RegionCols = Option<(u64, std::rc::Rc<Vec<Vec<Pixels>>>)>;

/// The editor-owned caches `shape_document` reads and writes (interior-
/// mutable — shaping runs under a read borrow of the editor):
/// - `line_runs`: each markdown line's built display + runs (cross-frame).
/// - `natural_region_cols`: the measured table column widths for the WHOLE document,
///   one keyed entry — rebuilt on content generation, font size/epoch, or a
///   live column drag. Viewport width does not change natural column widths.
/// - `region_cols`: view-fitted allocations, also keyed by viewport width.
/// - `cell_rows`: per table row, how many wrap rows its tallest cell needs.
#[derive(Default)]
struct ShapeCaches {
    line_runs: std::cell::RefCell<std::collections::HashMap<u64, CachedLineRuns>>,
    region_cols: std::cell::RefCell<RegionCols>,
    natural_region_cols: std::cell::RefCell<RegionCols>,
    cell_rows: std::cell::RefCell<std::collections::HashMap<u64, usize>>,
    /// Per-line (row height, wrap rows), keyed by the line-run key ⊕ font
    /// size ⊕ wrap width — the shaping window's exact heights for skipped
    /// offscreen lines.
    line_heights: std::cell::RefCell<std::collections::HashMap<u64, (Pixels, usize)>>,
    /// Per-row CONTENT-part run keys (line bytes + line_base + diags + epoch),
    /// valid for one (scan generation, epoch) pair — so a steady-state frame
    /// hashes three u64s per line instead of every line's bytes. Diagnostics
    /// changes invalidate explicitly (see `set_diagnostics`).
    row_keys: std::cell::RefCell<RowKeys>,
}

/// A hash of the inputs shared by every line's run build (font + palette) —
/// part of the per-line cache key, so a theme or font change misses cleanly.
fn line_run_epoch(font: &Font, st: Option<&SyntaxStyle>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    font.family.hash(&mut h);
    font.weight.0.to_bits().hash(&mut h);
    let hash_hsla = |c: Hsla, h: &mut std::collections::hash_map::DefaultHasher| {
        c.h.to_bits().hash(h);
        c.s.to_bits().hash(h);
        c.l.to_bits().hash(h);
        c.a.to_bits().hash(h);
    };
    if let Some(st) = st {
        for c in [
            st.marker, st.code, st.code_bg, st.link, st.quote, st.mark_bg,
        ] {
            hash_hsla(c, &mut h);
        }
        st.mono.family.hash(&mut h);
    }
    h.finish()
}

/// Content-derived structural scans — tables, ordered-list numbering,
/// mermaid/math regions, property runs, foldable callouts, and per-line
/// fence parity — cached per [`EditorState::content_gen`]. `shape_document`
/// recomputed all of these on every call (twice a frame before the shape
/// memo); now they rebuild only when the content actually changes, and the
/// caret-driven table ops + auto-replace reuse the same scan.
pub(crate) struct ScanData {
    /// The `content_gen` this scan was built for — cache keys use this
    /// instead of rehashing content.
    generation: u64,
    ordered: Vec<(u32, usize)>,
    tables: Vec<markdown_syntax::TableRegion>,
    mermaid: Vec<(Range<usize>, String)>,
    math: Vec<markdown_syntax::MathRegion>,
    alert_folds: Vec<(Range<usize>, bool)>,
    /// Whether each line STARTS inside a fenced code block (odd count of ```
    /// fences above it).
    fence_odd: Vec<bool>,
}

/// One frame's shaping, memoized between the measure pass and prepaint —
/// both shape the IDENTICAL inputs, so the measure's result is handed to
/// prepaint instead of shaping the whole document twice per frame. Consumed
/// (taken) by prepaint, so it can never go stale across frames; a key
/// mismatch (e.g. the resolved width differs from the available width)
/// falls back to shaping.
struct ShapeMemo {
    wrap_width: Option<Pixels>,
    caret_row: Option<usize>,
    selection: (usize, usize),
    font_size: Pixels,
    shaped: ShapedDoc,
}

#[cfg(test)]
mod caret_tests;

#[cfg(test)]
mod tests {
    #[gpui::test]
    fn formatting_leaves_edge_whitespace_outside_the_markers(cx: &mut gpui::TestAppContext) {
        use super::*;
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update(cx, |editor, cx| {
            editor.set_text("alpha delta gamma", cx);
            // `delta ` with its trailing space, as Alt+Shift+← selects it (#77).
            editor.set_selection(6..12, cx);
        });
        cx.update(|window, cx| editor.update(cx, |e, cx| e.bold(&Bold, window, cx)));
        editor.read_with(cx, |e, _| {
            assert_eq!(e.text(), "alpha **delta** gamma");
            assert_eq!(e.selected_range, 8..13);
        });
        // A second press unwraps it again.
        cx.update(|window, cx| editor.update(cx, |e, cx| e.bold(&Bold, window, cx)));
        editor.read_with(cx, |e, _| assert_eq!(e.text(), "alpha delta gamma"));
        // Only whitespace selected: nothing to format.
        editor.update(cx, |editor, cx| editor.set_selection(5..6, cx));
        cx.update(|window, cx| editor.update(cx, |e, cx| e.italic(&Italic, window, cx)));
        editor.read_with(cx, |e, _| assert_eq!(e.text(), "alpha delta gamma"));
        // Existing padded markers unwrap as selected, not wrap again.
        editor.update(cx, |editor, cx| {
            editor.set_text("<u> delta </u>", cx);
            editor.set_selection(3..10, cx);
        });
        cx.update(|window, cx| editor.update(cx, |e, cx| e.underline(&Underline, window, cx)));
        editor.read_with(cx, |e, _| assert_eq!(e.text(), " delta "));
        // A trailing space after bold markers still unwraps them.
        editor.update(cx, |editor, cx| {
            editor.set_text("**delta** x", cx);
            editor.set_selection(0..10, cx);
        });
        cx.update(|window, cx| editor.update(cx, |e, cx| e.bold(&Bold, window, cx)));
        editor.read_with(cx, |e, _| assert_eq!(e.text(), "delta x"));
    }

    #[gpui::test]
    fn table_width_cache_tracks_viewport_content_and_style(cx: &mut gpui::TestAppContext) {
        use super::*;
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update(cx, |editor, cx| {
            editor.set_text("| heading | other |\n| --- | --- |\n| words | text |", cx);
            editor.set_markdown_style(markdown_syntax::search_style(), cx);
        });
        fn columns(
            editor: &gpui::Entity<EditorState>,
            cx: &mut gpui::VisualTestContext,
        ) -> std::rc::Rc<Vec<Vec<Pixels>>> {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
                editor
                    .read(cx)
                    .shape_caches
                    .region_cols
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .1
                    .clone()
            })
        }
        let first = columns(&editor, cx);
        let natural = editor.read_with(cx, |e, _| {
            e.shape_caches
                .natural_region_cols
                .borrow()
                .as_ref()
                .unwrap()
                .1
                .clone()
        });
        assert!(std::rc::Rc::ptr_eq(&first, &columns(&editor, cx)));
        cx.simulate_resize(size(px(300.), px(400.)));
        editor.update(cx, |editor, cx| {
            editor.set_cursor(editor.text().len(), cx);
            editor.set_diagnostics(Vec::new(), cx);
        });
        let after_resize = columns(&editor, cx);
        assert!(!std::rc::Rc::ptr_eq(&first, &after_resize));
        editor.read_with(cx, |e, _| {
            assert!(std::rc::Rc::ptr_eq(
                &natural,
                &e.shape_caches
                    .natural_region_cols
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .1
            ))
        });
        editor.update(cx, |editor, cx| {
            editor.set_text(
                "| a much longer heading | other |\n| --- | --- |\n| words | text |",
                cx,
            );
        });
        let changed = columns(&editor, cx);
        assert!(!std::rc::Rc::ptr_eq(&first, &changed));
        assert!(changed[0][0] > first[0][0]);
        editor.update(cx, |editor, cx| {
            let mut style = markdown_syntax::search_style();
            style.code = gpui::red();
            editor.set_markdown_style(style, cx);
        });
        assert!(!std::rc::Rc::ptr_eq(&changed, &columns(&editor, cx)));
    }

    #[gpui::test]
    fn table_viewport_fitting_preserves_edits_selection_and_explicit_widths(
        cx: &mut gpui::TestAppContext,
    ) {
        use super::*;
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update(cx, |e, cx| {
            e.set_text("| First lengthy heading that must wrap | Second lengthy heading that must wrap |\n| --- | --- |\n| Many words to measure in this column | More words to measure here |", cx);
            e.set_markdown_style(markdown_syntax::search_style(), cx);
            e.set_cursor(5, cx);
        });
        let source = editor.read_with(cx, |e, _| e.text().to_owned());
        let revision = editor.read_with(cx, |e, _| e.revision());
        cx.simulate_resize(size(px(350.), px(400.)));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let fitted = editor.read_with(cx, |e, _| {
            e.shape_caches.region_cols.borrow().as_ref().unwrap().1[0].clone()
        });
        assert!(fitted.iter().copied().sum::<Pixels>() <= px(350.));
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        editor.update(cx, |e, cx| {
            assert_eq!(e.text(), source);
            assert_eq!(e.revision(), revision);
            assert_eq!(e.cursor(), 5);
            {
                let cache = e.shape_caches.region_cols.borrow();
                let widths = &cache.as_ref().unwrap().1[0];
                assert!(
                    widths.iter().copied().sum::<Pixels>() > fitted.iter().copied().sum::<Pixels>()
                );
            }
            e.set_text(
                "<!-- table:grid cols=500,500 -->\n| A | B |\n| --- | --- |\n| First | Second |",
                cx,
            );
        });
        cx.simulate_resize(size(px(350.), px(400.)));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        editor.read_with(cx, |e, _| {
            assert_eq!(
                e.shape_caches.region_cols.borrow().as_ref().unwrap().1[0],
                vec![px(500.), px(500.)]
            );
            assert!(
                !e.table_thumbs.is_empty(),
                "explicit overflow has a draggable thumb"
            );
        });
    }

    #[gpui::test]
    fn br_in_a_table_cell_breaks_the_line(cx: &mut gpui::TestAppContext) {
        use super::*;
        assert_eq!(
            tables::cell_breaks("a<br>b<BR/>c<br />d<br x>"),
            vec![1..5, 6..11, 12..18]
        );
        // Literal tags: in a code span, after a backslash.
        assert_eq!(
            tables::cell_breaks("`<br>` and ``a`<br>`` \\<br> x<br>"),
            vec![29..33]
        );
        assert_eq!(tables::cell_breaks("` unmatched <br>"), vec![12..16]);
        let (editor, cx) = cx.add_window_view(EditorState::new);
        cx.simulate_resize(size(px(700.), px(700.)));
        let draw = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
        };
        let mut heights = Vec::new();
        for source in [
            "| A | B |\n| --- | --- |\n| one | two |\nend",
            "| A | B |\n| --- | --- |\n| one<br>first | two |\nend",
        ] {
            editor.update(cx, |e, cx| {
                e.set_text(source, cx);
                e.set_markdown_style(markdown_syntax::search_style(), cx);
            });
            draw(cx);
            heights.push(editor.read_with(cx, |e, _| e.line_tops[3] - e.line_tops[2]));
        }
        let line_h = editor.read_with(cx, |e, _| e.font_size * LINE_HEIGHT_RATIO);
        let grown = heights[1] - heights[0];
        assert!(
            (f32::from(grown - line_h)).abs() < 0.5,
            "a <br> row is one line taller: {heights:?}"
        );
        // Formatting spanning a tag holds on both lines: no stray markers.
        let texts = cx.update(|window, _| {
            let font = window.text_style().font();
            tables::shape_cell_lines(
                window,
                "**first<br>second**",
                &font,
                px(14.),
                None,
                gpui::black(),
            )
            .into_iter()
            .map(|line| (line.line.text.to_string(), line.range))
            .collect::<Vec<_>>()
        });
        assert_eq!(
            texts,
            [("first".to_string(), 0..7), ("second".to_string(), 11..19)]
        );
        // A click on the second line lands after the tag, not inside it.
        let source = "| A | B |\n| --- | --- |\n| one<br>first | two |\nend";
        let offset = cx.update(|window, cx| {
            editor.update(cx, |e, _| {
                let bounds = e.last_bounds.unwrap();
                let top = bounds.top() + e.line_tops[2] + px(6.) + line_h * 1.5;
                let x = bounds.left() + px(TABLE_GUTTER) + px(tables::TABLE_CELL_PAD) + px(1.);
                e.table_offset_at(point(x, top), window)
            })
        });
        assert_eq!(offset, Some(source.find("first").unwrap()));
        editor.read_with(cx, |e, _| assert_eq!(e.text(), source));
    }

    #[gpui::test]
    fn resizing_wrapped_table_preserves_other_columns_and_wrap_through_release(
        cx: &mut gpui::TestAppContext,
    ) {
        use super::*;
        let (editor, cx) = cx.add_window_view(EditorState::new);
        cx.simulate_resize(size(px(700.), px(700.)));
        let source = format!(
            "| Clause | Contractor wording | Customer wording |\n| --- | --- | --- |\n| 4.5.2 | {} | {} |",
            "Payment must be made after the services have been provided. ".repeat(8),
            "Оплата производится после оказания соответствующих услуг. ".repeat(8),
        );
        let draw = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
        };
        for col in [2, 1] {
            editor.update(cx, |e, cx| {
                e.set_text(&source, cx);
                e.set_markdown_style(markdown_syntax::search_style(), cx);
                e.set_cursor(source.find("Оплата").unwrap(), cx);
            });
            draw(cx);
            let hover = editor.read_with(cx, |e, _| e.table_hover_zones[0].0.center());
            cx.simulate_mouse_move(hover, None, Default::default());
            draw(cx);
            let (widths, band) = editor.read_with(cx, |e, _| {
                let widths = e.table_rows[0].as_ref().unwrap().col_widths.clone();
                let band = e
                    .table_col_resize_rects
                    .iter()
                    .find(|(_, header, c, _)| *header == 0 && *c == col)
                    .unwrap()
                    .0;
                assert!(e.table_thumbs.is_empty());
                (widths, band)
            });
            let from = point(band.left() + px(1.), band.top() + px(8.));
            let to = from - point(px(12.), px(0.));
            cx.simulate_mouse_down(from, MouseButton::Left, Default::default());
            draw(cx);
            editor.read_with(cx, |e, _| {
                assert!(e.table_col_resize.is_some());
                assert_eq!(
                    e.table_rows[0].as_ref().unwrap().col_widths,
                    widths,
                    "pressing the border must not expand other columns"
                );
            });
            cx.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
            draw(cx);
            let live = editor.read_with(cx, |e, _| {
                let live = e.table_rows[0].as_ref().unwrap().col_widths.clone();
                for c in 0..widths.len() {
                    let expected = if c == col {
                        widths[c] - px(12.)
                    } else {
                        widths[c]
                    };
                    assert!((f32::from(live[c] - expected)).abs() < 0.01);
                }
                assert_eq!(e.text(), source);
                assert!(e.table_thumbs.is_empty());
                live
            });
            cx.simulate_mouse_up(to, MouseButton::Left, Default::default());
            draw(cx);
            let saved = editor.read_with(cx, |e, _| {
                assert!(e.table_col_resize.is_none());
                assert!(e.text().ends_with(&source));
                let widths = &e.table_rows[1].as_ref().unwrap().col_widths;
                for (a, b) in widths.iter().zip(&live) {
                    assert!(f32::from(*a - *b).abs() <= 0.5);
                }
                assert!(e.table_thumbs.is_empty());
                e.text().to_owned()
            });
            editor.update_in(cx, |e, window, cx| e.undo(&Undo, window, cx));
            draw(cx);
            editor.read_with(cx, |e, _| assert_eq!(e.text(), source));
            editor.update(cx, |e, cx| e.set_text(&saved, cx));
            draw(cx);
            editor.read_with(cx, |e, _| {
                for (a, b) in e.table_rows[1]
                    .as_ref()
                    .unwrap()
                    .col_widths
                    .iter()
                    .zip(&live)
                {
                    assert!(f32::from(*a - *b).abs() <= 0.5);
                }
            });
        }
    }

    #[gpui::test]
    fn tall_overflow_table_thumb_stays_visible_and_drags_without_edits(
        cx: &mut gpui::TestAppContext,
    ) {
        use super::*;
        let (editor, cx) = cx.add_window_view(EditorState::new);
        let source = format!(
            "<!-- table:grid cols=500,500 -->\n| First | Second |\n| --- | --- |\n{}",
            "| Left text | Right text |\n".repeat(30)
        );
        editor.update(cx, |e, cx| {
            e.set_text(&source, cx);
            e.set_markdown_style(markdown_syntax::search_style(), cx);
            e.set_cursor(5, cx);
        });
        let revision = editor.read_with(cx, |e, _| e.revision());
        cx.simulate_resize(size(px(350.), px(400.)));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let thumb = editor.read_with(cx, |e, _| e.table_thumbs[0]);
        assert!(thumb.rect.top() > px(0.) && thumb.rect.bottom() <= px(400.));
        let from = thumb.rect.center();
        let to = from + point(px(60.), px(0.));
        cx.simulate_mouse_down(from, MouseButton::Left, Default::default());
        cx.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
        cx.simulate_mouse_up(to, MouseButton::Left, Default::default());
        editor.read_with(cx, |e, _| {
            assert!(e.table_scroll_x.get(&thumb.header).copied().unwrap_or(0.) > 0.);
            assert_eq!(e.text(), source);
            assert_eq!(e.revision(), revision);
            assert_eq!(e.cursor(), 5);
            assert!(e.table_col_resize.is_none());
        });
    }

    #[gpui::test]
    fn double_click_drag_extends_the_selection_by_whole_words(cx: &mut gpui::TestAppContext) {
        use super::*;
        let (editor, cx) = cx.add_window_view(EditorState::new);
        let source = "alpha beta gamma delta epsilon";
        editor.update(cx, |e, cx| {
            e.set_text(source, cx);
            e.set_markdown_style(markdown_syntax::search_style(), cx);
        });
        cx.simulate_resize(size(px(600.), px(300.)));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        // A point inside the character at `offset`.
        let at = |cx: &mut gpui::VisualTestContext, offset: usize| {
            editor.read_with(cx, |e, _| {
                let b = e.bounds_for_offset(offset).unwrap();
                point(b.left() + px(2.), b.center().y)
            })
        };
        let gamma = source.find("gamma").unwrap();
        let start = at(cx, gamma + 1);
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: start,
            modifiers: Default::default(),
            click_count: 2,
            first_mouse: false,
        });
        editor.read_with(cx, |e, _| {
            assert_eq!(&source[e.selected_range.clone()], "gamma")
        });
        // Forward into the middle of "delta": the whole word joins.
        let delta = at(cx, source.find("delta").unwrap() + 2);
        cx.simulate_mouse_move(delta, Some(MouseButton::Left), Default::default());
        editor.read_with(cx, |e, _| {
            assert_eq!(&source[e.selected_range.clone()], "gamma delta");
            assert!(!e.selection_reversed);
        });
        // Back past the anchor into "beta": it keeps "gamma" and grows left.
        let beta = at(cx, source.find("beta").unwrap() + 2);
        cx.simulate_mouse_move(beta, Some(MouseButton::Left), Default::default());
        cx.simulate_mouse_up(beta, MouseButton::Left, Default::default());
        editor.read_with(cx, |e, _| {
            assert_eq!(&source[e.selected_range.clone()], "beta gamma");
            assert!(e.selection_reversed);
            assert!(e.word_drag.is_none() && !e.is_selecting);
        });
        // A plain drag afterwards is character-wise again.
        let (alpha, third) = (at(cx, 1), at(cx, 3));
        cx.simulate_mouse_down(alpha, MouseButton::Left, Default::default());
        cx.simulate_mouse_move(third, Some(MouseButton::Left), Default::default());
        cx.simulate_mouse_up(third, MouseButton::Left, Default::default());
        editor.read_with(cx, |e, _| assert_eq!(e.selected_range.len(), 2));
    }

    #[gpui::test]
    fn structural_scan_reuses_content_generation_not_caret_or_diagnostics(
        cx: &mut gpui::TestAppContext,
    ) {
        use super::*;
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update(cx, |editor, cx| {
            let source = "<!-- math:center -->\n$$\nx + y\n$$\n\ntext";
            editor.set_text(source, cx);
            let scan = editor.scan_data();
            assert_eq!(scan.math.len(), 1);
            assert_eq!(scan.math[0].marker_line, Some(0));
            editor.set_cursor(source.len(), cx);
            editor.set_diagnostics(Vec::new(), cx);
            assert!(std::rc::Rc::ptr_eq(&scan, &editor.scan_data()));
            editor.set_text("ordinary text", cx);
            let replaced = editor.scan_data();
            assert!(!std::rc::Rc::ptr_eq(&scan, &replaced));
            assert!(replaced.math.is_empty());
            assert!(std::rc::Rc::ptr_eq(&replaced, &editor.scan_data()));
        });
    }

    #[gpui::test]
    fn active_search_navigation_retains_allocations(cx: &mut gpui::TestAppContext) {
        use super::*;
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update(cx, |editor, cx| {
            let matches: Vec<_> = (0..100_000)
                .map(|i| SearchMatch {
                    source: std::iter::once(i..i + 1).collect(),
                    replaceable: true,
                })
                .collect();
            editor.set_search_matches(matches, Some(0), cx);
            let allocation = editor.search.as_ref().unwrap().0.as_ptr();
            let ranges = editor.search.as_ref().unwrap().0[0].source.as_ptr();
            let started = std::time::Instant::now();
            for i in 0..10_000 {
                editor.set_active_search_match(Some(i), cx);
            }
            eprintln!(
                "10,000 active updates / 100,000 matches: {:?}",
                started.elapsed()
            );
            assert_eq!(editor.search.as_ref().unwrap().0.as_ptr(), allocation);
            assert_eq!(editor.search.as_ref().unwrap().0[0].source.as_ptr(), ranges);
            assert_eq!(editor.search.as_ref().unwrap().1, Some(9_999));
            editor.set_active_search_match(Some(100_000), cx);
            assert_eq!(editor.search.as_ref().unwrap().1, None);
            editor.set_search_matches(Vec::new(), None, cx);
            editor.set_active_search_match(Some(0), cx);
            assert!(editor.search.is_none());
        });
    }

    #[test]
    fn caret_off_marker_line_cases() {
        use super::caret_off_marker_line;
        // Table marker at the top: the caret steps to the header line.
        let doc = "<!-- table:grid cols=40,40 -->\n| a | b |\n| --- | --- |\n| 1 | 2 |";
        assert_eq!(caret_off_marker_line(doc, 0), 31);
        // Math align marker: the caret lands after the block.
        let doc = "<!-- math:center -->\n$$\nx^2\n$$\nafter";
        assert_eq!(caret_off_marker_line(doc, 0), 31);
        assert_eq!(&doc[31..], "after");
        // Plain lines pass through untouched.
        assert_eq!(caret_off_marker_line("hello\nworld", 0), 0);
        assert_eq!(caret_off_marker_line("", 0), 0);
    }

    #[test]
    fn strip_block_prefix_cases() {
        use super::strip_block_prefix;
        assert_eq!(strip_block_prefix("# Title"), "Title");
        assert_eq!(strip_block_prefix("### Deep"), "Deep");
        assert_eq!(strip_block_prefix("- [x] done"), "done");
        assert_eq!(strip_block_prefix("- item"), "item");
        assert_eq!(strip_block_prefix("12. nth"), "nth");
        assert_eq!(strip_block_prefix("> quoted"), "quoted");
        assert_eq!(strip_block_prefix(">bare"), "bare");
        assert_eq!(strip_block_prefix("plain"), "plain");
        // Renderer-grammar forms the old hand-rolled version missed.
        assert_eq!(strip_block_prefix("* [ ] star task"), "star task");
        assert_eq!(strip_block_prefix("+ [x] plus task"), "plus task");
        assert_eq!(strip_block_prefix("3) paren"), "paren");
        assert_eq!(strip_block_prefix("#### H4"), "H4");
        // Not block prefixes: mid-word hash runs, `#tag`, a lone dash.
        assert_eq!(strip_block_prefix("#tag"), "#tag");
        assert_eq!(strip_block_prefix("-dash"), "-dash");
    }

    use super::{display_col_in, set_image_width};

    #[test]
    fn display_col_leftmost_for_inline_math_spacer() {
        // An inline `$…$` spacer maps its whole width to the span's start offset (here source 2,
        // repeated across display 2..5). The caret at source 2 must land at the spacer's LEFT
        // edge (display 2), not an arbitrary spot inside it; source 5 (just past the formula)
        // lands at display 5.
        let map = std::rc::Rc::new(vec![0, 1, 2, 2, 2, 5, 6, 7]);
        assert_eq!(display_col_in(Some(&map), 2), 2);
        assert_eq!(display_col_in(Some(&map), 5), 5);
        // A strictly-increasing map (hidden markers) is unaffected.
        let plain = std::rc::Rc::new(vec![0, 1, 2, 3]);
        assert_eq!(display_col_in(Some(&plain), 2), 2);
        assert_eq!(display_col_in(None, 4), 4);
    }

    #[test]
    fn image_width_splice() {
        // No existing attr: append `{width=N}` right after `)`.
        assert_eq!(
            set_image_width("![a](b.png)", 200),
            "![a](b.png){width=200}"
        );
        // Existing `{width=N}` is replaced (not duplicated).
        assert_eq!(
            set_image_width("![a](b.png){width=320}", 200),
            "![a](b.png){width=200}"
        );
        // The `px` unit form is replaced too.
        assert_eq!(
            set_image_width("![a](b.png){width=320px}", 200),
            "![a](b.png){width=200}"
        );
        // List-item image: the leading marker is preserved, attr lands after `)`.
        assert_eq!(
            set_image_width("- ![](x){width=10}", 50),
            "- ![](x){width=50}"
        );
        // Trailing whitespace is preserved (attr lands before it).
        assert_eq!(
            set_image_width("![a](b.png)  ", 80),
            "![a](b.png){width=80}  "
        );
        // Not an image row: returned unchanged.
        assert_eq!(set_image_width("just text", 100), "just text");
    }

    // --- RTL row geometry (#66) ---------------------------------------------

    use super::{checkbox_x, px, row_of_spans, rtl_marker_x, rtl_shift};

    #[test]
    fn a_caret_offset_lands_on_the_row_that_holds_it() {
        // Three rows: "0..5", "5..11", "11..14" — contiguous, reading order.
        let rows = || [(0usize, 5usize), (5, 6), (11, 3)].into_iter();
        assert_eq!(row_of_spans(rows(), 0), (0, 0));
        assert_eq!(row_of_spans(rows(), 4), (0, 4));
        // A boundary belongs to the row that STARTS there, not the one ending.
        assert_eq!(row_of_spans(rows(), 5), (1, 0));
        assert_eq!(row_of_spans(rows(), 12), (2, 1));
        // The caret sits one past the last character after typing at the end
        // of a paragraph: it must stay on the last row, not fall off.
        assert_eq!(row_of_spans(rows(), 14), (2, 3));
        assert_eq!(row_of_spans(rows(), 99), (2, 88));
        // No rows at all (an empty line) is row 0.
        assert_eq!(row_of_spans([].into_iter(), 0), (0, 0));
    }

    #[test]
    fn rtl_shift_mirrors_the_row_inset() {
        // Plain paragraph (no inset): the text's right edge meets the content
        // edge, so the shift is all the slack.
        assert_eq!(rtl_shift(px(500.), px(0.), px(200.)), px(300.));
        // Inset row (a list item / blockquote body): callers add the shift to
        // an origin they already inset, so the doubled inset leaves the SAME
        // gap on the right that an LTR row gets on the left.
        assert_eq!(rtl_shift(px(500.), px(24.), px(200.)), px(252.));
        assert_eq!(px(24.) + rtl_shift(px(500.), px(24.), px(200.)), px(276.));
        // …i.e. text spans 276..476, exactly 24 in from the right edge.
        // Text that fills or overflows the row wraps, and every wrap row starts
        // at the left edge — no shift, never a negative one.
        assert_eq!(rtl_shift(px(500.), px(0.), px(500.)), px(0.));
        assert_eq!(rtl_shift(px(500.), px(0.), px(900.)), px(0.));
        assert_eq!(rtl_shift(px(100.), px(60.), px(50.)), px(0.));
    }

    #[test]
    fn rtl_markers_mirror_to_the_right_edge() {
        // A bullet 8px wide at x=10 lands 10 in from the right edge instead.
        assert_eq!(rtl_marker_x(px(500.), px(10.), px(8.)), px(482.));
        // Nesting is preserved: a deeper level (larger x) indents further FROM
        // THE RIGHT, so the levels keep their order.
        let l1 = rtl_marker_x(px(500.), px(10.), px(8.));
        let l2 = rtl_marker_x(px(500.), px(34.), px(8.));
        assert!(l2 < l1, "level 2 must sit further in from the right");
        assert_eq!(l1 - l2, px(24.), "the indent step survives the mirror");
        // Never off the left edge, however wide the marker.
        assert_eq!(rtl_marker_x(px(20.), px(10.), px(40.)), px(0.));
        // The checkbox shares that math, and an LTR row is untouched.
        assert_eq!(checkbox_x(px(10.), px(12.), px(500.), true), px(478.));
        assert_eq!(checkbox_x(px(10.), px(12.), px(500.), false), px(10.));
    }

    // --- RTL table placement (#66) ------------------------------------------

    use super::tables::{TABLE_GUTTER, table_left_x, table_visible_band};

    /// The note column used throughout: origin 100, width 500 → the LTR band
    /// is 122..600 and the RTL band 100..578, both `TABLE_GUTTER` wide.
    const O: f32 = 100.;
    const W: f32 = 500.;

    #[test]
    fn an_rtl_table_hugs_the_right_edge_with_the_gutter_mirrored() {
        let g = TABLE_GUTTER;
        // A table narrower than the column: LTR starts a gutter in from the
        // left, RTL *ends* a gutter in from the right.
        let ltr = table_left_x(px(O), px(W), px(300.), px(0.), false);
        let rtl = table_left_x(px(O), px(W), px(300.), px(0.), true);
        assert_eq!(ltr, px(O + g));
        assert_eq!(rtl + px(300.), px(O + W - g), "right edge, one gutter in");
        // The two are mirror images about the column's centre.
        assert_eq!(
            f32::from(ltr - px(O)),
            f32::from(px(O + W) - (rtl + px(300.)))
        );
        // Column widths don't move an LTR table but do move an RTL one — it is
        // anchored at its trailing edge.
        assert_eq!(
            table_left_x(px(O), px(W), px(120.), px(0.), false),
            px(O + g)
        );
        assert_eq!(
            table_left_x(px(O), px(W), px(120.), px(0.), true),
            px(O + W - g - 120.)
        );
    }

    #[test]
    fn a_wide_table_scrolls_to_its_own_far_edge_either_way() {
        let g = TABLE_GUTTER;
        let total = px(900.);
        let avail = px(W - g); // what `table_sx` clamps against
        let max = total - avail;
        // Unscrolled, each direction shows its own leading edge at the gutter.
        assert_eq!(table_left_x(px(O), px(W), total, px(0.), false), px(O + g));
        assert_eq!(
            table_left_x(px(O), px(W), total, px(0.), true) + total,
            px(O + W - g)
        );
        // Fully scrolled, each shows its trailing edge at the band's far side:
        // LTR's right edge reaches the column's right, RTL's left edge the left.
        assert_eq!(
            table_left_x(px(O), px(W), total, max, false) + total,
            px(O + W)
        );
        assert_eq!(table_left_x(px(O), px(W), total, max, true), px(O));
        // Scroll moves the content in OPPOSITE directions — why the wheel and
        // the thumb's `factor` invert their sign on an RTL table.
        let step = px(50.);
        assert!(table_left_x(px(O), px(W), total, step, false) < px(O + g));
        assert!(
            table_left_x(px(O), px(W), total, step, true)
                > table_left_x(px(O), px(W), total, px(0.), true)
        );
    }

    #[test]
    fn rtl_mirrors_the_column_order() {
        use super::tables::{cell_span_width, col_offset};
        // Three columns, 10/20/30 wide. Left to right they start at 0/10/30;
        // mirrored, column 0 is the RIGHTMOST, so it starts at 50.
        let w = [px(10.), px(20.), px(30.)];
        assert_eq!(col_offset(&w, 3, 0, false), px(0.));
        assert_eq!(col_offset(&w, 3, 1, false), px(10.));
        assert_eq!(col_offset(&w, 3, 2, false), px(30.));
        assert_eq!(col_offset(&w, 3, 0, true), px(50.));
        assert_eq!(col_offset(&w, 3, 1, true), px(30.));
        assert_eq!(col_offset(&w, 3, 2, true), px(0.));
        // Either way the columns tile the table with no gap or overlap — paint,
        // caret and hit-testing all read this, so a gap is a mis-click.
        for rtl in [false, true] {
            let mut spans: Vec<(f32, f32)> = (0..3)
                .map(|c| {
                    let x = f32::from(col_offset(&w, 3, c, rtl));
                    (x, x + f32::from(cell_span_width(&w, 3, c)))
                })
                .collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            assert_eq!(spans[0].0, 0.0);
            assert_eq!(spans[2].1, 60.0);
            assert!(spans.windows(2).all(|s| s[0].1 == s[1].0), "{spans:?}");
        }
    }

    #[test]
    fn the_visible_band_is_the_column_less_its_gutter() {
        let g = TABLE_GUTTER;
        assert_eq!(
            table_visible_band(px(O), px(W), false),
            (px(O + g), px(O + W))
        );
        assert_eq!(
            table_visible_band(px(O), px(W), true),
            (px(O), px(O + W - g))
        );
        // Both bands are the `avail` width the scroll clamp assumes.
        for rtl in [false, true] {
            let (l, r) = table_visible_band(px(O), px(W), rtl);
            assert_eq!(r - l, px(W - g));
        }
    }
}

#[cfg(test)]
mod annotation_tests {
    use super::*;
    #[gpui::test]
    fn styled_mentions_keep_source_geometry_and_atomic_undo(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        let source = "==Анна== and <span style='color:blue'>Анна</span>";
        let first = source.find("Анна").unwrap();
        let second = source.rfind("Анна").unwrap();
        let ranges = [first..first + "Анна".len(), second..second + "Анна".len()];
        let mut revision = 0;
        editor.update(cx, |editor, cx| {
            editor.set_text(source, cx);
            editor.set_markdown_style(markdown_syntax::search_style(), cx);
            revision = editor.revision();
            let matches = SearchIndex::from_markdown(source).find("Анна", true);
            assert_eq!(matches.len(), 2);
            for (matched, range) in matches.iter().zip(&ranges) {
                assert_eq!(matched.source, vec![range.clone()]);
            }
            editor.set_search(ranges.to_vec(), Some(0), cx);
            editor.set_annotations(
                revision,
                ranges
                    .iter()
                    .enumerate()
                    .map(|(id, range)| SourceAnnotation {
                        id: id as u64,
                        range: range.clone(),
                        color: rgba(0xffaa0022).into(),
                        active_color: rgba(0xffaa0055).into(),
                        border: gpui::transparent_black(),
                        text_color: None,
                        underline: None,
                        dotted: false,
                    })
                    .collect(),
                cx,
            );
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        editor.update_in(cx, |editor, window, cx| {
            assert_eq!(editor.text(), source);
            for id in 0..2 {
                let annotation = editor.annotation_bounds(id).unwrap();
                assert!(annotation.size.width > px(0.0));
                assert!(editor.search_match_bounds(id as usize).is_some());
            }
            assert!(
                editor.replace_ranges(
                    revision,
                    &ranges
                        .iter()
                        .map(|range| (range.clone(), "PERSON_1".into()))
                        .collect::<Vec<_>>(),
                    cx
                )
            );
            assert_eq!(
                editor.text(),
                "==PERSON_1== and <span style='color:blue'>PERSON_1</span>"
            );
            assert!(editor.annotation_bounds(0).is_none());
            assert!(!editor.replace_ranges(revision, &[(ranges[0].clone(), "stale".into())], cx));
            editor.undo(&Undo, window, cx);
            assert_eq!(editor.text(), source);
        });
    }

    #[gpui::test]
    fn grouped_replacements_are_atomic_revision_checked_and_one_undo(
        cx: &mut gpui::TestAppContext,
    ) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Анна and Анна", cx);
            let revision = editor.revision();
            assert!(!editor.replace_ranges(revision, &[(1..8, "bad".into())], cx));
            assert!(!editor.replace_ranges(
                revision,
                &[(0..8, "x".into()), (4..8, "y".into())],
                cx
            ));
            assert_eq!(editor.text(), "Анна and Анна");
            editor.set_cursor(editor.text().len(), cx);
            assert!(editor.replace_ranges(
                revision,
                &[(0..8, "PERSON_1".into()), (13..21, "PERSON_1".into())],
                cx
            ));
            assert_eq!(editor.text(), "PERSON_1 and PERSON_1");
            assert!(!editor.replace_ranges(revision, &[(0..8, "bad".into())], cx));
            editor.undo(&Undo, window, cx);
            assert_eq!(editor.text(), "Анна and Анна");
            editor.redo(&Redo, window, cx);
            assert_eq!(editor.text(), "PERSON_1 and PERSON_1");
        });
    }
    #[gpui::test]
    fn annotations_coexist_with_find_and_discard_stale_geometry(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        let activated = std::rc::Rc::new(std::cell::Cell::new(None));
        let observed = activated.clone();
        let _subscription = cx.update(|_, cx| {
            cx.subscribe(&editor, move |_, event, _| {
                if let EditorEvent::ActivateAnnotation(id) = event {
                    observed.set(Some(*id));
                }
            })
        });
        editor.update(cx, |editor, cx| {
            editor.set_text("Alice and Bob\n\n[link](https://alice.invalid)", cx);
            editor.set_markdown_style(markdown_syntax::search_style(), cx);
            editor.set_search(std::iter::once(10..13).collect(), Some(0), cx);
            editor.set_annotations(
                editor.revision(),
                vec![
                    SourceAnnotation {
                        id: 7,
                        range: 0..5,
                        color: rgba(0xffaa0022).into(),
                        active_color: rgba(0xffaa0055).into(),
                        border: gpui::transparent_black(),
                        text_color: None,
                        underline: None,
                        dotted: false,
                    },
                    SourceAnnotation {
                        id: 8,
                        range: 30..35,
                        color: rgba(0xffaa0022).into(),
                        active_color: rgba(0xffaa0055).into(),
                        border: gpui::transparent_black(),
                        text_color: None,
                        underline: None,
                        dotted: false,
                    },
                ],
                cx,
            );
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let hidden = cx.update(|_, cx| editor.read(cx).annotation_bounds(8).unwrap());
        cx.simulate_click(hidden.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            activated.get(),
            Some(8),
            "hidden source gutter activates review"
        );
        editor.update(cx, |editor, cx| {
            assert!(editor.annotation_bounds(7).is_some());
            assert!(
                editor.annotation_bounds(8).is_some(),
                "hidden source has a containing-element marker"
            );
            assert!(editor.search_match_bounds(0).is_some());
            editor.replace_range(0..5, "Other", cx);
            assert!(editor.annotation_bounds(7).is_none());
        });
    }

    #[gpui::test]
    fn search_matches_paint_nothing_at_the_previous_rows_end(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        let mut source = String::new();
        for k in 0..80 {
            source.push_str(&"w".repeat(1 + k % 9));
            source.push_str(" Ann Lee ");
        }
        // Every word and every two-word name: some start a wrap row, some
        // names wrap between their words.
        let mut matches: Vec<Range<usize>> = Vec::new();
        let mut start = None;
        for (i, c) in source.char_indices().chain([(source.len(), ' ')]) {
            match (c == ' ', start) {
                (false, None) => start = Some(i),
                (true, Some(s)) => {
                    matches.push(s..i);
                    start = None;
                }
                _ => {}
            }
        }
        matches.extend(source.match_indices("Ann Lee").map(|(i, m)| i..i + m.len()));
        editor.update(cx, |editor, cx| {
            editor.set_text(source.clone(), cx);
            editor.set_search(matches.clone(), None, cx);
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        editor.update(cx, |editor, _| {
            let area = editor.last_bounds.unwrap();
            let rows = editor.search_bounds.iter().flatten();
            let tops: std::collections::BTreeSet<_> =
                rows.clone().map(|b| f32::from(b.top()) as i32).collect();
            assert!(tops.len() > 3, "the text wraps");
            for (i, first) in editor.search_bounds.iter().enumerate() {
                let first = first.expect("visible match");
                assert!(
                    first.right() < area.right() - px(1.),
                    "match {:?} starts at its text, not the previous row's end",
                    &source[matches[i].clone()]
                );
            }
        });
    }

    #[gpui::test]
    fn wrapped_annotations_box_each_segment(cx: &mut gpui::TestAppContext) {
        // A one-letter first word keeps its own segment too.
        for mention in ["Ann Lee", "A Lee"] {
            wrapped_mention_boxes(cx, mention);
        }
    }

    fn wrapped_mention_boxes(cx: &mut gpui::TestAppContext, mention: &str) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        // Varying filler moves each mention against the wrap width, so some
        // wrap between their two words.
        let mut source = String::new();
        let mut ranges = Vec::new();
        for k in 0..80 {
            source.push_str(&"w".repeat(1 + k % 9));
            source.push(' ');
            ranges.push(source.len()..source.len() + mention.len());
            source.push_str(mention);
            source.push(' ');
        }
        editor.update(cx, |editor, cx| {
            editor.set_text(source.clone(), cx);
            editor.set_annotations(
                editor.revision(),
                ranges
                    .iter()
                    .enumerate()
                    .map(|(id, range)| SourceAnnotation {
                        id: id as u64,
                        range: range.clone(),
                        color: rgba(0xffaa0022).into(),
                        active_color: rgba(0xffaa0055).into(),
                        border: rgba(0xffaa00ff).into(),
                        text_color: None,
                        underline: None,
                        dotted: false,
                    })
                    .collect(),
                cx,
            );
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        editor.update(cx, |editor, _| {
            let area = editor.last_bounds.unwrap();
            let quads = |id: u64| -> Vec<Bounds<Pixels>> {
                editor
                    .annotation_bounds
                    .iter()
                    .filter(|(key, _)| *key == id)
                    .map(|(_, bounds)| *bounds)
                    .collect()
            };
            let whole = (0..ranges.len() as u64)
                .map(quads)
                .find(|q| q.len() == 1)
                .expect("an unwrapped mention")[0]
                .size
                .width;
            let wrapped: Vec<_> = (0..ranges.len() as u64)
                .map(quads)
                .filter(|q| q.len() == 2)
                .collect();
            assert!(!wrapped.is_empty(), "some {mention:?} wraps");
            for q in wrapped {
                let (first, rest) = (q[0], q[1]);
                assert!(rest.top() > first.top());
                // Chips pad 2 px around the glyphs.
                assert!(
                    first.right() < area.right() - px(1.),
                    "the first segment ends at its text, not the right edge"
                );
                assert!(
                    rest.left() <= area.left(),
                    "the continuation starts at the row's text"
                );
                assert!(
                    first.size.width + rest.size.width < whole + px(8.),
                    "neither segment carries the other's part"
                );
                assert!(first.size.width > px(6.) && rest.size.width > px(6.));
            }
        });
    }

    /// A host that stacks the editor in its own scroll view, like mdoc's
    /// document view (ADR 0054).
    struct ScrollHost {
        editor: gpui::Entity<super::EditorState>,
        scroll: gpui::ScrollHandle,
    }

    impl gpui::Render for ScrollHost {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            use gpui::{InteractiveElement, ParentElement, StatefulInteractiveElement, Styled};
            gpui::div()
                .id("scroll")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .child(self.editor.clone())
        }
    }

    fn scroll_host(
        cx: &mut gpui::TestAppContext,
    ) -> (
        gpui::Entity<super::EditorState>,
        gpui::ScrollHandle,
        &mut gpui::VisualTestContext,
    ) {
        use super::*;
        use gpui::AppContext as _;
        let scroll = ScrollHandle::new();
        let (host, cx) = cx.add_window_view(|window, cx| {
            let editor = cx.new(|cx| {
                let mut editor = EditorState::new(window, cx);
                let text = (0..120).map(|i| format!("line {i}")).collect::<Vec<_>>();
                editor.set_text(text.join("\n"), cx);
                editor.set_autoscroll(scroll.clone());
                editor
            });
            ScrollHost {
                editor,
                scroll: scroll.clone(),
            }
        });
        cx.simulate_resize(size(px(400.), px(300.)));
        let editor = host.read_with(cx, |host, _| host.editor.clone());
        editor.update_in(cx, |e, window, cx| e.focus(window, cx));
        cx.run_until_parked();
        (editor, scroll, cx)
    }

    #[gpui::test]
    fn keyboard_selection_keeps_the_caret_in_view(cx: &mut gpui::TestAppContext) {
        use super::*;
        let (editor, scroll, cx) = scroll_host(cx);
        let caret_visible = |cx: &mut gpui::VisualTestContext| {
            cx.run_until_parked();
            let caret = editor.read_with(cx, |e, _| e.bounds_for_offset(e.cursor()).unwrap());
            let viewport = scroll.bounds();
            assert!(
                caret.top() >= viewport.top() && caret.bottom() <= viewport.bottom(),
                "caret {caret:?} outside {viewport:?}"
            );
        };
        // A move away from the edges never scrolls.
        cx.dispatch_action(Down);
        caret_visible(cx);
        assert_eq!(scroll.offset().y, px(0.));
        for _ in 0..40 {
            cx.dispatch_action(SelectDown);
            caret_visible(cx);
        }
        assert!(scroll.offset().y < px(0.), "Shift+Down scrolled down");
        // Three lines of context stay below the caret.
        let (caret, lh) = editor.read_with(cx, |e, _| {
            (e.bounds_for_offset(e.cursor()).unwrap(), e.line_height)
        });
        assert!(scroll.bounds().bottom() - caret.bottom() >= lh * 3. - px(1.));
        let scrolled = scroll.offset().y;
        for _ in 0..40 {
            cx.dispatch_action(SelectUp);
            caret_visible(cx);
        }
        assert!(scroll.offset().y > scrolled, "Shift+Up scrolled back up");
    }

    #[gpui::test]
    fn held_drag_at_the_edge_scrolls_and_extends_until_release(cx: &mut gpui::TestAppContext) {
        use super::*;
        let (editor, scroll, cx) = scroll_host(cx);
        let viewport = scroll.bounds();
        let from = point(viewport.left() + px(20.), viewport.top() + px(10.));
        cx.simulate_mouse_down(from, MouseButton::Left, Default::default());
        // Pointer held still just inside the bottom edge: no further moves.
        let edge = point(from.x, viewport.bottom() - px(2.));
        cx.simulate_mouse_move(edge, Some(MouseButton::Left), Default::default());
        let before = editor.read_with(cx, |e, _| e.cursor());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(500));
        cx.run_until_parked();
        assert!(scroll.offset().y < px(0.), "held drag scrolled");
        let extended = editor.read_with(cx, |e, _| e.cursor());
        assert!(extended > before, "selection followed the scroll");
        cx.simulate_mouse_up(edge, MouseButton::Left, Default::default());
        cx.run_until_parked();
        let released = scroll.offset();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(500));
        cx.run_until_parked();
        assert_eq!(scroll.offset(), released, "release stops the scroll");
    }
}
