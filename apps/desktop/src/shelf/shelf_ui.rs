//! The shelf's popup: the borderless window under the menu bar
//! item. The shelf's rows ([`ShelfRows`]): pseudonymized documents as small
//! previews to open, drag out or Copy for AI; Restart to update; the
//! Clipboard section with Pseudonymize and Restore for one source; and one
//! footer row: Open mdoc, Settings and Quit. It closes on a click outside
//! (the panel loses key status), Esc, a Copy for AI after its "Copied", or
//! an action that brings mdoc forward. `view.rs` draws it and
//! `actions.rs` carries out what it does.

mod actions;
mod note;
mod rows;
mod view;

pub(crate) use note::NoteView;
pub(crate) use note::{NOTE_HEIGHT, NOTE_WIDTH};
#[cfg(all(
    debug_assertions,
    feature = "headless-renders",
    any(target_os = "macos", target_os = "windows")
))]
pub(crate) use note::{NOTE_TEXT, NOTE_TITLE};
#[cfg(test)]
pub(crate) use rows::row_label;
pub(crate) use rows::{ShelfRows, ShelfRowsEvent, clear_exports, height as rows_height};

use crate::{
    shelf::menu_bar,
    shelf::{ShelfDocument, text},
    ui::style::Theme,
    workspace::{ClipboardSource, Workspace},
};
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, Global, Hsla, KeyBinding, Subscription, Task,
    Window, WindowHandle, actions, prelude::*,
};
use std::{cell::Cell, rc::Rc, time::Duration};

actions!(shelf, [CloseShelf, ShelfUp, ShelfDown, ShelfCopyForAi]);

/// ⌘1–⌘8: Copy for AI on that row, numbered from 1.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = shelf, no_json)]
pub(crate) struct ShelfCopyRow(pub usize);

/// Keys inside the shelf: ↑↓ and Esc are plumbing (moving inside a
/// list); ⌘C and ⌘1–⌘8 are the table's Copy for AI rows; Enter
/// activates the focused row like any control.
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseShelf, Some("Shelf")),
        KeyBinding::new("up", ShelfUp, Some("Shelf")),
        KeyBinding::new("down", ShelfDown, Some("Shelf")),
    ]);
    let bindings =
        crate::commands::shortcuts::bindings(crate::commands::shortcuts::Owner::Shelf, cx);
    cx.bind_keys(bindings);
}

const PADDING: f32 = 8.;
const ACTION_ROW: f32 = 30.;
const DIVIDER: f32 = 9.;
const CLIPBOARD_HEADING: f32 = 20.;
const CLIPBOARD_BUTTONS: f32 = 28.;
const SOURCE_LINE: f32 = 24.;
const SECTION_GAP: f32 = 4.;
const CLIPBOARD: f32 = CLIPBOARD_HEADING + CLIPBOARD_BUTTONS + SOURCE_LINE + 2. * SECTION_GAP;
const FOOTER: f32 = 30.;
const PICKER_HEADING: f32 = 24.;
/// "Copied" shows this long before the shelf closes.
pub(crate) const COPIED_FOR: Duration = Duration::from_millis(800);
/// A clipboard result stays on the source line this long.
pub(crate) const RESULT_FOR: Duration = Duration::from_secs(4);

/// The shelf's ground: the theme's background, translucent over the native
/// blur (vibrancy) the window draws behind it on macOS.
pub(crate) fn surface(theme: Theme) -> Hsla {
    theme.pdf_style().bg.opacity(0.78)
}

/// The popup's height for content measured `measured` tall: the content's,
/// whole points, never past `max` (the room under the item), so a long
/// shelf scrolls.
pub(crate) fn fitted_height(measured: f32, max: f32) -> f32 {
    measured.ceil().min(max)
}

/// The shelf popup follows its content's height: after each
/// layout the content is measured and, when that changes the height, the
/// window is resized with its top kept under the item. One path for every
/// change: the picker, the hint, a result line.
pub(crate) struct Fit {
    max: f32,
    /// The height last asked for.
    asked: Cell<f32>,
}

impl Fit {
    /// A popup `height` tall now, growing to at most `max`.
    pub(crate) fn new(height: f32, max: f32) -> Self {
        Self {
            max,
            asked: Cell::new(height),
        }
    }

