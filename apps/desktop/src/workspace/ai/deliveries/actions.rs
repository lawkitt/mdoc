//! Deliveries' actions: hearing from the OS's notifications, raising one for
//! a delivery or a review request, opening what a click or the badge names.
use crate::workspace::ai::deliveries::{APP_ID, Note};
use crate::workspace::ai::text;
use crate::workspace::{Workspace, ai::project::Arrived};
use gpui::{App, Context, Window};
use mdoc_store::document_id::VaultId;
use std::time::Instant;

impl Workspace {
    /// The connector started: notifications carry the app's identity, and
    /// a click on one comes back to this window. Taken only now, so with
    /// the preview off the app's identity stays as it was.
    pub(in crate::workspace) fn listen_for_notifications(&mut self, cx: &mut Context<Self>) {
        if self.notifier.system && self.notifier.posts() {
            cx.set_app_identity(APP_ID, "mdoc");
        }
        let workspace = cx.entity().downgrade();
        let window = self.window_handle;
        cx.on_system_notification_response(move |response, cx| {
            let Some(item) = Arrived::from_tag(&response.tag) else {
                return;
            };
            let _ = window.update(cx, |_, window, cx| {
                let _ = workspace.update(cx, |workspace, cx| {
                    workspace.notification_clicked(&item, window, cx)
                });
            });
        });
    }

    fn notify_os(&self, note: &Note, cx: &mut Context<Self>) {
        if self.notifier.posts() {
            (self.notifier.post)(note, cx);
        }
    }

    /// AI app `app` delivered `reply`, landed as `document` when it asked
    /// for one: a notification, the badge, and with "Open deliveries when
    /// they arrive" on, it opens in front, unless `picker_was_up`: a system
    /// file picker may have been up when it arrived.
    pub(in crate::workspace) fn ai_delivered(
        &mut self,
        app: &str,
        reply: String,
        document: Option<String>,
        picker_was_up: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai
            .delivered(reply.clone(), app.to_string(), document.is_some());
        let item = match document {
            Some(document) => Arrived::Document(document),
            None => Arrived::Reply(reply),
        };
        if self.ai.arrived(item.clone(), Instant::now()) {
            self.notify_os(&Note::of(&item, app, ""), cx);
        }
        crate::shelf::menu_bar::ai_arrived(true, cx);
        // A dialog, sheet or file picker up keeps it on the badge: nothing
        // moves under it. The switch must be on as saved and as Settings
        // shows it, so it is off at once and on only once saved.
        let open = self.ai_setting().open_deliveries && self.settings.read(cx).opens_deliveries();
        if open && !picker_was_up && !self.modal_up(window, cx) {
            crate::shelf::menu_bar::reveal_window(window, cx);
            self.open_arrived(&item, window, cx);
        }
        cx.notify();
    }

    /// AI app `app` asked for `document`, named to it as `label`, which
    /// needs review: a notification, at most one per document within
    /// [`crate::workspace::ai::project::REVIEW_QUIET`], and the badge. Nothing opens by
    /// itself.
    pub(in crate::workspace) fn ai_review_requested(
        &mut self,
        app: &str,
        document: String,
        label: &str,
        cx: &mut Context<Self>,
    ) {
        // Heard after Stop sharing, or after another Project was shared:
        // the request is moot.
        let member = self.ai_shared().is_some_and(|shared| {
            (self.vault.vault().into_iter())
                .flat_map(|vault| vault.documents_in(Some(&shared)))
                .any(|summary| summary.id == document)
        });
        if !member {
            return;
        }
        self.ai
            .review_asked(document.clone(), label.to_string(), app.to_string());
        let item = Arrived::Review(document);
        // The companion opens as the notification shows: not again for a
        // request repeated within the quiet time.
        let new = self.ai.arrived(item.clone(), Instant::now());
        if new {
            self.notify_os(&Note::of(&item, app, label), cx);
        }
        crate::shelf::menu_bar::ai_arrived(new, cx);
        cx.notify();
    }

    /// The user clicked the notification of `item`: the window comes
    /// forward and opens it. With the preview off since, nothing of the
    /// connector is reachable: a click on one left behind does nothing.
    pub(in crate::workspace) fn notification_clicked(
        &mut self,
        item: &Arrived,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ai_connector_on() {
            return;
        }
        crate::shelf::menu_bar::reveal_window(window, cx);
        // A dialog or sheet up answers first; it stays on the badge.
        if !self.modal_up(window, cx) {
            self.open_arrived(item, window, cx);
        }
    }

    /// Open `item` where it lands: a reply on its Reply page, a document
    /// in its tab for review. One deleted since says so.
    pub(in crate::workspace) fn open_arrived(
        &mut self,
        item: &Arrived,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        match item {
            Arrived::Reply(id) => {
                self.ai.seen(Some(id), None);
                if vault.reply(id).is_err() {
                    self.notices.post(text::REPLY_DELETED.to_string());
                } else {
                    self.open_reply(id, window, cx);
                }
            }
            Arrived::Document(id) | Arrived::Review(id) => {
                self.ai.seen(None, Some(id));
                match VaultId::parse(id).filter(|_| vault.document(id).is_ok()) {
                    Some(document) => {
                        // For review: its Review panel opens, now or once
                        // it has loaded.
                        self.ai.review_on_load(id);
                        self.open_project_member(document, window, cx);
                        let tab = self.tabs.active();
                        self.review_if_asked(tab, window, cx);
                    }
                    None => self.notices.post(text::DOCUMENT_DELETED.to_string()),
                }
            }
        }
        // Opened: the companion drops its request.
        crate::shelf::menu_bar::companion_changed(cx);
        cx.notify();
    }

    /// Tab `id`, loaded, holds a document opened for review: its Review
    /// panel opens, as Pseudonymize opens it.
    pub(in crate::workspace) fn review_if_asked(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.tab_view(id) else {
            return;
        };
        let (document, loaded) = {
            let view = view.read(cx);
            let lifecycle = &view.lifecycle;
            (
                view.session.document.vault.clone(),
                !lifecycle.loading() && !lifecycle.unavailable(),
            )
        };
        let Some(document) = document else {
            return;
        };
        if !loaded || !self.ai.take_review(document.as_str()) {
            return;
        }
        view.update(cx, |view, cx| view.open_review(window, cx));
    }

    /// A dialog, sheet, system file picker or Settings is up: nothing
    /// navigates under it.
    pub(in crate::workspace) fn modal_up(&self, window: &Window, cx: &App) -> bool {
        self.drawn_modal_up(window, cx) || crate::app::native_picker::up(cx)
    }

    /// A dialog, sheet or Settings that mdoc draws is up. A system file
    /// picker is a window of its own, over none of mdoc's layers.
    pub(in crate::workspace) fn drawn_modal_up(&self, window: &Window, cx: &App) -> bool {
        self.settings.read(cx).open
            || window.has_active_prompt()
            || self.new_project.is_some()
            || self.folder_sheets.open()
            || self.project_ui.explainer
    }

    /// The badge's click, and the rail's while it counts: the newest
    /// arrival opens.
    pub(in crate::workspace) fn open_newest(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(item) = self.ai.unseen().last().cloned() {
            self.open_arrived(&item, window, cx);
        }
    }
}
