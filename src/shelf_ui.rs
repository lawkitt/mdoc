//! The shelf's view (ADR 0084): the borderless popup under the menu bar
//! item. Pseudonymized documents as small previews to open, drag out or
//! Copy for AI; Restart to update; the Clipboard section with Pseudonymize
//! and Restore for one source; and one footer row: Open mdoc, Settings and
//! Quit. It closes on a click outside (the panel loses key status), Esc, a
//! Copy for AI after its "Copied", or an action that brings mdoc forward.
//!
//! Only ready documents copy or drag; previews and exports hold
//! pseudonymized text only (the menu bar shows in screen shares).

mod note;
mod row;

pub(crate) use note::NoteView;
pub(crate) use note::{NOTE_HEIGHT, NOTE_WIDTH};
#[cfg(all(
    debug_assertions,
    feature = "headless-renders",
    any(target_os = "macos", target_os = "windows")
))]
pub(crate) use note::{NOTE_TEXT, NOTE_TITLE};
#[cfg(test)]
pub(crate) use row::row_label;

use crate::{
    menu_bar,
    shelf::{self, ShelfDocument, ShelfExports, ShelfKey},
    style::Theme,
    ui,
    workspace::{ClipboardNames, ClipboardSource, Quit, Workspace},
};
use gpui::{
    AnyElement, App, ClipboardItem, Context, FocusHandle, Focusable, Global, Hsla, KeyBinding,
    MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, Render, SharedString, Task, Window,
    WindowHandle, actions, div, prelude::*, px,
};
use mdoc_pii::restore::RestoreMap;
use std::{cell::Cell, rc::Rc, time::Duration};

actions!(shelf, [CloseShelf, ShelfUp, ShelfDown, ShelfCopyForAi]);

/// ⌘1–⌘8: Copy for AI on that row, numbered from 1 (ADR 0084).
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = shelf, no_json)]
pub(crate) struct ShelfCopyRow(pub usize);

/// Keys inside the shelf: ↑↓ and Esc are plumbing (moving inside a list,
/// ADR 0073); ⌘C and ⌘1–⌘8 are the table's Copy for AI rows; Enter
/// activates the focused row like any control.
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseShelf, Some("Shelf")),
        KeyBinding::new("up", ShelfUp, Some("Shelf")),
        KeyBinding::new("down", ShelfDown, Some("Shelf")),
    ]);
    cx.bind_keys(crate::shortcuts::bindings(crate::shortcuts::Owner::Shelf));
}

const PADDING: f32 = 8.;
const ROW_GAP: f32 = 2.;
const HINT: f32 = 22.;
const ACTION_ROW: f32 = 30.;
const EMPTY: f32 = 44.;
const DIVIDER: f32 = 9.;
const CLIPBOARD_HEADING: f32 = 20.;
const CLIPBOARD_BUTTONS: f32 = 28.;
const SOURCE_LINE: f32 = 24.;
const SECTION_GAP: f32 = 4.;
const CLIPBOARD: f32 = CLIPBOARD_HEADING + CLIPBOARD_BUTTONS + SOURCE_LINE + 2. * SECTION_GAP;
const FOOTER: f32 = 30.;
/// A drag starts after the pointer moves this far with the button down.
const DRAG_DISTANCE: f32 = 4.;
/// "Copied" shows this long before the shelf closes.
pub(crate) const COPIED_FOR: Duration = Duration::from_millis(800);
/// A clipboard result stays on the source line this long.
pub(crate) const RESULT_FOR: Duration = Duration::from_secs(4);