    #[cfg(test)]
    pub(crate) fn asked(&self) -> f32 {
        self.asked.get()
    }

    /// The content measured `height` tall after a layout: resize to fit.
    pub(crate) fn measured(&self, height: f32, window: &mut Window, cx: &mut App) {
        let wanted = fitted_height(height, self.max);
        if (wanted - self.asked.get()).abs() < 0.5 {
            return;
        }
        self.asked.set(wanted);
        // After this frame, outside layout.
        let handle = window.window_handle();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                #[cfg(all(target_os = "macos", not(test)))]
                gpui_macos::set_window_height_keeping_top(window, cx, wanted);
                // Only the companion fits itself there (item 17).
                #[cfg(all(windows, not(test)))]
                crate::ai::companion::windows::set_height(window, wanted, cx);
                let _ = (window, cx);
            });
        });
    }
}

/// Project `id` among the sources, named as the shelf shows it.
fn project_source(sources: &[ClipboardSource], id: &str) -> Option<ClipboardSource> {
    sources
        .iter()
        .find(|s| matches!(s, ClipboardSource::Project { id: project, .. } if project == id))
        .cloned()
}

/// What the shelf shows, read from the workspace when it opens.
pub(crate) struct ShelfContent {
    pub documents: Vec<ShelfDocument>,
    pub sources: Vec<ClipboardSource>,
    pub source: Option<ClipboardSource>,
    pub update: Option<String>,
    /// "Drag a preview into any app" shows until a drag has dropped.
    pub drag_hint: bool,
}

/// The clipboard rows' Project, as last picked on the shelf this session.
#[derive(Default)]
struct PickedSource(Option<ClipboardSource>);
impl Global for PickedSource {}

impl ShelfContent {
    pub(crate) fn of(workspace: &Workspace, cx: &App) -> Self {
        let documents = workspace.shelf_documents(cx);
        let sources = workspace.clipboard_sources(cx);
        let picked = cx
            .try_global::<PickedSource>()
            .and_then(|picked| picked.0.clone())
            .filter(|picked| sources.contains(picked));
        // The document used last: a pick on the shelf, the active tab, or
        // the Project of the most recent row.
        let source = picked
            .or_else(|| workspace.active_clipboard_source(cx))
            .or_else(|| {
                documents
                    .iter()
                    .find_map(|d| d.project.as_ref())
                    .and_then(|project| project_source(&sources, &project.id))
            })
            .or_else(|| sources.first().cloned());
        let update = match crate::app::updater::state(cx).install {
            crate::app::updater::Install::Ready { version, .. } => Some(version),
            _ => None,
        };
        Self {
            documents,
            sources,
            source,
            update,
            drag_hint: !workspace.shelf_drag_hint_shown(),
        }
    }

    /// The shelf's height for this content, before the screen limits it.
    pub(crate) fn height(&self) -> f32 {
        rows::height(&self.documents, self.drag_hint) + chrome_height(self.update.is_some())
    }
}

/// Everything but the rows: padding, dividers, Restart to update, the
/// Clipboard section and the footer.
fn chrome_height(update: bool) -> f32 {
    let update = if update { ACTION_ROW } else { 0. };
    2. * PADDING + DIVIDER + update + CLIPBOARD + DIVIDER + FOOTER
}

/// A clipboard action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClipboardRow {
    Pseudonymize,
    Restore,
}

pub(crate) struct ShelfView {
    workspace: WindowHandle<Workspace>,
    theme: Rc<Cell<Theme>>,
    focus: FocusHandle,
    /// The documents, a view of their own.
    rows: Entity<ShelfRows>,
    _rows_events: Subscription,
    sources: Vec<ClipboardSource>,
    /// The clipboard rows' source.
    source: Option<ClipboardSource>,
    /// The version Restart to update installs.
    update: Option<String>,
    /// Set after a row's Copy for AI; the shelf closes when it ends.
    closing: Option<Task<()>>,
    /// The source picker replaces the list while open.
    picking: bool,
    /// One focus handle per picker entry, for ↑↓.
    picker_rows: Vec<FocusHandle>,
    /// The ▾ that opens the picker; focus returns to it.
    pick_button: FocusHandle,
    /// Set when the shelf is its own popup: it then follows its content.
    fit: Option<Rc<Fit>>,
    /// The last clipboard action's result, on the source line for a while.
    result: Option<String>,
    result_clear: Option<Task<()>>,
}

