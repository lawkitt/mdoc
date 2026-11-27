//! What the shelf's rows carry out: Copy for AI and a drag out, each on a
//! row checked again as its text leaves, the `.md` file a drag carries,
//! a click's open and a refused row's step for the host, and ↑↓.
use super::{DRAG_DISTANCE, ROWS_DRAG, ShelfRows, ShelfRowsEvent, row};
use crate::shelf::{ShelfDocument, ShelfExports};
#[cfg(all(target_os = "macos", not(test)))]
use crate::ui::style::Theme;
use gpui::{
    App, ClipboardItem, Context, Global, MouseButton, MouseDownEvent, MouseMoveEvent, Window, px,
};

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

impl ShelfRows {
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

    pub(super) fn mouse_down(&mut self, index: usize, event: &MouseDownEvent) {
        self.press = self
            .documents
            .get(index)
            .filter(|d| ROWS_DRAG && d.readiness.is_ready())
            .map(|_| (index, event.position));
    }

    pub(super) fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let Some(key) = self.documents.get(index).map(|d| d.key.clone()) else {
            return;
        };
        // Its file's neutral name, asked only now it leaves (#1208).
        let name = (self.workspace.read(cx).ok())
            .and_then(|workspace| workspace.shelf_share_name(&key, cx));
        let document = &mut self.documents[index];
        document.set_share_name(name);
        let document = &self.documents[index];
        let card = document.drag_card();
        let Some(path) = export(document, cx) else {
            return;
        };
        self.dropped_here = false;
        self.dragging = true;
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
        self.dragging = false;
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

    /// A click on row `index`: a press that did not become a drag ends,
    /// and the host opens the row in mdoc.
    pub(super) fn open(&mut self, index: usize, cx: &mut Context<Self>) {
        self.press = None;
        cx.emit(ShelfRowsEvent::Open(index));
    }
}
