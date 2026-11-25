//! Notifications and where a reply lands (ADR 0126; #1164 Decisions 4 and
//! 6; Milestone 2 item 9), all of it only while "AI connector (preview)"
//! is on:
//!
//! - a delivery, and a request for a document that needs review, each
//!   raise one OS notification; clicking it brings the window forward and
//!   opens the Reply page, or the document for review (a delivered
//!   Document opens for review too);
//! - the AI card's badge counts what arrived and is not opened yet, and
//!   opens the newest;
//! - mdoc never takes focus by itself unless "Open deliveries when they
//!   arrive" is on: then a delivery opens where it lands, in front;
//! - the companion window (item 14) opens by itself for each, without
//!   taking focus, and shows the latest reply and review request.
//!
//! A notification names only the AI app and the AI label of the document
//! asked for: never a title, a restored name or document text. Its tag
//! holds ids only. GPUI posts it (`App::show_system_notification`); tests
//! and headless renders inject a [`Notifier`] that records instead.
use super::{Workspace, ai_project::Arrived};
use crate::{ai_connector, ui};
use gpui::{AnyElement, App, Context, SharedString, Window, div, prelude::*, px};
use mdoc_store::document_id::VaultId;
use std::{rc::Rc, time::Instant};

/// The app's identity for notifications: the packager's identifier
/// (`[package.metadata.packager]`), the Windows AppUserModelID.
pub(super) const APP_ID: &str = "io.github.lawkitt.mdoc";

/// The longest AI label a notification quotes, in characters.
const LABEL_CHARS: usize = 80;

/// One notification, as mdoc words it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Note {
    /// Ids only: [`Arrived::tag`].
    pub(super) tag: String,
    pub(super) title: String,
    pub(super) body: String,
}

impl Note {
    /// `item` from AI app `app`, as it sent itself; `label`, a review
    /// request's AI label.
    pub(super) fn of(item: &Arrived, app: &str, label: &str) -> Self {
        // A product's name, never the client's own free text.
        let app = ai_connector::product_name(app).unwrap_or("An AI app");
        let (title, body) = match item {
            Arrived::Reply(_) => (
                format!("{app} sent a reply"),
                "Open it in mdoc to read it with the names restored.".to_string(),
            ),
            Arrived::Document(_) => (
                format!("{app} sent a document"),
                "Open it in mdoc to review it.".to_string(),
            ),
            Arrived::Review(_) => {
                let label: String = (label.chars())
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .take(LABEL_CHARS)
                    .collect();
                (
                    format!("{app} asked for \u{201c}{}\u{201d}", label.trim()),
                    "It needs review before AI apps can read it. Open it in mdoc to review it."
                        .to_string(),
                )
            }
        };
        Self {
            tag: item.tag(),
            title,
            body,
        }
    }
}

/// What posts a note.
type Post = Rc<dyn Fn(&Note, &mut App)>;

/// Where the window's notifications go: the OS, by default.
#[derive(Clone)]
pub(super) struct Notifier {
    post: Post,
    /// The OS's: the app takes its identity when the connector starts.
    system: bool,
}

impl Default for Notifier {
    fn default() -> Self {
        Self {
            post: Rc::new(|note, cx| {
                cx.show_system_notification(gpui::SystemNotification {
                    tag: note.tag.clone().into(),
                    title: note.title.clone().into(),
                    body: note.body.clone().into(),
                    actions: Vec::new(),
                })
            }),
            system: !mdoc_store::app_paths::isolated(),
        }
    }
}

impl Notifier {
    /// One that keeps each note in the list it returns, and posts none.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(super) fn recording() -> (Self, Rc<std::cell::RefCell<Vec<Note>>>) {
        let notes = Rc::new(std::cell::RefCell::new(Vec::new()));
        let kept = notes.clone();
        let notifier = Self {
            post: Rc::new(move |note, _| kept.borrow_mut().push(note.clone())),
            system: false,
        };
        (notifier, notes)
    }

    /// Renders: the OS's notifier posts nothing there.
    fn posts(&self) -> bool {
        !self.system || !mdoc_store::app_paths::isolated()
    }
}

