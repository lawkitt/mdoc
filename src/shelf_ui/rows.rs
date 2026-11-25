//! The shelf's rows: pseudonymized documents as small previews to open,
//! drag out or Copy for AI, with the drag hint under them, or the empty
//! line. A view of its own: the shelf popup hosts it, and the companion
//! window ([ADR 0128](../../docs/adr/0128-floating-companion-window.md)) is
//! to host it next. What follows a row's use is the host's: it hears
//! [`ShelfRowsEvent`]s, and it owns the keys and the drop target, calling
//! in here.
//!
//! Only ready documents copy or drag; previews and exports hold
//! pseudonymized text only (the menu bar shows in screen shares). A row
//! that is not ready carries the step that resolves it ("Replace 2 and
//! copy", "Review", "Find names"), and a Copy for AI on it says why.

mod row;

#[cfg(test)]
pub(crate) use row::row_label;

use crate::{
    refusal::Step,
    shelf::{self, ShelfDocument, ShelfExports},
    style::Theme,
    workspace::Workspace,
};
use gpui::{
    AnyElement, App, ClipboardItem, Context, EventEmitter, FocusHandle, Global, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, Point, Render, Window, WindowHandle, div, prelude::*,
    px,
};
use std::{cell::Cell, rc::Rc};

const ROW_GAP: f32 = 2.;
const HINT: f32 = 22.;
const EMPTY: f32 = 44.;
/// A drag starts after the pointer moves this far with the button down.
const DRAG_DISTANCE: f32 = 4.;

/// Each row's "N replaced" as last painted, for renders to hover.
#[cfg(all(
    debug_assertions,
    feature = "headless-renders",
    any(target_os = "macos", target_os = "windows")
))]
type CountBounds = Rc<std::cell::RefCell<Vec<(usize, gpui::Bounds<Pixels>)>>>;

/// The drag exports, alive until quit.
#[derive(Default)]
struct Exports(Option<ShelfExports>);
impl Global for Exports {}

/// Remove every shelf export, at quit.
pub(crate) fn clear_exports(cx: &mut App) {
    if let Some(exports) = cx.try_global::<Exports>().and_then(|e| e.0.as_ref()) {
        exports.clear();
    }
}

fn export(document: &ShelfDocument, cx: &mut App) -> Option<std::path::PathBuf> {
    if cx.try_global::<Exports>().is_none_or(|e| e.0.is_none()) {
        let exports = ShelfExports::create().ok()?;
        cx.set_global(Exports(Some(exports)));
    }
    cx.global::<Exports>().0.as_ref()?.export(document).ok()?
}