/// The shelf's ground: the theme's background, translucent over the native
/// blur (vibrancy) the window draws behind it on macOS.
pub(crate) fn surface(theme: Theme) -> Hsla {
    theme.pdf_style().bg.opacity(0.78)
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

/// The clipboard rows' case, as last picked on the shelf this session.
#[derive(Default)]
struct PickedSource(Option<ClipboardSource>);
impl Global for PickedSource {}

/// The drag exports, alive until quit (ADR 0084).
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

impl ShelfContent {
    pub(crate) fn of(workspace: &Workspace, cx: &App) -> Self {
        let documents = workspace.shelf_documents(cx);
        let sources = workspace.clipboard_sources(cx);
        let picked = cx
            .try_global::<PickedSource>()
            .and_then(|picked| picked.0.clone())
            .filter(|picked| sources.contains(picked));
        // The document used last: a pick on the shelf, the active tab, or
        // the case of the most recent row.
        let source = picked
            .or_else(|| workspace.active_clipboard_source(cx))
            .or_else(|| {
                documents.iter().find_map(|d| {
                    d.case.as_ref().map(|case| ClipboardSource::Case {
                        id: case.id.clone(),
                        name: case.name.clone(),
                    })
                })
            })
            .or_else(|| sources.first().cloned());
        let update = match crate::updater::state(cx).install {
            crate::updater::Install::Ready { version, .. } => Some(version),
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

    /// Whether the drag hint shows: before the first drop, over a preview.
    fn shows_hint(&self) -> bool {
        self.drag_hint
            && self
                .documents
                .iter()
                .any(|d| d.readiness.is_ready() && d.preview.is_some())
    }

    /// The shelf's height for this content, before the screen limits it.
    pub(crate) fn height(&self) -> f32 {
        let list = if self.documents.is_empty() {
            EMPTY
        } else {
            self.documents
                .iter()
                .map(|d| ROW_GAP + row::height(d))
                .sum::<f32>()
                + if self.shows_hint() { HINT } else { 0. }
        };
        let update = if self.update.is_some() {
            ACTION_ROW
        } else {
            0.
        };
        2. * PADDING + list + DIVIDER + update + CLIPBOARD + DIVIDER + FOOTER
    }
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
    content: ShelfContent,
    focus: FocusHandle,
    /// One focus handle per document row, for ↑↓ and the focus ring.
    rows: Vec<FocusHandle>,
    /// A press on a ready row that may become a drag: its row and place.
    press: Option<(usize, Point<Pixels>)>,
    /// The row whose Copy for AI just ran; the shelf closes after it.
    copied: Option<usize>,
    closing: Option<Task<()>>,
    /// The source picker replaces the list while open.
    picking: bool,
    /// One focus handle per picker entry, for ↑↓.
    picker_rows: Vec<FocusHandle>,
    /// The ▾ that opens the picker; focus returns to it.
    pick_button: FocusHandle,
    /// The last clipboard action's result, on the source line for a while.
    result: Option<String>,
    result_clear: Option<Task<()>>,
    /// The last drag's payload and card, for tests (no native drag runs
    /// there).
    #[cfg(test)]
    pub(crate) dragged: Option<(String, std::path::PathBuf)>,
    #[cfg(test)]
    pub(crate) dragged_card: Option<(String, String)>,
}

impl ShelfView {
    pub(crate) fn new(
        workspace: WindowHandle<Workspace>,
        content: ShelfContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = workspace
            .read(cx)
            .map(|workspace| workspace.theme_cell())
            .unwrap_or_default();
        let focus = cx.focus_handle().tab_stop(false);
        let picker_rows = content
            .sources
            .iter()
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        let rows: Vec<_> = content
            .documents
            .iter()
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        // Focus starts on the first row (ADR 0084).
        window.focus(rows.first().unwrap_or(&focus), cx);
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
            content,
            focus,
            rows,
            press: None,
            copied: None,
            closing: None,
            picking: false,
            picker_rows,
            pick_button: cx.focus_handle().tab_stop(true),
            result: None,
            result_clear: None,
            #[cfg(test)]
            dragged: None,
            #[cfg(test)]
            dragged_card: None,
        }
    }

    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn content(&self) -> &ShelfContent {
        &self.content
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

    /// Show "Copied" on `index` without copying or closing, for renders.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn show_copied(&mut self, index: usize) {
        self.copied = Some(index);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        menu_bar::shelf_closed(cx);
    }

    /// Close the shelf, bring mdoc forward and run `then` in its window.
    fn in_mdoc(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Workspace, &mut Window, &mut gpui::Context<Workspace>) + 'static,
    ) {
        self.close(window, cx);
        let workspace = self.workspace;
        cx.defer(move |cx| {
            let _ = workspace.update(cx, |workspace, window, cx| {
                menu_bar::reveal_window(window, cx);
                then(workspace, window, cx);
            });
        });
    }

    pub(crate) fn open(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.content.documents.get(index) else {
            return;
        };
        let key = document.key.clone();
        let ready = document.readiness.is_ready();
        self.in_mdoc(window, cx, move |workspace, window, cx| {
            workspace.open_from_shelf(&key, ready, window, cx)
        });
    }

    /// The row is the document used last: the clipboard source follows it.
    fn used(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(document) = self.content.documents.get(index) else {
            return;
        };
        let source = match (&document.case, &document.key) {
            (Some(case), _) => Some(ClipboardSource::Case {
                id: case.id.clone(),
                name: case.name.clone(),
            }),
            (None, ShelfKey::Tab(id)) => self
                .content
                .sources
                .iter()
                .find(|s| matches!(s, ClipboardSource::Tab { id: tab, .. } if tab == id))
                .cloned(),
            (None, ShelfKey::Member { .. }) => None,
        };
        if let Some(source) = source {
            self.pick(source, cx);
        }
    }

    /// Copy for AI on a ready row: the text a drag carries, then ✓ "Copied"
    /// and the shelf closes after [`COPIED_FOR`].
    pub(crate) fn copy(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self
            .content
            .documents
            .get(index)
            .and_then(ShelfDocument::text_for_ai)
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        self.copied = Some(index);
        self.used(index, cx);
        self.closing = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update_in(cx, |this, window, cx| this.close(window, cx));
        }));
        cx.notify();
    }

    fn focused_row(&self, window: &Window) -> Option<usize> {
        if self.picking {
            return None;
        }
        self.rows.iter().position(|row| row.is_focused(window))
    }

    /// ↑↓ move within the list that shows: the documents, or the picker.
    fn move_focus(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let targets = if self.picking {
            &self.picker_rows
        } else {
            &self.rows
        };
        if targets.is_empty() {
            return;
        }
        let at = targets
            .iter()
            .position(|target| target.is_focused(window))
            .map_or(0, |at| {
                (at as isize + by).clamp(0, targets.len() as isize - 1) as usize
            });
        window.focus(&targets[at], cx);
    }

    /// Open the picker on the current source, or close it, returning focus
    /// to the ▾.
    pub(crate) fn toggle_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picking {
            self.close_picker(window, cx);
            return;
        }
        self.picking = true;
        let at = self
            .content
            .sources
            .iter()
            .position(|source| Some(source) == self.content.source.as_ref())
            .unwrap_or(0);
        if let Some(target) = self.picker_rows.get(at) {
            window.focus(target, cx);
        }
        cx.notify();
    }

    fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picking = false;
        window.focus(&self.pick_button, cx);
        cx.notify();
    }

    fn mouse_down(&mut self, index: usize, event: &MouseDownEvent) {
        self.press = self
            .content
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
        let Some(document) = self.content.documents.get(index) else {
            return;
        };
        let Some(text) = document.text_for_ai().map(str::to_owned) else {
            return;
        };
        let card = document.drag_card();
        let Some(path) = export(document, cx) else {
            return;
        };
        #[cfg(all(target_os = "macos", not(test)))]
        {
            let workspace = self.workspace;
            let view = cx.entity().downgrade();
            let async_cx = cx.to_async();
            // AppKit reports the end inside its own event: finish on GPUI's
            // executor. The shelf may have closed by then.
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
        self.used(index, cx);
    }

    /// A drag ended; a drop somewhere retires "Drag a preview into any app"
    /// for good (ADR 0084).
    // Only the macOS drag reports its end; elsewhere there is no shelf.
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) fn drag_ended(&mut self, dropped: bool, cx: &mut Context<Self>) {
        if !dropped || !std::mem::take(&mut self.content.drag_hint) {
            return;
        }
        let _ = self.workspace.update(cx, |workspace, _, cx| {
            workspace.update_general(|general| general.shelf_drag_hint_shown = true, cx)
        });
        cx.notify();
    }

    fn pick(&mut self, source: ClipboardSource, cx: &mut Context<Self>) {
        if self.content.source.as_ref() != Some(&source) {
            self.result = None;
        }
        cx.set_global(PickedSource(Some(source.clone())));
        self.content.source = Some(source);
        cx.notify();
    }

    /// A picker entry: use its names, and close the picker.
    fn choose(&mut self, source: ClipboardSource, window: &mut Window, cx: &mut Context<Self>) {
        self.pick(source, cx);
        self.close_picker(window, cx);
    }

    /// Run a clipboard action with the current source's names; its result
    /// shows on the source line for [`RESULT_FOR`].
    pub(crate) fn clipboard(
        &mut self,
        row: ClipboardRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.content.source.clone() else {
            return;
        };
        let names = self
            .workspace
            .read(cx)
            .map(|workspace| workspace.clipboard_names(&source, cx));
        let names = match names {
            Ok(ClipboardNames::Names(names)) => names,
            Ok(ClipboardNames::Locked) => {
                self.in_mdoc(window, cx, |workspace, window, cx| {
                    workspace.unlock_from_shelf(window, cx)
                });
                return;
            }
            _ => return,
        };
        let summary = match cx.read_from_clipboard().and_then(|item| item.text()) {
            None => "The clipboard has no text".to_string(),
            Some(text) => match row {
                ClipboardRow::Restore => {
                    let restored = shelf::restore_clipboard(&text, names);
                    if restored.text != text {
                        cx.write_to_clipboard(ClipboardItem::new_string(restored.text.clone()));
                    }
                    restored.summary()
                }
                ClipboardRow::Pseudonymize => {
                    let (replaced, count) = RestoreMap::new(names).pseudonymize(&text);
                    if count > 0 {
                        cx.write_to_clipboard(ClipboardItem::new_string(replaced));
                    }
                    match count {
                        0 => "No known names found".to_string(),
                        1 => "1 name replaced".to_string(),
                        n => format!("{n} names replaced"),
                    }
                }
            },
        };
        self.result = Some(summary);
        self.result_clear = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(RESULT_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.result = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// The source line: a recent result, or "Names from Ivanov".
    pub(crate) fn source_line(&self) -> Option<String> {
        self.result.clone().or_else(|| {
            self.content
                .source
                .as_ref()
                .map(|source| format!("Names from {}", source.name()))
        })
    }

    pub(crate) fn clipboard_locked(&self, cx: &App) -> bool {
        let Some(source) = &self.content.source else {
            return false;
        };
        self.workspace.read(cx).is_ok_and(|workspace| {
            matches!(
                workspace.clipboard_names(source, cx),
                ClipboardNames::Locked
            )
        })
    }
}

impl Focusable for ShelfView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ShelfView {
    fn action_row(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> ui::Control {
        let theme = self.theme.get();
        ui::control(id, label, theme, true)
            .h(px(ACTION_ROW))
            .flex()
            .items_center()
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
    }

    fn divider(&self) -> AnyElement {
        let p = self.theme.get().pdf_style();
        div()
            .h(px(DIVIDER))
            .flex()
            .items_center()
            .px(px(4.))
            .child(div().w_full().h(px(1.)).bg(p.border))
            .into_any_element()
    }

    /// One of the two equal clipboard buttons.
    fn clipboard_button(
        &self,
        id: &'static str,
        label: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> ui::Control {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        ui::control(id, label, theme, enabled)
            .flex_1()
            .h(px(CLIPBOARD_BUTTONS))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(p.border)
            .text_size(px(12.))
            .font_weight(gpui::FontWeight::MEDIUM)
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
    }

    /// "Clipboard", Pseudonymize and Restore (or one Unlock button for a
    /// locked case), then the quiet source line with its ▾.
    fn clipboard_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let enabled = self.content.source.is_some();
        let buttons = if enabled && self.clipboard_locked(cx) {
            div().flex().flex_row().child(self.clipboard_button(
                "shelf-unlock",
                "Unlock cases in mdoc…",
                true,
                cx,
                |this, window, cx| this.clipboard(ClipboardRow::Restore, window, cx),
            ))
        } else {
            div()
                .flex()
                .flex_row()
                .gap(px(6.))
                .child(self.clipboard_button(
                    "shelf-pseudonymize",
                    "Pseudonymize",
                    enabled,
                    cx,
                    |this, window, cx| this.clipboard(ClipboardRow::Pseudonymize, window, cx),
                ))
                .child(self.clipboard_button(
                    "shelf-restore",
                    "Restore",
                    enabled,
                    cx,
                    |this, window, cx| this.clipboard(ClipboardRow::Restore, window, cx),
                ))
        };
        let line = self
            .source_line()
            .unwrap_or_else(|| "Names appear once a document is pseudonymized".into());
        let source = div()
            .h(px(SOURCE_LINE))
            .px(px(4.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .text_size(px(11.))
            .text_color(p.header_muted)
            .child(
                div()
                    .id("shelf-source-line")
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .when(self.result.is_some(), |v| v.text_color(p.header_fg))
                    .child(line),
            )
            .when(self.content.sources.len() > 1, |v| {
                v.child(
                    ui::control("shelf-pick", "▾", theme, true)
                        .track_focus(&self.pick_button)
                        .aria_label("Choose whose names to use")
                        .px(px(4.))
                        .py_0()
                        .text_size(px(11.))
                        .text_color(p.header_muted)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.toggle_picker(window, cx)),
                        ),
                )
            });
        div()
            .h(px(CLIPBOARD))
            .px(px(4.))
            .flex()
            .flex_col()
            .gap(px(SECTION_GAP))
            .child(
                div()
                    .h(px(CLIPBOARD_HEADING))
                    .flex()
                    .items_end()
                    .px(px(4.))
                    .text_size(px(11.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(p.header_muted)
                    .child("Clipboard"),
            )
            .child(buttons)
            .child(source)
            .into_any_element()
    }

    fn picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(px(11.))
                    .text_color(p.header_muted)
                    .child("Clipboard names from"),
            )
            .children(self.content.sources.iter().enumerate().map(|(i, source)| {
                let chosen = self.content.source.as_ref() == Some(source);
                let picked = source.clone();
                ui::control(("shelf-source", i), source.name().to_string(), theme, true)
                    .track_focus(&self.picker_rows[i])
                    .w_full()
                    .h(px(ACTION_ROW))
                    .flex()
                    .items_center()
                    .role(gpui::Role::RadioButton)
                    .aria_toggled(if chosen {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .when(chosen, |v| v.bg(theme.sidebar_selected()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose(picked.clone(), window, cx)
                    }))
            }))
            .into_any_element()
    }

    /// Open mdoc at the left; Settings and Quit at the right.
    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        div()
            .h(px(FOOTER))
            .flex()
            .flex_row()
            .items_center()
            .child(
                self.action_row("shelf-open-mdoc", "Open mdoc", cx, |this, window, cx| {
                    this.in_mdoc(window, cx, |workspace, window, cx| {
                        workspace.home_from_shelf(window, cx)
                    })
                }),
            )
            .child(div().flex_1())
            .child(
                ui::header_icon(
                    "shelf-settings",
                    "Settings",
                    ui::Icon::Settings,
                    theme,
                    false,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.in_mdoc(window, cx, |workspace, window, cx| {
                        workspace.settings_from_shelf(window, cx)
                    })
                })),
            )
            .child(
                self.action_row("shelf-quit", "Quit", cx, |this, window, cx| {
                    this.close(window, cx);
                    // The quit flow asks about unsaved work as ⌘Q does (ADR 0071).
                    cx.defer(|cx| cx.dispatch_action(&Quit));
                }),
            )
            .into_any_element()
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

impl Render for ShelfView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let list: AnyElement = if self.picking {
            self.picker(cx)
        } else if self.content.documents.is_empty() {
            div()
                .h(px(EMPTY))
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.header_muted)
                .child("Pseudonymized documents appear here")
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(ROW_GAP))
                .children((0..self.content.documents.len()).map(|i| self.document_row(i, cx)))
                .when(self.content.shows_hint(), |v| v.child(self.hint()))
                .into_any_element()
        };
        let update = self.content.update.clone();
        div()
            .id("shelf")
            .key_context("Shelf")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &CloseShelf, window, cx| {
                if this.picking {
                    this.close_picker(window, cx);
                } else {
                    this.close(window, cx)
                }
            }))
            // Tab reaches every control: rows, Copy for AI, Restart, the
            // clipboard buttons and the ▾, Open mdoc, Settings and Quit.
            .on_action(
                cx.listener(|_, _: &ui::NextControl, window, cx| {
                    ui::cycle(window, cx, None, false)
                }),
            )
            .on_action(cx.listener(|_, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, None, true)
            }))
            .on_action(cx.listener(|this, _: &ShelfUp, window, cx| this.move_focus(-1, window, cx)))
            .on_action(
                cx.listener(|this, _: &ShelfDown, window, cx| this.move_focus(1, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShelfCopyForAi, window, cx| {
                if let Some(index) = this.focused_row(window) {
                    this.copy(index, window, cx)
                }
            }))
            .on_action(cx.listener(|this, row: &ShelfCopyRow, window, cx| {
                if !this.picking && row.0 >= 1 {
                    this.copy(row.0 - 1, window, cx)
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.press = None),
            )
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .p(px(PADDING))
            .bg(surface(theme))
            .text_color(p.header_fg)
            .text_size(px(13.))
            .child(list)
            .child(self.divider())
            .when_some(update, |v, version| {
                v.child(self.action_row(
                    "shelf-restart",
                    format!("Restart to update to {version}"),
                    cx,
                    |this, window, cx| {
                        this.in_mdoc(window, cx, |workspace, window, cx| {
                            workspace.restart_from_shelf(window, cx)
                        })
                    },
                ))
            })
            .child(self.clipboard_section(cx))
            .child(self.divider())
            .child(self.footer(cx))
    }
}
