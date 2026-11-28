//! What the shelf asks of the window: a row or its step opened in mdoc,
//! Open mdoc, Settings, Unlock in mdoc… and Restart to update, the
//! first-close note's Quit when windows close, and whether the note and the
//! drag hint have shown.
use super::Workspace;
use crate::{review::refusal::Step, shelf::ShelfKey, workspace::sidebar::navigation::Arrival};
use gpui::{Context, Window};
use mdoc_store::document_id::DocumentId;

impl Workspace {
    /// A shelf row's click: the tab comes forward in its current view, a
    /// stored document opens in a tab, and a document that is not ready
    /// shows its first pending proposal once it has loaded. With `step`
    /// (a refused row's button), that step runs instead once it has loaded.
    pub(crate) fn open_from_shelf(
        &mut self,
        key: &ShelfKey,
        ready: bool,
        step: Option<Step>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = match key {
            ShelfKey::Tab(tab) => {
                if !self.tabs.contains(*tab) {
                    return;
                }
                self.activate_as(*tab, Arrival::Visit, window, cx);
                *tab
            }
            ShelfKey::Document(id) => {
                self.open_vault_document(id.clone(), window, cx);
                let document = DocumentId::Vault(id.clone());
                match self.find_document(&document, None, cx) {
                    Some(tab) => tab,
                    None => return,
                }
            }
        };
        if ready && step.is_none() {
            self.focus_active(window, cx);
            return;
        }
        // A tab still loading shows its proposals once it is ready: wait a
        // little for them.
        cx.spawn_in(window, async move |this, cx| {
            for _ in 0..50 {
                let shown = this
                    .update_in(cx, |this, window, cx| {
                        let Some(view) = (this.tabs.get(tab)).and_then(|tab| tab.view().cloned())
                        else {
                            return false;
                        };
                        if !view.read(cx).lifecycle.ready() {
                            return false;
                        }
                        match step {
                            Some(step) => this.run_tab_step(tab, step, window, cx),
                            None => {
                                view.update(cx, |view, cx| view.select_first_proposal(window, cx))
                            }
                        }
                    })
                    .unwrap_or(true);
                if shown {
                    return;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
            }
        })
        .detach();
    }

    /// "Open mdoc": Home lists every document.
    pub(crate) fn home_from_shelf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_home(window, cx);
    }

    /// The shelf's Settings gear.
    pub(crate) fn settings_from_shelf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_settings(window, cx);
    }

    /// The first-close note's "Quit when windows close": the
    /// switch goes off and mdoc quits, as it now would with its last window
    /// closed. The hidden window shows only if the quit asks something; a
    /// quit that cannot start brings it back, since no item is left to.
    pub(crate) fn quit_when_windows_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let saved = self.update_general(|general| general.menu_bar = false, cx);
        crate::shelf::menu_bar::quit_when_windows_close(cx);
        if let Err(error) = saved {
            // Quitting now would bring the item back at the next launch with
            // no note: show the window and say so instead.
            crate::shelf::menu_bar::reveal_if_hidden(window, cx);
            self.notices
                .post(crate::shelf::text::quit_when_windows_close_not_saved(error));
            cx.notify();
            return;
        }
        self.quit(window, cx);
        if !self.quit_flow.quitting() {
            crate::shelf::menu_bar::reveal_if_hidden(window, cx);
        }
    }

    /// Whether the first-close note has shown.
    pub(crate) fn menu_bar_note_shown(&self) -> bool {
        self.general()
            .is_some_and(|general| general.menu_bar_note_shown)
    }

    /// Whether a shelf drag has dropped, retiring its hint.
    pub(crate) fn shelf_drag_hint_shown(&self) -> bool {
        self.general()
            .is_some_and(|general| general.shelf_drag_hint_shown)
    }

    /// "Unlock in mdoc…": the Keychain explainer, in the window.
    pub(crate) fn unlock_from_shelf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.unlock_projects(window, cx);
    }

    /// "Restart to update": the ordinary quit, prompts included.
    pub(crate) fn restart_from_shelf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.restart_to_update(window, cx);
    }
}
