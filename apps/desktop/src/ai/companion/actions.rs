//! What the companion carries out: reading the window again when what it
//! shares or asks to review changes, the shelf's rows and their use, Hide
//! and Quit, Stop sharing, the reply's Copy and Save as document, the review
//! request's Open in mdoc and Replace all, and files dropped on it.
use super::{CompanionView, proposals};
use crate::{
    shelf::{
        ShelfKey, menu_bar,
        shelf_ui::{COPIED_FOR, ShelfRows, ShelfRowsEvent},
    },
    workspace::{CompanionContent, Quit, ReviewStep, Workspace},
};
use gpui::{Context, Entity, ExternalPaths, Subscription, Window, WindowHandle, prelude::*};

impl CompanionView {
    /// The window changed: read it again if what it shares, what it asks
    /// to review, or that document's review changed.
    pub(super) fn marks_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(marks) = self.workspace.read(cx).map(|w| w.companion_marks(cx)) else {
            return;
        };
        if marks != self.marks {
            self.marks = marks;
            self.refresh(window, cx);
        }
        self.watch_review(window, cx);
    }

    /// Watch the requested document's tab, if one has it.
    pub(super) fn watch_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = (self.workspace.read(cx).ok()).and_then(|w| w.companion_review_view(cx));
        let id = view.as_ref().map(|view| view.entity_id());
        if self.review_watch.as_ref().map(|(watched, _)| *watched) == id {
            return;
        }
        self.review_watch = view.map(|view| {
            let watch = cx.observe_in(&view, window, |this: &mut Self, _, window, cx| {
                this.marks_changed(window, cx)
            });
            (view.entity_id(), watch)
        });
    }

    pub(super) fn rows(
        workspace: WindowHandle<Workspace>,
        content: &CompanionContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<ShelfRows>, Subscription) {
        let documents = content.rows.clone();
        let hint = content.drag_hint;
        let rows = cx.new(|cx| ShelfRows::new(workspace, documents, hint, window, cx));
        let events = cx.subscribe_in(&rows, window, |this, _, event, window, cx| match *event {
            // "Copied" shows on the row for a moment; the companion stays.
            ShelfRowsEvent::Copied(_) => {
                this.row_copied = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(COPIED_FOR).await;
                    let _ = this.update_in(cx, |this, window, cx| this.refresh(window, cx));
                }));
            }
            ShelfRowsEvent::Dragged(_) => {}
            ShelfRowsEvent::Open(index) => this.open_row(index, None, cx),
            ShelfRowsEvent::Step(index, step) => this.open_row(index, Some(step), cx),
        });
        (rows, events)
    }

    /// Read the window again: what is shared, its documents, the latest
    /// reply and review request, and the shelf's rows.
    pub(crate) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        ))]
        if self.frozen {
            return;
        }
        let Ok(content) = self.workspace.read(cx).map(|w| w.companion_content(cx)) else {
            return;
        };
        // A drag of a row keeps its rows until it ends.
        if !self.rows.read(cx).dragging() {
            let (rows, events) = Self::rows(self.workspace, &content, window, cx);
            self.rows = rows;
            self._rows_events = events;
        }
        if content.reply.as_ref().map(|r| &r.id) != self.content.reply.as_ref().map(|r| &r.id) {
            self.copied = None;
        }
        // Another card's proposals show only once the pointer moves onto
        // them: a hover over the last card is no hover over these.
        if proposals(&content) != proposals(&self.content) {
            self.proposals_hovered = false;
        }
        self.content = content;
        self.watch_review(window, cx);
        cx.notify();
    }

    /// Hide: the window closes; the item or the next delivery opens it.
    pub(crate) fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        menu_bar::companion_closed(cx);
    }

    /// Run `then` in mdoc's window, brought forward; the companion stays.
    fn in_mdoc(
        &self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    ) {
        let workspace = self.workspace;
        cx.defer(move |cx| {
            let _ = workspace.update(cx, |workspace, window, cx| {
                menu_bar::reveal_window(window, cx);
                then(workspace, window, cx);
            });
        });
    }

    /// Run `then` in mdoc's window, which stays where it is.
    fn in_window(
        &self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    ) {
        let workspace = self.workspace;
        cx.defer(move |cx| {
            let _ = workspace.update(cx, then);
        });
    }

    /// A shelf row's click, or its step: in mdoc, as the shelf opens it.
    fn open_row(
        &mut self,
        index: usize,
        step: Option<crate::review::refusal::Step>,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.rows.read(cx).documents().get(index) else {
            return;
        };
        let key: ShelfKey = document.key.clone();
        let ready = document.readiness.is_ready();
        self.in_mdoc(cx, move |workspace, window, cx| {
            workspace.open_from_shelf(&key, ready && step.is_none(), step, window, cx)
        });
    }

    /// Stop sharing: AI apps read nothing until a Project is shared again.
    pub(crate) fn stop_sharing(&mut self, cx: &mut Context<Self>) {
        self.in_window(cx, |workspace, _, cx| workspace.companion_stop_sharing(cx));
    }

    /// The reply's Copy: its restored text, read from the Vault now and
    /// marked as restored, as the Reply page's Copy is.
    pub(crate) fn copy_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reply) = &self.content.reply else {
            return;
        };
        let text = (self.workspace.read(cx).ok()).and_then(|w| w.companion_reply_text(&reply.id));
        let Some(text) = text else {
            return;
        };
        cx.write_to_clipboard(crate::review::restored_clipboard::item(text));
        self.copied = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Save as document: the reply becomes a Document in its place, once.
    pub(crate) fn save_reply(&mut self, cx: &mut Context<Self>) {
        let Some(reply) = (self.content.reply.as_mut()).filter(|r| !r.saved && !r.saving) else {
            return;
        };
        // Saved only once the promotion succeeds; the companion reads it
        // again when it ends.
        reply.saving = true;
        reply.failed = false;
        let id = reply.id.clone();
        self.in_window(cx, move |workspace, window, cx| {
            workspace.companion_save_reply(&id, window, cx)
        });
        cx.notify();
    }

    /// Open in mdoc on the review request: its review in mdoc's window,
    /// brought forward, for whatever the companion does not decide.
    pub(crate) fn open_in_mdoc(&mut self, cx: &mut Context<Self>) {
        let Some(review) = &self.content.review else {
            return;
        };
        let document = review.document.clone();
        self.in_window(cx, move |workspace, window, cx| {
            workspace.companion_review(&document, window, cx)
        });
    }

    /// Replace all on the review request: every proposal it lists, in
    /// mdoc's window, which stays where it is.
    pub(crate) fn replace_all(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.content.review.as_mut() else {
            return;
        };
        let ReviewStep::Proposals(proposals) = &review.step else {
            return;
        };
        let listed = proposals.clone();
        review.step = ReviewStep::Replacing(listed.clone());
        let document = review.document.clone();
        self.in_window(cx, move |workspace, window, cx| {
            workspace.companion_replace_all(&document, listed, window, cx)
        });
        cx.notify();
    }

    /// Files dropped on the drop target. A row's own drag coming back is
    /// no drop.
    pub(crate) fn dropped(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        if self.dropped_on_companion(cx) {
            return;
        }
        let paths = paths.clone();
        self.in_window(cx, move |workspace, window, cx| {
            workspace.companion_drop(&paths, window, cx)
        });
    }

    /// A drop anywhere on the companion: `true` when it was a row's drag.
    pub(super) fn dropped_on_companion(&mut self, cx: &mut Context<Self>) -> bool {
        self.rows.update(cx, |rows, _| {
            let dragging = rows.dragging();
            if dragging {
                rows.dropped_on_shelf();
            }
            dragging
        })
    }

    /// Quit, from the footer: the companion hides and the quit flow runs.
    pub(super) fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hide(window, cx);
        // The quit flow asks about unsaved work as ⌘Q does.
        cx.defer(|cx| cx.dispatch_action(&Quit));
    }

    /// Open mdoc, from the footer: Home in mdoc's window, brought forward.
    pub(super) fn open_mdoc(&mut self, cx: &mut Context<Self>) {
        self.in_mdoc(cx, |workspace, window, cx| {
            workspace.home_from_shelf(window, cx)
        })
    }

    /// The footer's Settings gear: Settings in mdoc's window, brought
    /// forward.
    pub(super) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.in_mdoc(cx, |workspace, window, cx| {
            workspace.settings_from_shelf(window, cx)
        })
    }
}
