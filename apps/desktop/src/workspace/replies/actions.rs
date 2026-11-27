//! Saved replies' actions: saving a reply from its box or Paste AI Reply,
//! showing, refreshing, copying, promoting and deleting it on the Reply page.
use crate::workspace::replies::{ReplyPage, ReplySave, reply_detail, text};
use crate::workspace::{
    Workspace,
    saving::autosave::Write,
    saving::document_writer::Target,
    sidebar::navigation::{Arrival, Stop},
};
use crate::{review::SavedReplyRow, ui::style};
use gpui::{Context, PromptLevel, Window, prelude::*};
use mdoc_pii::restore::{RestoreMap, RestoredText};
use mdoc_store::document_id::VaultId;
use mdoc_vault::{Reply, ReplyOwner, SharedFrom, VaultError};

impl Workspace {
    /// The reply in tab `tab`'s box settled, or Open reply asked for it:
    /// save it, and show it when `open`. At the head of the queue its
    /// document is written first, names and text, so its aliases bind to
    /// the names the Vault holds (ADR 0130).
    pub(in crate::workspace) fn queue_box_save(
        &mut self,
        tab: u64,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tab_document(tab, cx).is_none() {
            // A pasted document becomes one at its first write; the box's
            // reply is saved with the next change after that.
            if open {
                self.notices.post(text::NOT_IN_MDOC_OPEN.to_string());
                cx.notify();
            }
            return;
        }
        // Only the box's latest save waits; it reads the box when it runs.
        let save = ReplySave::Box { tab, open };
        self.queue_write(Target::Reply(tab), Write::Reply(save), window, cx);
    }

    /// Paste AI Reply in tab `tab`: save `markdown`, its document written
    /// first at the head of the queue, then show it.
    pub(in crate::workspace) fn queue_pasted_reply(
        &mut self,
        tab: u64,
        markdown: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.tab_document(tab, cx) else {
            self.notices.post(text::NOT_IN_MDOC_PASTE.to_string());
            cx.notify();
            return;
        };
        // Its aliases are the document's as it shows them; a move waiting
        // to run would read them in another mapping.
        if self.moving.contains(&document) {
            self.notices.post(text::MOVING.to_string());
            cx.notify();
            return;
        }
        let title = self.tab_view(tab).map_or_else(String::new, |view| {
            let name = view.read(cx).display_name();
            name.strip_suffix(".md").unwrap_or(&name).to_string()
        });
        let save = ReplySave::Pasted {
            tab,
            document,
            title: text::reply_to(title),
            markdown,
        };
        self.next_op += 1;
        let target = Target::Op(self.next_op);
        self.queue_write(target, Write::Reply(save), window, cx);
    }

    /// Queue the reply in tab `tab`'s box when it changed since it was last
    /// saved; it is read when its save runs.
    pub(in crate::workspace) fn queue_box_reply(
        &mut self,
        tab: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.tab_view(tab) else {
            return;
        };
        if view.read(cx).reply.unsaved().is_none()
            // One waiting saves whatever the box holds when it runs.
            || self.document_writer.has(&Target::Reply(tab))
            || self.tab_document(tab, cx).is_none()
            || self.vault.vault().is_none()
        {
            return;
        }
        let save = ReplySave::Box { tab, open: false };
        self.queue_write(Target::Reply(tab), Write::Reply(save), window, cx);
    }