/// Whether the drag hint shows: before the first drop, over a preview.
fn shows_hint(documents: &[ShelfDocument], drag_hint: bool) -> bool {
    drag_hint
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

    /// The first row's focus handle: where focus starts.
    pub(crate) fn first_row(&self) -> Option<&FocusHandle> {
        self.rows.first()
    }

    pub(crate) fn focused_row(&self, window: &Window) -> Option<usize> {
        self.rows.iter().position(|row| row.is_focused(window))
    }

    /// ↑↓ move from row to row.
    pub(crate) fn move_focus(&self, by: isize, window: &mut Window, cx: &mut App) {
        if self.rows.is_empty() {
            return;
        }
        let at = self
            .rows
            .iter()
            .position(|target| target.is_focused(window))
            .map_or(0, |at| {
                (at as isize + by).clamp(0, self.rows.len() as isize - 1) as usize
            });
        window.focus(&self.rows[at], cx);
    }

    /// Row `index`, if listed ready, checked again as its text is about to
    /// leave: the Vault's outbound check runs now, and a row it refuses
    /// shows why and hands out nothing.
    fn checked_again(&mut self, index: usize, window: &mut Window, cx: &App) -> Option<String> {
        let listed = self.documents.get(index)?;
        listed.text_for_ai()?;
        let now = (self.workspace.read(cx).ok())
            .and_then(|workspace| workspace.shelf_row(&listed.key, cx))
            .filter(|now| now.key == listed.key)?;
        let text = now.text_for_ai().map(str::to_owned);
        self.fitted[index] = (now.preview.as_ref())
            .map(|preview| row::fit_preview(preview, row::PREVIEW_WIDTH, window));
        self.titles[index] = row::fit_title(&now, window);
        self.documents[index] = now;
        text
    }

    /// A test of the exits suite took `gate`'s check out: row `index`
    /// hands out what it listed, unchecked.
    #[cfg(test)]
    fn unchecked(&self, gate: crate::exits::Gate, index: usize) -> Option<Option<String>> {
        crate::exits::taken_out(gate).then(|| {
            let listed = self.documents.get(index)?;
            listed.text_for_ai().map(str::to_owned)
        })
    }

    /// Copy for AI on a ready row: the text a drag carries, then ✓ "Copied".
    /// On a row that is not ready (a click, ⌘C or ⌘1…⌘8) nothing is
    /// copied: its meta line says "Not copied" with its state, and focus
    /// goes to its step.
    pub(crate) fn copy(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(test)]
        let text = (self.unchecked(crate::exits::Gate::ShelfCopy, index))
            .unwrap_or_else(|| self.checked_again(index, window, cx));
        #[cfg(not(test))]
        let text = self.checked_again(index, window, cx);
        let Some(text) = text else {
            if let Some(document) = self.documents.get(index) {
                self.refused = Some(index);
                if document.step().is_some() {
                    window.focus(&self.steps[index], cx);
                }
            }
            cx.notify();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.refused = None;
        self.copied = Some(index);
        cx.emit(ShelfRowsEvent::Copied(index));
        cx.notify();
    }

    fn mouse_down(&mut self, index: usize, event: &MouseDownEvent) {
        self.press = self
            .documents
            .get(index)
            .filter(|d| d.readiness.is_ready())
            .map(|_| (index, event.position));
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some((index, start)) = self.press else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.press = None;
            return;
        }
        let moved = event.position - start;
        if moved.x.abs().max(moved.y.abs()) < px(DRAG_DISTANCE) {
            return;
        }
        self.press = None;
        self.drag(index, window, cx);
    }

    /// Drag a ready document: its Copy for AI text and a fresh `.md` export
    /// on one pasteboard item, under a card with its title and preview. The
    /// first drop retires the hint.
    pub(crate) fn drag(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(test)]
        let text = (self.unchecked(crate::exits::Gate::ShelfDrag, index))
            .unwrap_or_else(|| self.checked_again(index, window, cx));
        #[cfg(not(test))]
        let text = self.checked_again(index, window, cx);
        let Some(text) = text else {
            cx.notify();
            return;
        };
        let Some(document) = self.documents.get(index) else {
            return;
        };
        let card = document.drag_card();
        let Some(path) = export(document, cx) else {
            return;
        };
        self.dropped_here = false;
        #[cfg(all(target_os = "macos", not(test)))]
        {
            let workspace = self.workspace;
            let view = cx.entity().downgrade();
            let async_cx = cx.to_async();
            // AppKit reports the end inside its own event: finish on GPUI's
            // executor. The host may have closed by then.
            let on_end = Box::new(move |dropped: bool| {
                let cx = async_cx.clone();
                cx.foreground_executor()
                    .clone()
                    .spawn(async move {
                        cx.update(|cx| match view.upgrade() {
                            Some(view) => view.update(cx, |view, cx| view.drag_ended(dropped, cx)),
                            None if dropped => {
                                let _ = workspace.update(cx, |workspace, _, cx| {
                                    workspace.update_general(
                                        |general| general.shelf_drag_hint_shown = true,
                                        cx,
                                    )
                                });
                            }
                            None => {}
                        });
                    })
                    .detach();
            });
            let card = card.as_ref().map(|(title, line)| gpui_macos::DragCard {
                title,
                line,
                dark: self.theme.get() == Theme::Dark,
            });
            gpui_macos::start_text_and_file_drag(window, &text, &path, card, on_end);
        }
        #[cfg(not(all(target_os = "macos", not(test))))]
        let _ = window;
        #[cfg(test)]
        {
            self.dragged = Some((text, path));
            self.dragged_card = card;
        }
        #[cfg(not(any(target_os = "macos", test)))]
        let _ = (text, path, card);
        cx.emit(ShelfRowsEvent::Dragged(index));
    }

    /// A refused row's step, as its button runs it: the host opens the
    /// document in mdoc and runs it there.
    pub(crate) fn step(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(step) = self.documents.get(index).and_then(ShelfDocument::step) {
            cx.emit(ShelfRowsEvent::Step(index, step));
        }
    }

    /// The running drag was released over the host itself.
    pub(crate) fn dropped_on_shelf(&mut self) {
        self.dropped_here = true;
    }

    /// A drag ended; a drop somewhere retires "Drag a preview into any app"
    /// for good.
    // Only the macOS drag reports its end; elsewhere there is no shelf.
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) fn drag_ended(&mut self, dropped: bool, cx: &mut Context<Self>) {
        // A drop back on the host took nothing anywhere.
        let elsewhere = dropped && !std::mem::take(&mut self.dropped_here);
        if !elsewhere || !std::mem::take(&mut self.drag_hint) {
            return;
        }
        let _ = self.workspace.update(cx, |workspace, _, cx| {
            workspace.update_general(|general| general.shelf_drag_hint_shown = true, cx)
        });
        cx.notify();
    }

    fn hint(&self) -> AnyElement {
        let p = self.theme.get().pdf_style();
        div()
            .h(px(HINT))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(11.))
            .text_color(p.header_muted)
            .child("Drag a preview into any app")
            .into_any_element()
    }
}

impl Render for ShelfRows {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.theme.get().pdf_style();
        if self.documents.is_empty() {
            return div()
                .h(px(EMPTY))
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.header_muted)
                .child("Pseudonymized documents appear here");
        }
        div()
            .flex()
            .flex_col()
            .gap(px(ROW_GAP))
            // A release anywhere ends a press that did not become a drag.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.press = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.press = None),
            )
            .children((0..self.documents.len()).map(|i| self.document_row(i, cx)))
            .when(shows_hint(&self.documents, self.drag_hint), |v| {
                v.child(self.hint())
            })
    }
}
