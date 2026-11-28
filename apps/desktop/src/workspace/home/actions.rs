//! Home's actions: showing Home, what its drop zone and strip read, a drag
//! over it, and pasting text as a new document.

use super::text;
use crate::document::DocumentView;
use crate::workspace::Tab;
use crate::workspace::Workspace;
use crate::workspace::home;
use gpui::App;
use gpui::Context;
use gpui::Entity;
use gpui::Window;
use mdoc_import::import;
use mdoc_vault::DocumentSummary;
use std::collections::HashMap;

impl Workspace {
    /// What's new: the release notes of the version just installed.
    pub(super) fn open_release_notes(version: &str, cx: &mut App) {
        cx.open_url(&crate::app::updater::release_url(version));
    }

    /// The What's new strip's ×.
    pub(super) fn dismiss_whats_new(cx: &mut App) {
        crate::app::updater::dismiss_banner(cx);
    }

    /// The hidden blank tab under Home: never edited, never listed.
    pub(in crate::workspace) fn is_placeholder(&self, tab: &Tab, cx: &App) -> bool {
        tab.record().blank_disposable
            && tab.view().is_none_or(|view| {
                let view = view.read(cx);
                view.lifecycle.blank_disposable()
                    && view.preview.source().is_none()
                    && view.editor.read(cx).text().is_empty()
            })
    }

    pub(in crate::workspace) fn showing_home(&self, cx: &App) -> bool {
        self.project_ui.page.is_none()
            && self.reply_page.is_none()
            && (self.tabs.home()
                || self
                    .tabs
                    .active_tab()
                    .is_some_and(|tab| self.is_placeholder(tab, cx)))
    }

    pub(in crate::workspace) fn showing_overview(&self, cx: &App) -> bool {
        self.project_ui.page.is_some() || self.reply_page.is_some() || self.showing_home(cx)
    }

    pub(in crate::workspace) fn show_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_home_as(
            crate::workspace::sidebar::navigation::Arrival::Visit,
            window,
            cx,
        );
    }

    /// Show Home, recording it in Back/Forward as `arrival` says.
    pub(in crate::workspace) fn show_home_as(
        &mut self,
        arrival: crate::workspace::sidebar::navigation::Arrival,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(old) = self.active_view() {
            old.update(cx, |view, _| view.pii.dismiss_popup());
        }
        self.reply_page = None;
        self.project_ui.page = None;
        self.end_rename(true, cx);
        self.tabs.show_home();
        self.note_destination(arrival, cx);
        self.drop_if_empty_new(self.tabs.active(), window, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Each live view's proposals left, by its Vault document's id.
    pub(super) fn live_proposals(&self, cx: &App) -> HashMap<String, usize> {
        (self.tabs.iter())
            .filter_map(|tab| {
                let view = tab.view()?.read(cx);
                let document = self.live_document(tab, cx)?;
                Some((document.as_str().to_string(), view.pii.review().remaining()))
            })
            .collect()
    }

    /// The Continue strip as it stands, with the summaries it was read
    /// from; `None` while the Vault is closed.
    pub(in crate::workspace) fn home_strip_now(
        &self,
        cx: &App,
    ) -> Option<(home::Strip, Vec<DocumentSummary>)> {
        let vault = self.vault.vault()?;
        let documents = vault.documents();
        let strip = home::strip(&documents, &vault.replies(), &self.live_proposals(cx));
        Some((strip, documents))
    }

    /// A file drag anywhere over the window while Home shows.
    pub(in crate::workspace) fn home_drag(
        &mut self,
        event: &gpui::DragMoveEvent<gpui::ExternalPaths>,
        cx: &mut Context<Self>,
    ) {
        if !self.showing_overview(cx) {
            return;
        }
        let over = event.bounds.contains(&event.event.position);
        let count = over.then(|| {
            event
                .drag(cx)
                .paths()
                .iter()
                .filter(|path| import::droppable(path))
                .count()
        });
        if self.shell.drop != count {
            if self.shell.drop.is_none() {
                self.shell.drop_generation += 1;
            }
            self.shell.drop = count;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn paste_as_new_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.intake_refused(window, cx) {
            return;
        }
        let item = cx.read_from_clipboard();
        // A copied file carries its path as text; the editor's Paste skips it.
        let has_files = item.as_ref().is_some_and(|item| {
            item.entries()
                .iter()
                .any(|entry| matches!(entry, gpui::ClipboardEntry::ExternalPaths(_)))
        });
        let Some(text) = item
            .and_then(|item| item.text())
            .filter(|text| !has_files && !text.trim().is_empty())
        else {
            self.notices.post(text::NO_TEXT_TO_PASTE);
            cx.notify();
            return;
        };
        // Straight into the new buffer: a dispatched Paste would arrive before
        // the new editor holds focus and reach nothing (issue #300).
        let id = self.new_tab(window, cx);
        if let Some(view) = self.tabs.get(id).and_then(|tab| tab.view()).cloned() {
            view.update(cx, |view, cx| {
                // An ordinary document, not a new one.
                view.session.pasted = true;
                view.editor
                    .update(cx, |editor, cx| editor.insert_pasted(text, window, cx))
            });
        }
    }

    /// Home's clipboard actions belong to a document; with none open, say so.
    pub(in crate::workspace) fn clipboard_document(
        &mut self,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<DocumentView>> {
        let real = self
            .tabs
            .active_tab()
            .is_some_and(|tab| !self.is_placeholder(tab, cx));
        if !real {
            // The notice opens even over an earlier one.
            let effect = self.notices.post_now(text::needs_a_document(action));
            self.carry_out_notices(effect);
            cx.notify();
            return None;
        }
        // Leave Home so document-level feedback and any error are visible.
        if self.showing_overview(cx) {
            self.activate(self.tabs.active(), window, cx);
        }
        self.active_view()
    }

    /// Whether the Vault is sealed with another key, so Home's drop zone asks
    /// for the recovery key.
    pub(super) fn home_needs_recovery_key(&self) -> bool {
        self.vault.wrong_key_root().is_some()
    }

    /// Why nothing can be added this run, while the Vault is unavailable.
    pub(super) fn home_unavailable(&self) -> Option<&str> {
        self.vault.unavailable()
    }

    /// The pointer moved onto or off Home's local note.
    pub(super) fn hover_local_note(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.shell.local_note_hover != hovered {
            self.shell.local_note_hover = hovered;
            cx.notify();
        }
    }
}