    /// Why tab `tab`'s names are not in the Vault as it shows them, when
    /// they are not: a reply bound now would miss them.
    fn names_unsaved(&self, tab: u64, cx: &Context<Self>) -> Option<&'static str> {
        let view = self.tab_view(tab)?;
        let view = view.read(cx);
        if let Some(link) = view.project.link()
            && self.project_ui.is_unwritten(&link.id)
        {
            return Some(text::PROJECT_NAMES_UNSAVED);
        }
        view.unsaved(cx).then_some(text::DOCUMENT_NAMES_UNSAVED)
    }

    /// Save a reply at the head of the writer's queue, after what its
    /// document shows: its names, then its text.
    pub(in crate::workspace) fn run_reply_save(
        &mut self,
        save: ReplySave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        let tab = match &save {
            ReplySave::Box { tab, .. } | ReplySave::Pasted { tab, .. } => *tab,
        };
        let moving = (self.tab_document(tab, cx)).is_some_and(|d| self.moving.contains(&d));
        if !moving && let Err(error) = self.write_tab_now(tab, &vault, cx) {
            self.unsaved.insert(tab);
            let open = match &save {
                ReplySave::Box { open, .. } => *open,
                ReplySave::Pasted { .. } => true,
            };
            let waits =
                open || self.closing_after_write.contains(&tab) || self.quit_flow.quitting();
            if waits {
                self.reply_failed(Some(tab), text::document_not_saved_first(error), window, cx);
            }
            return;
        }
        let (id, open) = match save {
            ReplySave::Box { tab, open } => {
                let (Some(view), Some(document)) = (self.tab_view(tab), self.tab_document(tab, cx))
                else {
                    return;
                };
                // A move of it waits behind this save: it loads again with
                // its new aliases, and its next autosave queues the box.
                if self.moving.contains(&document) {
                    return;
                }
                let (saved, unsaved, title) = view.read_with(cx, |view, _| {
                    let name = view.display_name();
                    let stem = name.strip_suffix(".md").unwrap_or(&name).to_string();
                    (
                        view.reply.saved_id(),
                        view.reply.unsaved(),
                        text::reply_to(stem),
                    )
                });
                let unready = unsaved.as_ref().and_then(|_| self.names_unsaved(tab, cx));
                let id = match unsaved {
                    None => saved,
                    Some(_) if unready.is_some() => {
                        // Left unsaved for its next save, unless something
                        // waits on it now.
                        if open
                            || self.closing_after_write.contains(&tab)
                            || self.quit_flow.quitting()
                        {
                            let why = unready.unwrap_or_default().to_string();
                            self.reply_failed(Some(tab), why, window, cx);
                        }
                        None
                    }
                    Some((markdown, revision)) => {
                        let from = SharedFrom::Document(document.as_str().to_string());
                        // One fill of the box makes one reply, its id kept
                        // across edits: a document promoted from it still
                        // finds it. Deleted meanwhile, it is saved anew.
                        let updated = match &saved {
                            Some(old) => match vault.update_shared_reply(
                                old,
                                &from,
                                title.clone(),
                                markdown.clone(),
                            ) {
                                Err(VaultError::NotFound { .. }) => None,
                                result => Some(result),
                            },
                            None => None,
                        };
                        let result = updated.unwrap_or_else(|| {
                            vault.save_shared_reply(&from, title, None, markdown)
                        });
                        match result {
                            Ok(reply) => {
                                view.update(cx, |view, _| {
                                    view.reply.mark_saved(&reply.id, revision)
                                });
                                // Edited in place: its page shows it as saved,
                                // which is what Promote takes.
                                if let Some(page) = self.reply_page.as_mut()
                                    && page.id == reply.id
                                {
                                    page.loaded = Ok(reply.clone());
                                    page.made_from = None;
                                    self.refresh_reply_page(cx);
                                }
                                Some(reply.id)
                            }
                            Err(error) => {
                                self.reply_failed(Some(tab), error.to_string(), window, cx);
                                None
                            }
                        }
                    }
                };
                (id, open)
            }
            ReplySave::Pasted {
                tab,
                document,
                title,
                markdown,
            } => {
                let from = SharedFrom::Document(document.as_str().to_string());
                let id = match self.names_unsaved(tab, cx) {
                    Some(why) => {
                        self.reply_failed(None, text::paste_it_again(why), window, cx);
                        None
                    }
                    None => match vault.save_shared_reply(&from, title, None, markdown) {
                        Ok(reply) => Some(reply.id),
                        Err(error) => {
                            self.reply_failed(None, error.to_string(), window, cx);
                            None
                        }
                    },
                };
                (id, true)
            }
        };
        self.refresh_saved_replies(cx);
        if let Some(id) = id
            && open
        {
            self.open_reply(&id, window, cx);
        }
        cx.notify();
    }

    /// A reply could not be saved: say why. A box keeps its text, and a
    /// close or quit waiting for it stops, as for a document write.
    fn reply_failed(
        &mut self,
        tab: Option<u64>,
        why: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notices.post(text::could_not_save(why));
        if let Some(tab) = tab {
            self.closing_after_write.remove(&tab);
        }
        if self.quit_flow.abandon() {
            Self::quit_cancelled(cx);
            crate::shelf::menu_bar::reveal_if_hidden(window, cx);
        }
    }

    /// Show saved reply `id` on the Reply page.
    pub(in crate::workspace) fn open_reply(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_reply_page(id, Arrival::Visit, window, cx);
    }

    pub(in crate::workspace) fn show_reply_page(
        &mut self,
        id: &str,
        arrival: Arrival,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        if let Some(view) = self.active_view() {
            view.update(cx, |view, _| {
                view.active = false;
                view.pii.dismiss_popup();
            });
        }
        self.end_rename(true, cx);
        self.project_ui.list_open = false;
        self.project_ui.page = None;
        let theme = self.theme.get();
        let scroll = gpui::ScrollHandle::new();
        let editor = cx.new(|cx| {
            let mut editor = mdoc_editor::EditorState::new(window, cx);
            editor.set_markdown_style(style::markdown_style(theme), cx);
            editor.set_read_only(true, cx);
            editor.set_autoscroll(scroll.clone());
            // The restored reply holds originals.
            editor.set_copy_metadata(Some(crate::review::restored_clipboard::TAG.into()));
            editor
        });
        let focus = || cx.focus_handle().tab_index(0).tab_stop(true);
        let (copy_focus, next_focus, promote_focus, delete_focus) =
            (focus(), focus(), focus(), focus());
        self.reply_page = Some(ReplyPage {
            id: id.to_string(),
            loaded: vault.reply(id).map_err(|error| error.to_string()),
            editor,
            restored: RestoredText::default(),
            made_from: None,
            next: 0,
            copied: false,
            scroll,
            copy_focus: copy_focus.clone(),
            next_focus,
            promote_focus,
            delete_focus,
        });
        self.refresh_reply_page(cx);
        self.note_destination(arrival, cx);
        window.defer(cx, move |window, cx| window.focus(&copy_focus, cx));
        cx.notify();
    }

    /// What reply `reply` restores with now: each bound identity's
    /// originals in its owner's mapping, the window's copy of a Project
    /// first, so a change not written yet shows too.
    pub(in crate::workspace) fn reply_restore_map(
        &self,
        reply: &Reply,
    ) -> Result<RestoreMap, VaultError> {
        let Some(vault) = self.vault.vault() else {
            return Ok(RestoreMap::new(Vec::new()));
        };
        let project = |id: &str| match self.projects.project(id) {
            Some(project) => Ok(project.mapping.clone()),
            None => vault.project(id).map(|project| project.mapping),
        };
        let mapping = match &reply.owner {
            ReplyOwner::Project(id) => project(id)?,
            ReplyOwner::Document(id) => {
                let document = vault.document(id)?;
                match &document.project {
                    Some(id) => project(id)?,
                    None => document.mapping,
                }
            }
        };
        Ok(mapping.reply_map(&reply.bindings))
    }

    /// Restore the shown reply again when the names it binds changed.
    pub(in crate::workspace) fn refresh_reply_page(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.reply_page.as_ref() else {
            return;
        };
        let Ok(reply) = &page.loaded else {
            return;
        };
        let map = match self.reply_restore_map(reply) {
            Ok(map) => map,
            // Its owner is gone: nothing restores.
            Err(_) => RestoreMap::new(Vec::new()),
        };
        let identities = map.identities().to_vec();
        if page.made_from.as_ref() == Some(&identities) {
            return;
        }
        let restored = map.restore_in_place(&reply.markdown);
        let theme = self.theme.get();
        let marks = crate::review::reply_marks(&restored, theme);
        let page = self.reply_page.as_mut().unwrap();
        let text = restored.text.clone();
        page.editor.update(cx, |editor, cx| {
            if editor.text() != text {
                editor.set_text(text, cx);
            }
            let revision = editor.revision();
            editor.set_annotations(revision, marks, cx);
        });
        page.next = page.next.min(restored.unresolved.len());
        page.restored = restored;
        page.made_from = Some(identities);
        cx.notify();
    }

    /// Copy reply: the restored text, marked as restored.
    pub(in crate::workspace) fn copy_saved_reply(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.reply_page.as_mut() else {
            return;
        };
        if page.loaded.is_err() {
            return;
        }
        cx.write_to_clipboard(crate::review::restored_clipboard::item(
            page.restored.text.clone(),
        ));
        page.copied = true;
        cx.notify();
    }

    /// Next unrecognized: reveal the next unresolved token, wrapping.
    pub(in crate::workspace) fn next_saved_unrecognised(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.reply_page.as_mut() else {
            return;
        };
        let tokens = &page.restored.unresolved;
        if tokens.is_empty() {
            return;
        }
        let at = page.next % tokens.len();
        page.next = at + 1;
        let range = tokens[at].clone();
        let id = crate::review::UNRESOLVED_ID | range.start as u64;
        page.editor.update(cx, |editor, cx| {
            editor.set_active_annotation(Some(id), cx);
            editor.reveal_offset(range.start, cx);
        });
        cx.notify();
    }

    /// The unresolved token Next unrecognized last moved to.
    #[cfg(test)]
    pub(in crate::workspace) fn current_saved_unrecognised(
        &self,
    ) -> Option<std::ops::Range<usize>> {
        let page = self.reply_page.as_ref()?;
        let at = page.next.checked_sub(1)?;
        page.restored.unresolved.get(at).cloned()
    }

    /// Promote to Document: the shown reply, restored, becomes a new
    /// document in its place once the window's Project names are written,
    /// so it restores with the names the page shows; it then opens.
    pub(in crate::workspace) fn promote_saved_reply(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = self.reply_page.as_ref() else {
            return;
        };
        if page.loaded.is_err() || self.vault.vault().is_none() {
            return;
        }
        let id = page.id.clone();
        self.next_op += 1;
        let target = Target::Op(self.next_op);
        self.queue_write(target, Write::Promote(id), window, cx);
    }

    /// Promote reply `id` at the head of the writer's queue and open the
    /// document it made; the reply stays.
    pub(in crate::workspace) fn run_promote(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The companion's Save as document says how it ended: not saved,
        // unless the promotion below succeeds.
        self.ai.reply_not_saved(id);
        crate::shelf::menu_bar::companion_changed(cx);
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        // The page restores with the window's Project: its names are
        // written first, so the document restores the same way there.
        let project = vault.reply(id).ok().and_then(|reply| match reply.owner {
            ReplyOwner::Project(project) => Some(project),
            ReplyOwner::Document(document) => vault.document(&document).ok()?.project,
        });
        if let Some(project) = project {
            // Its open members' names waiting for their fold count too.
            let folded = self.fold_members(&[Some(project.clone())], cx);
            let due = self.project_ui.is_unwritten(&project)
                || self.document_writer.has(&Target::Project(project.clone()));
            let written = folded.and_then(|()| match due {
                true => {
                    (self.write_now(&vault, &[Some(project.clone())])).map_err(|e| e.to_string())
                }
                false => Ok(()),
            });
            if let Err(error) = written {
                self.project_ui.mark_unwritten(&project);
                self.notices.post(text::names_not_saved_for_promote(error));
                cx.notify();
                return;
            }
            self.project_ui.take_unwritten(&project);
        }
        let promoted = vault
            .promote_reply(id)
            .map_err(|error| error.to_string())
            .and_then(|document| {
                VaultId::parse(&document.id).ok_or_else(|| text::INVALID_ID.to_string())
            });
        match promoted {
            Ok(document) => {
                self.ai.reply_saved(id);
                // A quit waiting on the writer opens nothing more.
                if self.quit_flow.may_quit() {
                    self.open_vault_document(document, window, cx);
                }
            }
            Err(error) => {
                self.notices.post(text::could_not_promote(error));
            }
        }
        cx.notify();
    }

    /// Delete…: ask, then delete saved reply `id`.
    pub(in crate::workspace) fn confirm_delete_reply(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault() else {
            return;
        };
        let title = (vault.replies().into_iter())
            .find(|reply| reply.id == id)
            .map_or_else(|| text::THIS_REPLY.to_string(), |r| text::quoted(&r.title));
        let answer = window.prompt(
            PromptLevel::Warning,
            &text::delete_title(title),
            Some(text::DELETE_DETAIL),
            &[text::DELETE, crate::ui::text::CANCEL],
            cx,
        );
        let id = id.to_string();
        cx.spawn_in(window, async move |this, cx| {
            if answer.await.ok() == Some(0) {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.delete_saved_reply(&id, window, cx)
                });
            }
        })
        .detach();
    }

    /// Delete saved reply `id` once the jobs before it are done.
    pub(in crate::workspace) fn delete_saved_reply(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.vault.vault().is_none() {
            return;
        }
        self.next_op += 1;
        let target = Target::Op(self.next_op);
        self.queue_write(target, Write::DeleteReply(id.to_string()), window, cx);
    }

    /// Delete saved reply `id` at the head of the writer's queue.
    pub(in crate::workspace) fn run_delete_reply(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        match vault.delete_reply(id) {
            Ok(()) | Err(VaultError::NotFound { .. }) => {}
            Err(error) => {
                self.notices.post(text::could_not_delete(error));
                cx.notify();
                return;
            }
        }
        // A box that saved it starts a new reply with its next change.
        for view in self.tabs.iter().filter_map(|tab| tab.view()) {
            view.update(cx, |view, _| view.reply.forget_saved(id));
        }
        self.navigation.forget(&Stop::Reply(id.to_string()));
        if self.reply_page.as_ref().is_some_and(|page| page.id == id) {
            self.reply_page = None;
            self.show_home_as(Arrival::Replace, window, cx);
        }
        self.refresh_saved_replies(cx);
        cx.notify();
    }

    /// Each live document's "Saved replies" in Share: its Project's while it
    /// is in one, its own while Unfiled, newest first.
    pub(in crate::workspace) fn refresh_saved_replies(&mut self, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        let titles: std::collections::HashMap<String, String> = (vault.documents().into_iter())
            .map(|summary| (summary.id, summary.title))
            .collect();
        let tabs: Vec<_> = (self.tabs.iter())
            .filter_map(|tab| Some((tab.id(), tab.view()?.clone())))
            .collect();
        let saved: std::collections::HashSet<String> = (vault.replies().into_iter())
            .map(|reply| reply.id)
            .collect();
        for (tab, view) in tabs {
            // A promoted document's reply may have been deleted since.
            view.update(cx, |view, cx| {
                if let Some(from) = view.from_reply.as_mut() {
                    let gone = !saved.contains(&from.id);
                    if from.gone != gone {
                        from.gone = gone;
                        cx.notify();
                    }
                }
            });
            let Some(document) = self.tab_document(tab, cx) else {
                continue;
            };
            let project = view.read(cx).project.link().map(|link| link.id.clone());
            let replies = match &project {
                Some(project) => vault.project_replies(project),
                None => vault.document_replies(document.as_str()),
            };
            let rows: Vec<_> = (replies.into_iter())
                .map(|reply| SavedReplyRow {
                    detail: reply_detail(
                        &reply.owner,
                        reply.source_app.as_deref(),
                        reply.created,
                        &titles,
                    ),
                    id: reply.id,
                    title: reply.title,
                })
                .collect();
            view.update(cx, |view, cx| {
                if view.saved_replies != rows {
                    view.saved_replies = rows;
                    cx.notify();
                }
            });
        }
    }

    /// Show the open reply as saved on `seconds` (renders keep one date).
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(in crate::workspace) fn fix_reply_date(&mut self, seconds: u64) {
        if let Some(Ok(reply)) = self.reply_page.as_mut().map(|page| page.loaded.as_mut()) {
            reply.created = seconds;
        }
    }

    /// Focus the shown reply's Promote to Document (renders).
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(in crate::workspace) fn focus_reply_promote(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(page) = self.reply_page.as_ref() {
            window.focus(&page.promote_focus, cx);
        }
    }

    /// The reply page's title for Back and Forward.
    pub(in crate::workspace) fn reply_title(&self, id: &str) -> Option<String> {
        let vault = self.vault.vault()?;
        (vault.replies().into_iter())
            .find(|reply| reply.id == id)
            .map(|reply| reply.title)
    }

    /// Whether saved reply `id` still exists.
    pub(in crate::workspace) fn reply_exists(&self, id: &str) -> bool {
        self.vault
            .vault()
            .is_some_and(|vault| vault.replies().iter().any(|reply| reply.id == id))
    }

    /// The Reply page shows a reply that is gone (its Project was deleted):
    /// show Home in its place, and Back no longer stops there.
    pub(in crate::workspace) fn drop_gone_reply_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.reply_page.as_ref().map(|page| page.id.clone()) else {
            return;
        };
        if self.reply_exists(&id) {
            return;
        }
        self.navigation.forget(&Stop::Reply(id));
        self.reply_page = None;
        self.show_home_as(Arrival::Replace, window, cx);
    }

    /// What the Reply page says of `reply`'s place: every document's title
    /// by id, and its Project's name, or Unfiled.
    pub(in crate::workspace) fn reply_place(
        &self,
        reply: &Reply,
    ) -> (std::collections::HashMap<String, String>, Option<String>) {
        let titles: std::collections::HashMap<String, String> = (self.vault.vault())
            .map(|vault| {
                (vault.documents().into_iter())
                    .map(|summary| (summary.id, summary.title))
                    .collect()
            })
            .unwrap_or_default();
        let place = match &reply.owner {
            ReplyOwner::Project(id) => {
                (self.projects.project(id).map(|p| p.name.clone())).or_else(|| {
                    let vault = self.vault.vault()?;
                    (vault.projects().into_iter())
                        .find(|project| project.id == *id)
                        .map(|project| project.name)
                })
            }
            ReplyOwner::Document(id) => {
                let project = (self.vault.vault())
                    .and_then(|vault| vault.documents().into_iter().find(|s| s.id == *id))
                    .and_then(|summary| summary.project);
                match project.and_then(|id| self.projects.project(&id)) {
                    Some(project) => Some(project.name.clone()),
                    None => Some(text::UNFILED.to_string()),
                }
            }
        };
        (titles, place)
    }
}
