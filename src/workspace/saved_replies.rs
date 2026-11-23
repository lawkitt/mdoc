//! Saved AI Replies: a reply pasted into a document's reply box, or by
//! Paste AI Reply, is saved through the Vault as received
//! (`save_shared_reply`: a Project's document gives the Project's reply, an
//! Unfiled one its own), and the Reply page shows a saved reply restored
//! from the identities it was bound to as they are now. The page is rebuilt
//! from the Vault each time it shows; no restored text is kept.
use super::{
    Workspace,
    autosave::Write,
    document_writer::Target,
    navigation::{Arrival, Stop},
};
use crate::{pii::ui::SavedReplyRow, style, ui};
use gpui::{
    AnyElement, Context, Entity, FocusHandle, PromptLevel, SharedString, Window, div, prelude::*,
    px,
};
use mdoc_pii::restore::{ExportedIdentity, RestoreMap, RestoredText};
use mdoc_vault::{Reply, ReplyOwner, SharedFrom, VaultError};

/// A reply on its way into the Vault.
pub(super) struct ReplySave {
    /// The tab whose document it answers.
    pub(super) tab: u64,
    /// As pasted: aliases, never restored text.
    pub(super) markdown: String,
    /// The reply box's revision this text is, when it came from the box;
    /// the box's earlier saved reply is replaced by it. None: Paste AI Reply.
    pub(super) revision: Option<u64>,
    /// Show it on the Reply page once saved.
    pub(super) open: bool,
}

/// View state only: never serialized, cleared on any other destination.
pub(super) struct ReplyPage {
    id: String,
    loaded: Result<Reply, String>,
    editor: Entity<mdoc_editor::EditorState>,
    restored: RestoredText,
    /// The identities `restored` was made with.
    made_from: Option<Vec<ExportedIdentity>>,
    /// The unresolved token Next unrecognized moves to after the last.
    next: usize,
    copied: bool,
    scroll: gpui::ScrollHandle,
    copy_focus: FocusHandle,
    next_focus: FocusHandle,
    delete_focus: FocusHandle,
}

impl ReplyPage {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    /// The restored text as the page shows it.
    #[cfg(test)]
    pub(super) fn restored(&self) -> &RestoredText {
        &self.restored
    }

    #[cfg(test)]
    pub(super) fn test_controls(&self) -> Vec<(&'static str, FocusHandle)> {
        let mut controls = vec![
            ("reply-page-copy", self.copy_focus.clone()),
            ("reply-page-delete", self.delete_focus.clone()),
        ];
        if !self.restored.unresolved.is_empty() {
            controls.push(("reply-page-next", self.next_focus.clone()));
        }
        controls
    }

    /// "N values restored · M unrecognized", as the reply box counts.
    pub(super) fn summary(&self) -> String {
        let text = &self.restored.text;
        let unresolved: std::collections::HashSet<_> = (self.restored.unresolved.iter())
            .filter_map(|range| text.get(range.clone()))
            .collect();
        let values = self.restored.values;
        format!(
            "{values} {} restored · {} unrecognized",
            if values == 1 { "value" } else { "values" },
            unresolved.len()
        )
    }
}

impl Workspace {
    /// The tab's reply box, or Paste AI Reply in it: save `save` after the
    /// document's own writes and its Project's, so the aliases are bound to
    /// the names the Vault holds for them.
    pub(super) fn queue_reply_save(
        &mut self,
        save: ReplySave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = save.tab;
        if self.tab_document(tab, cx).is_none() {
            // A pasted document becomes one at its first write; the box's
            // reply is saved with the next change after that.
            if save.revision.is_none() || save.open {
                self.notices.post(
                    "The AI reply could not be saved: its document is not in mdoc yet. Type in \
                     it first, then paste the reply again."
                        .to_string(),
                );
                cx.notify();
            }
            return;
        }
        self.project_changed(tab, cx);
        self.autosave(tab, window, cx);
        self.queue_project_writes(window, cx);
        let target = if save.revision.is_some() {
            // Only the box's latest text waits.
            Target::Reply(tab)
        } else {
            self.next_op += 1;
            Target::Op(self.next_op)
        };
        self.queue_write(target, Write::Reply(save), window, cx);
    }