impl ShelfView {
    pub(crate) fn new(
        workspace: WindowHandle<Workspace>,
        content: ShelfContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let ShelfContent {
            documents,
            sources,
            source,
            update,
            drag_hint,
        } = content;
        let theme = workspace
            .read(cx)
            .map(|workspace| workspace.theme_cell())
            .unwrap_or_default();
        let focus = cx.focus_handle().tab_stop(false);
        let picker_rows = sources
            .iter()
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        let rows = cx.new(|cx| ShelfRows::new(workspace, documents, drag_hint, window, cx));
        let rows_events =
            cx.subscribe_in(&rows, window, |this, _, event, window, cx| match *event {
                ShelfRowsEvent::Copied(index) => this.after_copy(index, window, cx),
                ShelfRowsEvent::Dragged(index) => this.used(index, cx),
                ShelfRowsEvent::Open(index) => this.open(index, window, cx),
                ShelfRowsEvent::Step(index, step) => this.run_step(index, step, window, cx),
            });
        // Focus starts on the first row.
        let first = rows.read(cx).first_row().cloned();
        window.focus(first.as_ref().unwrap_or(&focus), cx);
        // A click outside takes key status from the panel: close it.
        cx.observe_window_activation(window, |_, window, cx| {
            if !window.is_window_active() {
                window.remove_window();
                menu_bar::shelf_closed(cx);
            }
        })
        .detach();
        Self {
            workspace,
            theme,
            focus,
            rows,
            _rows_events: rows_events,
            sources,
            source,
            update,
            closing: None,
            picking: false,
            picker_rows,
            pick_button: cx.focus_handle().tab_stop(true),
            fit: None,
            result: None,
            result_clear: None,
        }
    }

    /// The rows, for tests.
    #[cfg(test)]
    pub(crate) fn rows(&self) -> &Entity<ShelfRows> {
        &self.rows
    }

    /// What the shelf shows now, for tests and renders.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn content(&self, cx: &App) -> ShelfContent {
        let rows = self.rows.read(cx);
        ShelfContent {
            documents: rows.documents().to_vec(),
            sources: self.sources.clone(),
            source: self.source.clone(),
            update: self.update.clone(),
            drag_hint: rows.drag_hint(),
        }
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
    pub(crate) fn copied(&self, cx: &App) -> Option<usize> {
        self.rows.read(cx).copied()
    }

    /// Show "Copied" on `index` without copying or closing, for renders.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn show_copied(&mut self, index: usize, cx: &mut App) {
        self.rows.update(cx, |rows, _| rows.show_copied(index));
    }

    /// The middle of row `index`'s "N replaced" as last painted, for
    /// renders to hover.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn count_center(&self, index: usize, cx: &App) -> Option<gpui::Point<gpui::Pixels>> {
        self.rows.read(cx).count_center(index)
    }

    /// The shelf is its own popup, `height` tall with at most `max` under
    /// the item: from now on it follows its content's height.
    pub(crate) fn fit_to_screen(&mut self, height: f32, max: f32) {
        self.fit = Some(Rc::new(Fit::new(height, max)));
    }

    /// The popup's fitting, for tests.
    #[cfg(test)]
    pub(crate) fn fit(&self) -> Option<Rc<Fit>> {
        self.fit.clone()
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
    pub(crate) fn refused(&self, cx: &App) -> Option<usize> {
        self.rows.read(cx).refused()
    }

    fn focused_row(&self, window: &Window, cx: &App) -> Option<usize> {
        if self.picking {
            return None;
        }
        self.rows.read(cx).focused_row(window)
    }

    /// The source line: a recent result, or "Names from Ivanov".
    pub(crate) fn source_line(&self) -> Option<String> {
        self.result.clone().or_else(|| {
            self.source
                .as_ref()
                .map(|source| text::names_from(source.name()))
        })
    }
}

impl Focusable for ShelfView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