impl Workspace {
    /// The connector started: notifications carry the app's identity, and
    /// a click on one comes back to this window. Taken only now, so with
    /// the preview off the app's identity stays as it was.
    pub(super) fn listen_for_notifications(&mut self, cx: &mut Context<Self>) {
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
    /// they arrive" on, it opens in front.
    pub(super) fn ai_delivered(
        &mut self,
        app: &str,
        reply: String,
        document: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai.delivered(reply.clone(), app.to_string());
        let item = match document {
            Some(document) => Arrived::Document(document),
            None => Arrived::Reply(reply),
        };
        if self.ai.arrived(item.clone(), Instant::now()) {
            self.notify_os(&Note::of(&item, app, ""), cx);
        }
        crate::menu_bar::ai_arrived(true, cx);
        // A dialog or sheet up keeps it on the badge: nothing moves under
        // it.
        if self.ai_setting().open_deliveries && !self.modal_up(window, cx) {
            crate::menu_bar::reveal_window(window, cx);
            self.open_arrived(&item, window, cx);
        }
        cx.notify();
    }

    /// AI app `app` asked for `document`, named to it as `label`, which
    /// needs review: a notification, at most one per document within
    /// [`super::ai_project::REVIEW_QUIET`], and the badge. Nothing opens by
    /// itself.
    pub(super) fn ai_review_requested(
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
        crate::menu_bar::ai_arrived(new, cx);
        cx.notify();
    }

    /// The user clicked the notification of `item`: the window comes
    /// forward and opens it. With the preview off since, nothing of the
    /// connector is reachable: a click on one left behind does nothing.
    pub(super) fn notification_clicked(
        &mut self,
        item: &Arrived,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ai_connector_on() {
            return;
        }
        crate::menu_bar::reveal_window(window, cx);
        // A dialog or sheet up answers first; it stays on the badge.
        if !self.modal_up(window, cx) {
            self.open_arrived(item, window, cx);
        }
    }

    /// Open `item` where it lands: a reply on its Reply page, a document
    /// in its tab for review. One deleted since says so.
    pub(super) fn open_arrived(
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
                    self.notices.post("That AI reply was deleted.".to_string());
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
                    None => self.notices.post("That document was deleted.".to_string()),
                }
            }
        }
        // Opened: the companion drops its request.
        crate::menu_bar::companion_changed(cx);
        cx.notify();
    }

    /// Tab `id`, loaded, holds a document opened for review: its Review
    /// panel opens, as Pseudonymize opens it.
    pub(super) fn review_if_asked(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
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

    /// A dialog, sheet or Settings is up: nothing navigates under it.
    pub(super) fn modal_up(&self, window: &Window, cx: &App) -> bool {
        self.settings.read(cx).open
            || window.has_active_prompt()
            || self.new_project.is_some()
            || self.folder_sheets.open()
            || self.project_ui.explainer
    }

    // ---- What the window shows --------------------------------------------

    /// The AI card's badge: how many things arrived and are not opened
    /// yet; a click opens the newest.
    pub(super) fn ai_badge(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let count = self.ai.unseen().len();
        if count == 0 {
            return None;
        }
        let theme = self.theme.get();
        Some(
            // A control on the accent: its own hover, not the grey one.
            ui::focus_ring("ai-badge", theme)
                .when(cfg!(test), |v| v.debug_selector(|| "ai-badge".into()))
                .role(gpui::Role::Button)
                .aria_label(format!("{count} new from AI apps, open the newest"))
                .key_context("UiControl")
                .tab_index(0)
                .tab_stop(true)
                .cursor_pointer()
                .hover(|s| s.opacity(0.85))
                .flex_shrink_0()
                .px_2()
                .py(px(1.))
                .rounded_full()
                .bg(theme.search_accent())
                .text_color(theme.badge_fg())
                .text_size(px(10.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(format!("{count} new"))
                .on_click(cx.listener(|this, _, window, cx| this.open_newest(window, cx)))
                .into_any_element(),
        )
    }

    /// The collapsed rail's count of what arrived, over the AI icon.
    pub(super) fn ai_rail_badge(&self) -> Option<AnyElement> {
        let count = self.ai.unseen().len();
        if count == 0 {
            return None;
        }
        let theme = self.theme.get();
        Some(
            div()
                .when(cfg!(test), |v| v.debug_selector(|| "rail-ai-badge".into()))
                .absolute()
                .top(px(1.))
                .right(px(0.))
                .min_w(px(14.))
                .h(px(14.))
                .px(px(3.))
                .rounded_full()
                .bg(theme.search_accent())
                .text_color(theme.badge_fg())
                .text_size(px(10.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .flex()
                .items_center()
                .justify_center()
                .child(SharedString::from(count.to_string()))
                .into_any_element(),
        )
    }

    /// The badge's click, and the rail's while it counts: the newest
    /// arrival opens.
    pub(super) fn open_newest(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = self.ai.unseen().last().cloned() {
            self.open_arrived(&item, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Notification text names the app and the AI label only.
    #[test]
    fn notes_name_the_app_and_the_label_only() {
        let reply = Note::of(&Arrived::Reply("r1".into()), "claude-ai", "");
        assert_eq!(
            reply,
            Note {
                tag: "ai-reply-r1".into(),
                title: "Claude sent a reply".into(),
                body: "Open it in mdoc to read it with the names restored.".into(),
            }
        );
        let document = Note::of(&Arrived::Document("d1".into()), "openai-chatgpt", "");
        assert_eq!(document.title, "ChatGPT sent a document");
        assert_eq!(document.body, "Open it in mdoc to review it.");
        assert_eq!(document.tag, "ai-document-d1");
        // A name the client made up is not shown.
        let other = Note::of(&Arrived::Reply("r".into()), "Maria Lopez's lease", "");
        assert_eq!(other.title, "An AI app sent a reply");
        let review = Note::of(&Arrived::Review("d2".into()), "", "Document\n3");
        assert_eq!(
            review.title,
            "An AI app asked for \u{201c}Document 3\u{201d}"
        );
        assert_eq!(
            review.body,
            "It needs review before AI apps can read it. Open it in mdoc to review it."
        );
        let long = Note::of(&Arrived::Review("d".into()), "claude-ai", &"x".repeat(200));
        assert!(long.title.chars().count() < 120, "{}", long.title);
    }
}
