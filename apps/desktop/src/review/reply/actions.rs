//! What the AI reply box does on a tab: filling and following it,
//! showing the Reply view, copying the reply and saving it once it
//! settles.
use super::{ReplyEffect, reply_marks};
use crate::document::DocumentView;
use crate::review::{compare::CompareMode, restore::UNRESOLVED_ID, text};
use gpui::{Context, Entity, Focusable, Window, prelude::*};
use mdoc_pii::restore::ExportedIdentity;
use mdoc_pii::restore::RestoreMap;

impl DocumentView {
    /// The box is an editor so a reply keeps its lines; it is made when the
    /// Share tab first draws. It is plain text, so a paste is kept exactly
    /// as the AI wrote it: no Markdown paste or Enter rules. The caret stays
    /// in view in its scrolling box.
    pub(crate) fn ensure_reply_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reply_box.is_some() {
            return;
        }
        let text = self.reply.text().to_owned();
        let scroll = self.reply_scroll.clone();
        let editor = cx.new(|cx| {
            let mut editor = mdoc_editor::EditorState::new(window, cx)
                .with_placeholder(text::PASTE_THE_AIS_REPLY);
            editor.set_plain_text(true);
            editor.set_autoscroll(scroll);
            editor.set_text(text, cx);
            // Unlike the document editor, this field participates in the
            // Share panel's Tab order. Clones share the focus metadata.
            editor.focus_handle(cx).tab_index(0).tab_stop(true);
            editor
        });
        let subscription = cx.subscribe_in(&editor, window, |this, editor, event, window, cx| {
            if let mdoc_editor::EditorEvent::Changed = event {
                let text = editor.read(cx).text().to_owned();
                let effects = this.reply.edited(&text, this.showing_reply());
                this.carry_out_reply(effects, window, cx);
            }
        });
        self.reply_box = Some((editor, subscription));
    }

    pub(crate) fn reply_box(&self) -> Option<&Entity<mdoc_editor::EditorState>> {
        self.reply_box.as_ref().map(|(editor, _)| editor)
    }

    /// Fill the box, as a paste does (tests and headless renders).
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn fill_reply(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_reply_box(window, cx);
        if let Some(editor) = self.reply_box().cloned() {
            editor.update(cx, |editor, cx| editor.set_text(text.to_owned(), cx));
        }
        let effects = self.reply.edited(text, self.showing_reply());
        self.carry_out_reply(effects, window, cx);
    }

    /// Whether the Reply view is the one open.
    fn showing_reply(&self) -> bool {
        self.compare.as_ref().map(|c| c.mode) == Some(CompareMode::Reply)
    }

    /// A move renumbered this document's names (`aliases`, beside `names`
    /// as they were): its reply's tokens follow.
    pub(crate) fn realias_reply(
        &mut self,
        names: Vec<ExportedIdentity>,
        aliases: &[Option<String>],
        cx: &mut Context<Self>,
    ) {
        self.reply.realias(names, aliases);
        self.show_reply_text(cx);
    }

    /// The box shows the reply as it is now.
    fn show_reply_text(&mut self, cx: &mut Context<Self>) {
        let text = self.reply.text().to_owned();
        if let Some(editor) = self.reply_box().cloned()
            && editor.read(cx).text() != text
        {
            editor.update(cx, |editor, cx| editor.set_text(text, cx));
        }
    }

    fn bind_reply(&mut self) {
        let review = &self.pii.review;
        self.reply
            .bind(review.revision(), review.export_identities_with_ids());
    }

    /// The mapping may have changed: once this update settles (a join or an
    /// undo commits its identities last), the reply's alias tokens follow
    /// any identity whose alias changed, and the Project keeps the result.
    pub(crate) fn follow_reply_aliases_soon(&mut self, cx: &mut Context<Self>) {
        if !self.reply.exists() {
            return;
        }
        let weak = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = weak.update(cx, |this, cx| this.follow_reply_aliases(cx));
        });
    }

    fn follow_reply_aliases(&mut self, cx: &mut Context<Self>) {
        let review = &self.pii.review;
        if !self
            .reply
            .follow(review.revision(), || review.export_identities_with_ids())
        {
            return;
        }
        self.show_reply_text(cx);
        cx.notify();
    }

    fn carry_out_reply(
        &mut self,
        effects: Vec<ReplyEffect>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for effect in effects {
            match effect {
                ReplyEffect::Store => {
                    // A new reply's tokens name today's identities.
                    self.bind_reply();
                    self.save_reply_soon(cx);
                }
                ReplyEffect::ShowReply => {
                    // Typing goes on in the box while the view opens beside it.
                    let focused = window.focused(cx);
                    self.set_view_mode(Some(CompareMode::Reply), window, cx);
                    if let Some(focus) = focused {
                        window.focus(&focus, cx);
                    }
                }
                ReplyEffect::LeaveReply => {
                    if self.showing_reply() {
                        let focused = window.focused(cx);
                        self.close_compare(window, cx);
                        if let Some(focus) = focused {
                            window.focus(&focus, cx);
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    /// Bring the reply's restoration up to date with the reply and mapping.
    pub(crate) fn refresh_reply(&mut self) -> bool {
        let review = &self.pii.review;
        self.reply.refresh(review.revision(), || {
            RestoreMap::new(review.export_identities())
        })
    }

    /// Copy reply: the restored text.
    pub(crate) fn copy_reply(&mut self, cx: &mut Context<Self>) {
        self.refresh_reply();
        if !self.reply.restorable() {
            return;
        }
        cx.write_to_clipboard(crate::review::restored_clipboard::item(
            self.reply.restored().text.clone(),
        ));
        self.copied_for_ai = false;
        self.copied_reply = true;
        self.show_copy_feedback(cx);
    }

    /// Open reply: the box's reply, saved now if it changed, on the Reply
    /// page.
    pub(crate) fn open_saved_reply(&mut self, cx: &mut Context<Self>) {
        if !self.reply.exists() {
            return;
        }
        self.reply_save = None;
        cx.emit(crate::workspace::TabEvent::Reply(
            crate::workspace::ReplyEvent::Settled { open: true },
        ));
    }

    /// Once the box's text settles, the workspace saves it; a cleared box
    /// saves nothing.
    fn save_reply_soon(&mut self, cx: &mut Context<Self>) {
        if self.reply.unsaved().is_none() {
            self.reply_save = None;
            return;
        }
        self.reply_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REPLY_SETTLE).await;
            let _ = this.update(cx, |this, cx| {
                this.reply_save = None;
                cx.emit(crate::workspace::TabEvent::Reply(
                    crate::workspace::ReplyEvent::Settled { open: false },
                ));
            });
        }));
    }

    /// Next unrecognised: show the Reply view at the next unresolved token.
    pub(crate) fn next_unrecognised(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_reply();
        let Some(range) = self.reply.next_unresolved() else {
            return;
        };
        if !self.showing_reply() {
            self.set_view_mode(Some(CompareMode::Reply), window, cx);
        }
        let id = UNRESOLVED_ID | range.start as u64;
        if let Some(editor) = self.compare.as_ref().map(|c| c.reply_editor().clone()) {
            editor.update(cx, |editor, cx| editor.set_active_annotation(Some(id), cx));
        }
        self.reveal_in_reply(range.start, cx);
        cx.notify();
    }

    /// The reply editor's annotations: restored values in the original's
    /// gold, unresolved tokens red as in the restore tab.
    pub(in crate::review) fn reply_annotations(&self) -> Vec<mdoc_editor::SourceAnnotation> {
        reply_marks(self.reply.restored(), self.theme.get())
    }

    /// Focus the box (a click on its padding).
    pub(crate) fn focus_reply_box(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.reply_box() {
            window.focus(&editor.read(cx).focus_handle(cx), cx);
        }
    }
}

/// How long the box's text rests before it is saved, as a document's.
const REPLY_SETTLE: std::time::Duration = std::time::Duration::from_millis(700);
