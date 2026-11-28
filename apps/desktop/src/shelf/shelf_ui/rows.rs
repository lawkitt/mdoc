//! The shelf's rows: pseudonymized documents as small previews to open,
//! drag out or Copy for AI, with the drag hint under them, or the empty
//! line. A view of its own: the shelf popup hosts it, and so does the
//! companion window ([ADR 0128](../../docs/adr/0128-floating-companion-window.md))
//! in its place. What follows a row's use is the host's: it hears
//! [`ShelfRowsEvent`]s, and it owns the keys and the drop target, calling
//! in here. `view.rs` and `row.rs` draw the rows; `actions.rs` carries out
//! what they do.
//!
//! Only ready documents copy or drag; previews and exports hold
//! pseudonymized text only (the menu bar shows in screen shares). A row
//! that is not ready carries the step that resolves it ("Replace 2 and
//! copy", "Review", "Find names"), and a Copy for AI on it says why.

mod actions;
mod row;
mod view;

pub(crate) use actions::clear_exports;

#[cfg(test)]
pub(crate) use row::row_label;

use crate::{
    review::refusal::Step,
    shelf::{self, ShelfDocument},
    ui::style::Theme,
    workspace::Workspace,
};
use gpui::{Context, EventEmitter, FocusHandle, Pixels, Point, Window, WindowHandle};
use std::{cell::Cell, rc::Rc};

const ROW_GAP: f32 = 2.;
const HINT: f32 = 22.;
const EMPTY: f32 = 44.;
/// A drag starts after the pointer moves this far with the button down.
const DRAG_DISTANCE: f32 = 4.;
/// Rows drag into other apps: macOS only. GPUI has no drag out of a window
/// on Windows, so there a row copies and opens, with no hint (item 17).
const ROWS_DRAG: bool = cfg!(any(target_os = "macos", test));

/// Each row's "N replaced" as last painted, for renders to hover.
#[cfg(all(
    debug_assertions,
    feature = "headless-renders",
    any(target_os = "macos", target_os = "windows")
))]
type CountBounds = Rc<std::cell::RefCell<Vec<(usize, gpui::Bounds<Pixels>)>>>;

/// Whether the drag hint shows: before the first drop, over a preview.
fn shows_hint(documents: &[ShelfDocument], drag_hint: bool) -> bool {
    ROWS_DRAG
        && drag_hint
        && documents
            .iter()
            .any(|d| d.readiness.is_ready() && d.preview.is_some())
}

/// The rows' height for these documents, the hint included.
pub(crate) fn height(documents: &[ShelfDocument], drag_hint: bool) -> f32 {
    if documents.is_empty() {
        return EMPTY;
    }
    documents
        .iter()
        .map(|d| ROW_GAP + row::height(d))
        .sum::<f32>()
        + if shows_hint(documents, drag_hint) {
            HINT
        } else {
            0.
        }
}

/// What a row's use asks of its host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShelfRowsEvent {
    /// Copy for AI put the row's text on the clipboard; "Copied" shows.
    Copied(usize),
    /// A drag of the row started.
    Dragged(usize),
    /// A click on the row: open it in mdoc.
    Open(usize),
    /// A refused row's step: run it in mdoc.
    Step(usize, Step),
}

pub(crate) struct ShelfRows {
    workspace: WindowHandle<Workspace>,
    theme: Rc<Cell<Theme>>,
    documents: Vec<ShelfDocument>,
    /// "Drag a preview into any app" shows until a drag has dropped.
    drag_hint: bool,
    /// One focus handle per document row, for ↑↓ and the focus ring.
    rows: Vec<FocusHandle>,
    /// One per row for its step's button, where a Copy for AI it refused
    /// sends focus.
    steps: Vec<FocusHandle>,
    /// Each row's preview as it fits two lines, and its title as it fits
    /// with its width, measured once.
    fitted: Vec<Option<shelf::Preview>>,
    titles: Vec<(String, f32)>,
    /// A press on a ready row that may become a drag: its row and place.
    press: Option<(usize, Point<Pixels>)>,
    /// The row whose Copy for AI just ran.
    copied: Option<usize>,
    /// The row whose Copy for AI was refused: its meta line says so.
    refused: Option<usize>,
    /// The running drag was dropped on the host itself: not a drop.
    dropped_here: bool,
    /// A drag of a row runs: a drop on the host is that drag coming back.
    dragging: bool,
    /// Where each row's "N replaced" was painted, so a render can hover it.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    counts: CountBounds,
    /// The last drag's payload and card, for tests (no native drag runs
    /// there).
    #[cfg(test)]
    pub(crate) dragged: Option<(String, std::path::PathBuf)>,
    #[cfg(test)]
    pub(crate) dragged_card: Option<(String, String)>,
}

impl EventEmitter<ShelfRowsEvent> for ShelfRows {}

impl ShelfRows {
    pub(crate) fn new(
        workspace: WindowHandle<Workspace>,
        documents: Vec<ShelfDocument>,
        drag_hint: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = workspace
            .read(cx)
            .map(|workspace| workspace.theme_cell())
            .unwrap_or_default();
        let rows = documents
            .iter()
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        let steps = documents
            .iter()
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        let fitted = documents
            .iter()
            .map(|document| {
                document
                    .preview
                    .as_ref()
                    .map(|preview| row::fit_preview(preview, row::PREVIEW_WIDTH, window))
            })
            .collect();
        let titles = documents
            .iter()
            .map(|document| row::fit_title(document, window))
            .collect();
        Self {
            workspace,
            theme,
            documents,
            drag_hint,
            rows,
            steps,
            fitted,
            titles,
            press: None,
            copied: None,
            refused: None,
            dropped_here: false,
            dragging: false,
            #[cfg(all(
                debug_assertions,
                feature = "headless-renders",
                any(target_os = "macos", target_os = "windows")
            ))]
            counts: Rc::default(),
            #[cfg(test)]
            dragged: None,
            #[cfg(test)]
            dragged_card: None,
        }
    }

    pub(crate) fn documents(&self) -> &[ShelfDocument] {
        &self.documents
    }

    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn drag_hint(&self) -> bool {
        self.drag_hint
    }

    /// The row whose "Copied" shows, for tests and renders.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn copied(&self) -> Option<usize> {
        self.copied
    }

    /// The row whose Copy for AI was refused, for tests and renders.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn refused(&self) -> Option<usize> {
        self.refused
    }

    /// Show "Copied" on `index` without copying, for renders.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn show_copied(&mut self, index: usize) {
        self.copied = Some(index);
    }

    /// The middle of row `index`'s "N replaced" as last painted, for
    /// renders to hover.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn count_center(&self, index: usize) -> Option<Point<Pixels>> {
        self.counts
            .borrow()
            .iter()
            .find(|(row, _)| *row == index)
            .map(|(_, bounds)| bounds.center())
    }

    /// Whether row `index`'s step button has focus, for tests.
    #[cfg(test)]
    pub(crate) fn step_focused(&self, index: usize, window: &Window) -> bool {
        self.steps
            .get(index)
            .is_some_and(|step| step.is_focused(window))
    }

    /// The first row's focus handle: where focus starts.
    pub(crate) fn first_row(&self) -> Option<&FocusHandle> {
        self.rows.first()
    }

    pub(crate) fn focused_row(&self, window: &Window) -> Option<usize> {
        self.rows.iter().position(|row| row.is_focused(window))
    }

    /// Whether a drag of a row runs: a host with a drop target of its own
    /// takes a drop then as the drag coming back.
    pub(crate) fn dragging(&self) -> bool {
        self.dragging
    }
}
