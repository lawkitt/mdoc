//! What the shelf's popup carries out: a row or its step opened in mdoc,
//! Copy for AI and drags, ↑↓ and the source picker, Pseudonymize and
//! Restore on the clipboard, and closing.
use super::{COPIED_FOR, ClipboardRow, PickedSource, RESULT_FOR, ShelfView, project_source};
use crate::{
    shelf::{self, ShelfKey, menu_bar, text},
    workspace::{ClipboardNames, ClipboardSource, Quit, Workspace},
};
use gpui::{App, ClipboardItem, Context, Window};
use mdoc_pii::restore::RestoreMap;

impl ShelfView {
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
            workspace.open_from_shelf(&key, ready, None, window, cx)
        });
    }

    /// A refused row's step, in mdoc: "Replace N and copy" in its tab,
    /// "Review" or "Find names" in its document once it has opened.
    pub(crate) fn run_step(
        &mut self,
        index: usize,
        step: crate::review::refusal::Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.rows.read(cx).documents().get(index) else {
            return;
        };
        let key = document.key.clone();
        self.in_mdoc(window, cx, move |workspace, window, cx| {
            workspace.open_from_shelf(&key, false, Some(step), window, cx)
        });
    }

    /// The row is the document used last: the clipboard source follows it.
    pub(super) fn used(&mut self, index: usize, cx: &mut Context<Self>) {
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
    pub(super) fn after_copy(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
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

    /// ↑↓ move within the list that shows: the documents, or the picker.
    pub(super) fn move_focus(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn choose(
        &mut self,
        source: ClipboardSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            None => text::NO_TEXT.to_string(),
            Some(text) => match row {
                ClipboardRow::Restore => {
                    let restored = shelf::restore_clipboard(&text, names);
                    if restored.text != text {
                        cx.write_to_clipboard(crate::review::restored_clipboard::item(
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
                    text::names_replaced(count)
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

    /// Quit, from the footer: the shelf closes and the quit flow runs.
    pub(super) fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
        // The quit flow asks about unsaved work as ⌘Q does.
        cx.defer(|cx| cx.dispatch_action(&Quit));
    }

    /// Esc: the picker closes, or else the shelf.
    pub(super) fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picking {
            self.close_picker(window, cx);
        } else {
            self.close(window, cx)
        }
    }

    /// Open mdoc, from the footer: Home in mdoc's window.
    pub(super) fn open_mdoc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.in_mdoc(window, cx, |workspace, window, cx| {
            workspace.home_from_shelf(window, cx)
        })
    }

    /// The footer's Settings gear: Settings in mdoc's window.
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.in_mdoc(window, cx, |workspace, window, cx| {
            workspace.settings_from_shelf(window, cx)
        })
    }

    /// Restart to update: the ordinary quit, in mdoc's window.
    pub(super) fn restart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.in_mdoc(window, cx, |workspace, window, cx| {
            workspace.restart_from_shelf(window, cx)
        })
    }
}