    /// Queue the reply in tab `tab`'s box when it changed since it was last
    /// saved: its document's writes are queued already.
    pub(super) fn queue_box_reply(
        &mut self,
        tab: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.tab_view(tab) else {
            return;
        };
        let Some((markdown, revision)) = view.read(cx).reply.unsaved() else {
            return;
        };
        // One waiting saves whatever the box holds when it runs.
        if self.document_writer.has(&Target::Reply(tab)) {
            return;
        }
        if self.tab_document(tab, cx).is_none() || self.vault.vault().is_none() {
            return;
        }
        self.queue_project_writes(window, cx);
        let save = ReplySave {
            tab,
            markdown,
            revision: Some(revision),
            open: false,
        };
        self.queue_write(Target::Reply(tab), Write::Reply(save), window, cx);
    }

    /// Save a reply at the head of the writer's queue.
    pub(super) fn run_reply_save(
        &mut self,
        save: ReplySave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.tab_view(save.tab) else {
            return;
        };
        let (Some(vault), Some(document)) =
            (self.vault.vault().cloned(), self.tab_document(save.tab, cx))
        else {
            return;
        };
        let (saved, unsaved, title) = view.read_with(cx, |view, _| {
            let name = view.display_name();
            let stem = name.strip_suffix(".md").unwrap_or(&name).to_string();
            (
                view.reply.saved(),
                view.reply.unsaved(),
                format!("Reply to {stem}"),
            )
        });
        // The box's reply is saved as the box holds it now; a later change
        // queues another save.
        let text = match save.revision {
            Some(_) => match (unsaved, &saved) {
                (Some((text, revision)), _) => Some((text, Some(revision))),
                (None, Some(_)) => None,
                // Cleared since: it keeps the reply it saved.
                (None, None) => {
                    self.refresh_saved_replies(cx);
                    return;
                }
            },
            None => Some((save.markdown, None)),
        };
        let id = match text {
            None => saved.clone().map(|(id, _)| id),
            Some((markdown, revision)) => {
                let from = SharedFrom::Document(document.as_str().to_string());
                match vault.save_shared_reply(&from, title, None, markdown) {
                    Ok(reply) => {
                        if let Some(revision) = revision {
                            // One fill of the box makes one reply.
                            if let Some((old, _)) = &saved
                                && let Err(error) = vault.delete_reply(old)
                                && !matches!(error, VaultError::NotFound { .. })
                            {
                                self.notices.post(format!(
                                    "Could not replace an earlier AI reply: {error}"
                                ));
                            }
                            view.update(cx, |view, _| view.reply.mark_saved(&reply.id, revision));
                        }
                        Some(reply.id)
                    }
                    Err(error) => {
                        self.notices
                            .post(format!("Could not save the AI reply: {error}"));
                        None
                    }
                }
            }
        };
        self.refresh_saved_replies(cx);
        if let Some(id) = id
            && save.open
        {
            self.open_reply(&id, window, cx);
        }
        cx.notify();
    }

    /// Show saved reply `id` on the Reply page.
    pub(super) fn open_reply(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.show_reply_page(id, Arrival::Visit, window, cx);
    }

