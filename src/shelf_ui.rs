//! The shelf's view (ADR 0084): the borderless popup under the menu bar
//! item. Pseudonymized documents as small previews to open, drag out or
//! Copy for AI; then "Show all in mdoc", Restart to update, the clipboard
//! rows for the current case, and Quit. It closes on a click outside (the
//! panel loses key status), Esc, or an action that brings mdoc forward.
//!
//! Only ready documents copy or drag; previews and exports hold
//! pseudonymized text only (the menu bar shows in screen shares).

use crate::{
    menu_bar,
    shelf::{self, Readiness, ShelfDocument, ShelfExports, ShelfKey},
    style::{self, Theme},
    ui,
    workspace::{ClipboardNames, ClipboardSource, Quit, Workspace},
};
use gpui::{
    AnyElement, App, ClipboardItem, Context, FocusHandle, Focusable, Global, KeyBinding,
    MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, Render, SharedString, Window,
    WindowHandle, actions, div, prelude::*, px,
};
use mdoc_pii::restore::RestoreMap;
use std::{cell::Cell, ops::Range, rc::Rc, time::SystemTime};

actions!(shelf, [CloseShelf, ShelfUp, ShelfDown, ShelfCopyForAi]);

/// Keys inside the shelf: ↑↓ and Esc are plumbing (moving inside a list,
/// ADR 0073); ⌘C is the table's Copy for AI row; Enter activates the
/// focused row like any control.
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseShelf, Some("Shelf")),
        KeyBinding::new("up", ShelfUp, Some("Shelf")),
        KeyBinding::new("down", ShelfDown, Some("Shelf")),
    ]);
    cx.bind_keys(crate::shortcuts::bindings(crate::shortcuts::Owner::Shelf));
}

const PADDING: f32 = 6.;
const ROW_GAP: f32 = 2.;
const READY_ROW: f32 = 104.;
const WAITING_ROW: f32 = 50.;
const ACTION_ROW: f32 = 30.;
const EMPTY: f32 = 44.;
const DIVIDER: f32 = 9.;
/// Titles longer than this many characters are cut in the middle.
const TITLE_CHARS: usize = 42;
/// A drag starts after the pointer moves this far with the button down.
const DRAG_DISTANCE: f32 = 4.;

/// What the shelf shows, read from the workspace when it opens.
pub(crate) struct ShelfContent {
    pub documents: Vec<ShelfDocument>,
    pub sources: Vec<ClipboardSource>,
    pub source: Option<ClipboardSource>,
    pub update: Option<String>,
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
        }
    }

    /// The shelf's height for this content, before the screen limits it.
    pub(crate) fn height(&self) -> f32 {
        let rows: f32 = if self.documents.is_empty() {
            EMPTY
        } else {
            self.documents
                .iter()
                .map(|d| {
                    ROW_GAP
                        + if d.readiness.is_ready() {
                            READY_ROW
                        } else {
                            WAITING_ROW
                        }
                })
                .sum::<f32>()
                + ACTION_ROW
        };
        let update = if self.update.is_some() {
            ACTION_ROW
        } else {
            0.
        };
        2. * PADDING + rows + 2. * DIVIDER + update + 3. * ACTION_ROW
    }
}

/// What a clipboard row last did, shown on the row.
#[derive(Clone, Copy, PartialEq, Eq)]
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
    /// The row whose Copy for AI just ran.
    copied: Option<usize>,
    /// The case picker replaces the list while open.
    picking: bool,
    /// The last clipboard action's result, on its row.
    result: Option<(ClipboardRow, String)>,
    /// The last drag's payload, for tests (no native drag runs there).
    #[cfg(test)]
    pub(crate) dragged: Option<(String, std::path::PathBuf)>,
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
        let focus = cx.focus_handle();
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
            picking: false,
            result: None,
            #[cfg(test)]
            dragged: None,
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
        cx.defer(move |cx| menu_bar::reveal(cx, then));
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

    /// The row is the document used last: the clipboard rows follow it.
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

    pub(crate) fn copy(&mut self, index: usize, cx: &mut Context<Self>) {
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
        cx.notify();
    }

    fn focused_row(&self, window: &Window) -> Option<usize> {
        self.rows.iter().position(|row| row.is_focused(window))
    }

    fn move_focus(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let at = self.focused_row(window).map_or(0, |at| {
            (at as isize + by).clamp(0, self.rows.len() as isize - 1) as usize
        });
        window.focus(&self.rows[at], cx);
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
    /// on one pasteboard item.
    pub(crate) fn drag(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.content.documents.get(index) else {
            return;
        };
        let Some(text) = document.text_for_ai().map(str::to_owned) else {
            return;
        };
        let Some(path) = export(document, cx) else {
            return;
        };
        #[cfg(all(target_os = "macos", not(test)))]
        gpui_macos::start_text_and_file_drag(window, &text, &path);
        #[cfg(not(all(target_os = "macos", not(test))))]
        let _ = window;
        #[cfg(test)]
        {
            self.dragged = Some((text, path));
        }
        #[cfg(not(any(target_os = "macos", test)))]
        let _ = (text, path);
        self.used(index, cx);
    }

    fn pick(&mut self, source: ClipboardSource, cx: &mut Context<Self>) {
        if self.content.source.as_ref() != Some(&source) {
            self.result = None;
        }
        cx.set_global(PickedSource(Some(source.clone())));
        self.content.source = Some(source);
        self.picking = false;
        cx.notify();
    }

    /// Run a clipboard row with the current case's names.
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
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.result = Some((row, "The clipboard has no text".into()));
            cx.notify();
            return;
        };
        let summary = match row {
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
        };
        self.result = Some((row, summary));
        cx.notify();
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

