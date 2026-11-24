//! The shelf's view: the borderless popup under the menu bar
//! item. The shelf's rows ([`ShelfRows`]): pseudonymized documents as small
//! previews to open, drag out or Copy for AI; Restart to update; the
//! Clipboard section with Pseudonymize and Restore for one source; and one
//! footer row: Open mdoc, Settings and Quit. It closes on a click outside
//! (the panel loses key status), Esc, a Copy for AI after its "Copied", or
//! an action that brings mdoc forward.

mod note;
mod rows;

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
pub(crate) use rows::{ShelfRows, ShelfRowsEvent, clear_exports};

use crate::{
    menu_bar,
    shelf::{self, ShelfDocument, ShelfKey},
    style::Theme,
    ui,
    workspace::{ClipboardNames, ClipboardSource, Quit, Workspace},
};
use gpui::{
    AnyElement, App, ClipboardItem, Context, Entity, ExternalPaths, FocusHandle, Focusable, Global,
    Hsla, KeyBinding, Render, SharedString, Subscription, Task, Window, WindowHandle, actions, div,
    prelude::*, px,
};
use mdoc_pii::restore::RestoreMap;
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
    let bindings = crate::shortcuts::bindings(crate::shortcuts::Owner::Shelf, cx);
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
    #[cfg(test)]
    pub(crate) fn asked(&self) -> f32 {
        self.asked.get()
    }

    fn measured(&self, height: f32, window: &mut Window, cx: &mut App) {
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
        self.fit = Some(Rc::new(Fit {
            max,
            asked: Cell::new(height),
        }));
    }

    /// The popup's fitting, for tests.
    #[cfg(test)]
    pub(crate) fn fit(&self) -> Option<Rc<Fit>> {
        self.fit.clone()
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
        let Some(document) = self.rows.read(cx).documents().get(index) else {
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
        let Some(document) = self.rows.read(cx).documents().get(index) else {
            return;
        };
        let source = match (&document.project, &document.key) {
            (Some(project), _) => project_source(&self.sources, &project.id),
            (None, ShelfKey::Tab(id)) => self
                .sources
                .iter()
                .find(|s| matches!(s, ClipboardSource::Tab { id: tab, .. } if tab == id))
                .cloned(),
            // A stored Unfiled document is no clipboard source.
            (None, ShelfKey::Document(_)) => None,
        };
        if let Some(source) = source {
            self.pick(source, cx);
        }
    }

    /// Copy for AI on row `index`, a click or a key: [`ShelfRows::copy`].
    pub(crate) fn copy(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.rows
            .update(cx, |rows, cx| rows.copy(index, window, cx));
    }

    /// Row `index` was copied for AI: "Copied" shows, then the shelf closes
    /// after [`COPIED_FOR`].
    fn after_copy(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.used(index, cx);
        self.closing = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update_in(cx, |this, window, cx| this.close(window, cx));
        }));
    }

    /// Drag row `index` out, as a press and move does: [`ShelfRows::drag`].
    #[cfg(test)]
    pub(crate) fn drag(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.rows
            .update(cx, |rows, cx| rows.drag(index, window, cx));
    }

    /// The running drag was released over the shelf itself.
    pub(crate) fn dropped_on_shelf(&mut self, cx: &mut Context<Self>) {
        self.rows.update(cx, |rows, _| rows.dropped_on_shelf());
    }

    /// A drag ended, as AppKit reports it: [`ShelfRows::drag_ended`].
    #[cfg(test)]
    pub(crate) fn drag_ended(&mut self, dropped: bool, cx: &mut Context<Self>) {
        self.rows
            .update(cx, |rows, cx| rows.drag_ended(dropped, cx));
    }

    fn focused_row(&self, window: &Window, cx: &App) -> Option<usize> {
        if self.picking {
            return None;
        }
        self.rows.read(cx).focused_row(window)
    }

    /// ↑↓ move within the list that shows: the documents, or the picker.
    fn move_focus(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.picking {
            self.rows
                .update(cx, |rows, cx| rows.move_focus(by, window, cx));
            return;
        }
        let targets = &self.picker_rows;
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
            .sources
            .iter()
            .position(|source| Some(source) == self.source.as_ref())
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

    fn pick(&mut self, source: ClipboardSource, cx: &mut Context<Self>) {
        if self.source.as_ref() != Some(&source) {
            self.result = None;
        }
        cx.set_global(PickedSource(Some(source.clone())));
        self.source = Some(source);
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
        let Some(source) = self.source.clone() else {
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
                        cx.write_to_clipboard(crate::restored_clipboard::item(
                            restored.text.clone(),
                        ));
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
            self.source
                .as_ref()
                .map(|source| format!("Names from {}", source.name()))
        })
    }

    pub(crate) fn clipboard_locked(&self, cx: &App) -> bool {
        let Some(source) = &self.source else {
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
    /// locked Vault), then the quiet source line with its ▾.
    fn clipboard_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let enabled = self.source.is_some();
        let buttons = if enabled && self.clipboard_locked(cx) {
            div().flex().flex_row().child(self.clipboard_button(
                "shelf-unlock",
                "Unlock in mdoc…",
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
            .when(self.sources.len() > 1, |v| {
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
                    .h(px(PICKER_HEADING))
                    .px_2()
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(p.header_muted)
                    .child("Clipboard names from"),
            )
            .children(self.sources.iter().enumerate().map(|(i, source)| {
                let chosen = self.source.as_ref() == Some(source);
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
                    // The quit flow asks about unsaved work as ⌘Q does.
                    cx.defer(|cx| cx.dispatch_action(&Quit));
                }),
            )
            .into_any_element()
    }
}

impl Render for ShelfView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let list: AnyElement = if self.picking {
            self.picker(cx)
        } else {
            self.rows.clone().into_any_element()
        };
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
                if let Some(index) = this.focused_row(window, cx) {
                    this.copy(index, window, cx)
                }
            }))
            .on_action(cx.listener(|this, row: &ShelfCopyRow, window, cx| {
                if !this.picking && row.0 >= 1 {
                    this.copy(row.0 - 1, window, cx)
                }
            }))
            // A drag released over the shelf lands here, not in another app.
            .on_drop(cx.listener(|this, _: &ExternalPaths, _, cx| this.dropped_on_shelf(cx)))
            .size_full()
            .overflow_y_scroll()
            .bg(surface(theme))
            .text_color(p.header_fg)
            .text_size(px(13.))
            .child(self.body(list, cx))
    }
}

impl ShelfView {
    /// Everything the shelf shows, in one column the popup measures.
    fn body(&self, list: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let update = self.update.clone();
        div()
            .relative()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .p(px(PADDING))
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
            .when_some(self.fit.clone(), |body, fit| {
                body.child(
                    gpui::canvas(
                        move |bounds, window, cx| {
                            fit.measured(bounds.size.height.as_f32(), window, cx)
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            })
            .into_any_element()
    }
}