    pub(super) fn show_reply_page(
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
            editor.set_copy_metadata(Some(crate::restored_clipboard::TAG.into()));
            editor
        });
        let focus = || cx.focus_handle().tab_index(0).tab_stop(true);
        let (copy_focus, next_focus, delete_focus) = (focus(), focus(), focus());
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
    fn reply_restore_map(&self, reply: &Reply) -> Result<RestoreMap, VaultError> {
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
    pub(super) fn refresh_reply_page(&mut self, cx: &mut Context<Self>) {
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
        let marks = crate::pii::ui::reply_marks(&restored, theme);
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
    pub(super) fn copy_saved_reply(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.reply_page.as_mut() else {
            return;
        };
        if page.loaded.is_err() {
            return;
        }
        cx.write_to_clipboard(crate::restored_clipboard::item(page.restored.text.clone()));
        page.copied = true;
        cx.notify();
    }

    /// Next unrecognized: reveal the next unresolved token, wrapping.
    pub(super) fn next_saved_unrecognised(&mut self, cx: &mut Context<Self>) {
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
        let id = crate::pii::ui::UNRESOLVED_ID | range.start as u64;
        page.editor.update(cx, |editor, cx| {
            editor.set_active_annotation(Some(id), cx);
            editor.reveal_offset(range.start, cx);
        });
        cx.notify();
    }

    /// The unresolved token Next unrecognized last moved to.
    #[cfg(test)]
    pub(super) fn current_saved_unrecognised(&self) -> Option<std::ops::Range<usize>> {
        let page = self.reply_page.as_ref()?;
        let at = page.next.checked_sub(1)?;
        page.restored.unresolved.get(at).cloned()
    }

    /// Delete…: ask, then delete saved reply `id`.
    pub(super) fn confirm_delete_reply(
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
            .map_or_else(|| "this reply".to_string(), |r| format!("“{}”", r.title));
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Delete {title}?"),
            Some("Its text is deleted from this computer. This cannot be undone."),
            &["Delete", "Cancel"],
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

    pub(super) fn delete_saved_reply(
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
                self.notices
                    .post(format!("Could not delete the AI reply: {error}"));
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
    pub(super) fn refresh_saved_replies(&mut self, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        let titles: std::collections::HashMap<String, String> = (vault.documents().into_iter())
            .map(|summary| (summary.id, summary.title))
            .collect();
        let tabs: Vec<_> = (self.tabs.iter())
            .filter_map(|tab| Some((tab.id(), tab.view()?.clone())))
            .collect();
        for (tab, view) in tabs {
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

    pub(super) fn reply_page_view(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = self.reply_page.as_ref().unwrap();
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let detached = self.title.detached.get();
        let bar = ui::title_drag(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(style::HEADER_HEIGHT))
                .pl(px(16. + self.title.inset.get()))
                .pr_2()
                .gap_2()
                .when(detached, |v| {
                    v.h(px(style::CARD_MARGIN + style::HEADER_HEIGHT))
                        .pt(px(style::CARD_MARGIN))
                })
                .text_size(px(13.)),
            self.title.armed.clone(),
        )
        .child("AI reply")
        .child(div().flex_1())
        .children(ui::notice_bell(&self.title, theme).map(|bell| ui::no_drag(div().child(bell))));
        let header = ui::title_bar(bar, (!detached).then_some(p.border), window, theme);
        let content = match &page.loaded {
            Ok(reply) => self.reply_content(reply, cx),
            Err(error) => div()
                .p_6()
                .text_color(p.header_muted)
                .child(format!("This reply can't be opened: {error}"))
                .into_any_element(),
        };
        let body = div()
            .id("reply-page-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&page.scroll)
            .child(div().w_full().flex().justify_center().p_6().child(content));
        div()
            .id("reply-page")
            .when(cfg!(test), |v| v.debug_selector(|| "reply-page".into()))
            .track_focus(self.focus_regions.home())
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(if detached {
                ui::card_surface(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .ml(px(style::CARD_MARGIN))
                        .child(body),
                    theme,
                )
                .into_any_element()
            } else {
                body.into_any_element()
            })
            .into_any_element()
    }

    fn reply_content(&self, reply: &Reply, cx: &mut Context<Self>) -> AnyElement {
        let page = self.reply_page.as_ref().unwrap();
        let theme = self.theme.get();
        let p = theme.pdf_style();
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
                    None => Some("Unfiled".to_string()),
                }
            }
        };
        let meta = [
            place.map(|place| match &reply.owner {
                ReplyOwner::Project(_) => format!("Project “{place}”"),
                ReplyOwner::Document(_) if place == "Unfiled" => place,
                ReplyOwner::Document(_) => format!("Project “{place}”"),
            }),
            Some({
                let detail = reply_detail(
                    &reply.owner,
                    reply.source_app.as_deref(),
                    reply.created,
                    &titles,
                );
                // The place already names the Project.
                (detail.strip_prefix("From the Project · ")).map_or(detail.clone(), str::to_string)
            }),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let control = |id: &'static str, label: SharedString, focus: &FocusHandle| {
            ui::reveal_focus(
                ui::primary_button(id, label, theme, true)
                    .track_focus(focus)
                    .when(cfg!(test), move |v| v.debug_selector(move || id.into())),
                focus.clone(),
                page.scroll.clone(),
            )
        };
        let id = page.id.clone();
        let unresolved = !page.restored.unresolved.is_empty();
        div()
            .w_full()
            .max_w(px(680.))
            .flex()
            .flex_col()
            .gap_3()
            .text_size(px(13.))
            .child(
                div()
                    .text_size(px(26.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(reply.title.clone()),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(meta),
            )
            .child(
                div()
                    .id("reply-page-summary")
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "reply-page-summary".into())
                    })
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(page.summary()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        control(
                            "reply-page-copy",
                            if page.copied { "Copied" } else { "Copy reply" }.into(),
                            &page.copy_focus,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.copy_saved_reply(cx))),
                    )
                    .when(unresolved, |v| {
                        v.child(
                            control(
                                "reply-page-next",
                                "Next unrecognized".into(),
                                &page.next_focus,
                            )
                            .on_click(
                                cx.listener(|this, _, _, cx| this.next_saved_unrecognised(cx)),
                            ),
                        )
                    })
                    .child(
                        control(
                            "reply-page-delete",
                            "Delete reply…".into(),
                            &page.delete_focus,
                        )
                        .text_color(style::markdown_style(theme).popover_danger)
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.confirm_delete_reply(&id, window, cx),
                        )),
                    ),
            )
            .child(
                div()
                    .id("reply-page-text")
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "reply-page-text".into())
                    })
                    .mt_2()
                    .p_4()
                    .rounded_md()
                    .border_1()
                    .border_color(p.border)
                    .child(page.editor.clone()),
            )
            .into_any_element()
    }

    /// Show the open reply as saved on `seconds` (renders keep one date).
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(super) fn fix_reply_date(&mut self, seconds: u64) {
        if let Some(Ok(reply)) = self.reply_page.as_mut().map(|page| page.loaded.as_mut()) {
            reply.created = seconds;
        }
    }

    /// The reply page's title for Back and Forward.
    pub(super) fn reply_title(&self, id: &str) -> Option<String> {
        let vault = self.vault.vault()?;
        (vault.replies().into_iter())
            .find(|reply| reply.id == id)
            .map(|reply| reply.title)
    }

    /// Whether saved reply `id` still exists.
    pub(super) fn reply_exists(&self, id: &str) -> bool {
        self.vault
            .vault()
            .is_some_and(|vault| vault.replies().iter().any(|reply| reply.id == id))
    }
}

/// "From “Lease.pdf” · Claude · 23 Nov".
pub(super) fn reply_detail(
    owner: &ReplyOwner,
    source_app: Option<&str>,
    created: u64,
    titles: &std::collections::HashMap<String, String>,
) -> String {
    let from = match owner {
        ReplyOwner::Document(document) => titles
            .get(document)
            .map_or_else(|| "From a document".to_string(), |t| format!("From “{t}”")),
        ReplyOwner::Project(_) => "From the Project".to_string(),
    };
    [
        Some(from),
        source_app.map(str::to_string),
        Some(super::project_page::created_date(created)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}