/// `title`, cut in the middle when long (ADR 0084).
fn middle(title: &str) -> String {
    let count = title.chars().count();
    if count <= TITLE_CHARS {
        return title.to_string();
    }
    let head = (TITLE_CHARS - 1) / 2;
    let tail = TITLE_CHARS - 1 - head;
    let start: String = title.chars().take(head).collect();
    let end: String = title.chars().skip(count - tail).collect();
    format!("{start}…{end}")
}

/// "edited 14:32" today, "edited 12 Oct" before.
fn edited(time: SystemTime) -> String {
    let time = chrono::DateTime::<chrono::Local>::from(time);
    let today = chrono::Local::now().date_naive();
    if time.date_naive() == today {
        format!("edited {}", time.format("%H:%M"))
    } else {
        format!("edited {}", time.format("%-d %b"))
    }
}

/// "1,240 words".
fn words(count: usize) -> String {
    let digits = count.to_string();
    let mut grouped = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{grouped} {}", if count == 1 { "word" } else { "words" })
}

/// What VoiceOver reads for a row: "<title>, <case>, ready".
pub(crate) fn row_label(document: &ShelfDocument) -> String {
    let mut label = document.title.clone();
    if let Some(case) = &document.case {
        label.push_str(", ");
        label.push_str(&case.name);
    }
    label.push_str(", ");
    label.push_str(&document.readiness.label());
    label
}

/// A preview line with its aliases as chips, in the editor's colours.
fn preview_line(text: &str, aliases: &[Range<usize>], theme: Theme) -> AnyElement {
    let p = theme.pdf_style();
    let mut line = div()
        .flex()
        .flex_row()
        .overflow_hidden()
        .whitespace_nowrap()
        .h(px(18.))
        .items_center()
        .text_size(px(12.))
        .text_color(p.header_muted);
    let mut at = 0;
    for range in aliases {
        if range.start > at {
            line = line.child(SharedString::from(text[at..range.start].to_string()));
        }
        line = line.child(
            div()
                .px(px(3.))
                .rounded(px(3.))
                .bg(theme.applied().opacity(0.14))
                .text_color(theme.applied_text())
                .child(SharedString::from(text[range.clone()].to_string())),
        );
        at = range.end;
    }
    if at < text.len() {
        line = line.child(SharedString::from(text[at..].to_string()));
    }
    line.into_any_element()
}

impl ShelfView {
    fn document_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let document = &self.content.documents[index];
        let ready = document.readiness.is_ready();
        let dot = document
            .case
            .as_ref()
            .map_or(p.border, |case| style::case_color(case.hue(), theme));
        let state = match document.readiness {
            Readiness::Ready => format!("{} · {}", edited(document.edited), words(document.words)),
            other => other.label(),
        };
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(div().size(px(8.)).rounded_full().flex_shrink_0().bg(dot))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(middle(&document.title)),
            )
            .when(ready, |header| {
                let copied = self.copied == Some(index);
                header.child(
                    ui::control(
                        ("shelf-copy", index),
                        if copied { "Copied" } else { "Copy for AI" },
                        theme,
                        true,
                    )
                    .aria_label("Copy for AI")
                    .text_size(px(12.))
                    .py(px(2.))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.copy(index, cx);
                    })),
                )
            });
        let body = div()
            .pl(px(16.))
            .flex()
            .flex_col()
            .children(
                document
                    .preview
                    .iter()
                    .map(|line| preview_line(&line.text, &line.aliases, theme)),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(if ready {
                        p.header_muted
                    } else {
                        theme.proposed()
                    })
                    .child(state),
            );
        ui::focus_ring(("shelf-row", index), theme)
            .track_focus(&self.rows[index])
            .role(gpui::Role::Button)
            .aria_label(row_label(document))
            .key_context("UiControl")
            .w_full()
            .h(px(if ready { READY_ROW } else { WAITING_ROW }))
            .px_2()
            .py(px(6.))
            .rounded_md()
            .flex()
            .flex_col()
            .gap(px(2.))
            .cursor_pointer()
            .hover(move |s| s.bg(p.placeholder_bg))
            .child(header)
            .child(body)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    this.mouse_down(index, event)
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.mouse_move(event, window, cx)
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.press = None;
                this.open(index, window, cx)
            }))
            .into_any_element()
    }

    fn action_row(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> ui::Control {
        let theme = self.theme.get();
        ui::control(id, label, theme, true)
            .w_full()
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
            .child(div().w_full().h(px(1.)).bg(p.border))
            .into_any_element()
    }

    fn clipboard_row(&self, row: ClipboardRow, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let (id, action) = match row {
            ClipboardRow::Pseudonymize => ("shelf-pseudonymize", "Pseudonymize Clipboard"),
            ClipboardRow::Restore => ("shelf-restore", "Restore Clipboard"),
        };
        if self.content.source.is_some() && self.clipboard_locked(cx) {
            return self
                .action_row(
                    id,
                    "Unlock cases in mdoc…",
                    cx,
                    move |this, window, cx| this.clipboard(row, window, cx),
                )
                .into_any_element();
        }
        let result = self
            .result
            .as_ref()
            .filter(|(done, _)| *done == row)
            .map(|(_, text)| text.clone());
        let enabled = self.content.source.is_some();
        let label = match (&result, &self.content.source) {
            (Some(result), _) => result.clone(),
            (None, Some(source)) => format!("{action} · “{}”", source.name()),
            (None, None) => action.to_string(),
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                ui::control(id, label, theme, enabled)
                    .aria_label(action)
                    .flex_1()
                    .h(px(ACTION_ROW))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.clipboard(row, window, cx)),
                    ),
            )
            .when(self.content.sources.len() > 1, |v| {
                v.child(
                    ui::control(("shelf-pick", row as usize), "▾", theme, true)
                        .aria_label("Choose the case")
                        .text_color(p.header_muted)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.picking = !this.picking;
                            cx.notify();
                        })),
                )
            })
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
                    .on_click(cx.listener(move |this, _, _, cx| this.pick(picked.clone(), cx)))
            }))
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
                .child(self.action_row(
                    "shelf-show-all",
                    "Show all in mdoc",
                    cx,
                    |this, window, cx| {
                        this.in_mdoc(window, cx, |workspace, window, cx| {
                            workspace.show_all_from_shelf(window, cx)
                        })
                    },
                ))
                .into_any_element()
        };
        let update = self.content.update.clone();
        div()
            .id("shelf")
            .key_context("Shelf")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &CloseShelf, window, cx| {
                if this.picking {
                    this.picking = false;
                    cx.notify();
                } else {
                    this.close(window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &ShelfUp, window, cx| this.move_focus(-1, window, cx)))
            .on_action(
                cx.listener(|this, _: &ShelfDown, window, cx| this.move_focus(1, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShelfCopyForAi, window, cx| {
                if let Some(index) = this.focused_row(window) {
                    this.copy(index, cx)
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
            .bg(p.bg)
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
            // A locked case's clipboard rows are one: "Unlock cases in mdoc…".
            .child(self.clipboard_row(ClipboardRow::Pseudonymize, cx))
            .when(!self.clipboard_locked(cx), |v| {
                v.child(self.clipboard_row(ClipboardRow::Restore, cx))
            })
            .child(self.divider())
            .child(
                self.action_row("shelf-quit", "Quit mdoc", cx, |this, window, cx| {
                    this.close(window, cx);
                    // The quit flow asks about unsaved work as ⌘Q does (ADR 0071).
                    cx.defer(|cx| cx.dispatch_action(&Quit));
                }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_titles_are_cut_in_the_middle() {
        assert_eq!(middle("PERSON_1 settlement.md"), "PERSON_1 settlement.md");
        let long = "PERSON_1 v ORG_1 — the very long settlement agreement draft 3.md";
        let cut = middle(long);
        assert_eq!(cut.chars().count(), TITLE_CHARS);
        assert!(cut.starts_with("PERSON_1 v ORG_1") && cut.ends_with("draft 3.md"));
        assert!(cut.contains('…'));
    }

    #[test]
    fn word_counts_group_thousands() {
        assert_eq!(words(1), "1 word");
        assert_eq!(words(1240), "1,240 words");
        assert_eq!(words(1_000_000), "1,000,000 words");
    }
}
