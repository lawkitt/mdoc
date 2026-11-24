//! A document's private notes (#1091 Decision 3), in the Inspector's Notes
//! tab: kept with the Vault document, saved as the text is once typing
//! pauses, and never part of what leaves for an AI. Also the header's
//! "From AI reply" line of a document promoted from a saved reply.
use super::{AUTOSAVE_DELAY, DocumentView, Inspector};
use crate::{ui, workspace};
use gpui::{
    AnyElement, Context, Entity, Focusable, Subscription, Task, Window, div, prelude::*, px,
};
use mdoc_editor::EditorState;

/// What the Notes tab holds.
#[derive(Default)]
pub(crate) struct NotesState {
    /// The notes as they are now: the editor's text once it exists.
    text: String,
    /// As last loaded or written.
    saved: String,
    /// Made when the Notes tab first shows.
    editor: Option<(Entity<EditorState>, Subscription)>,
    scroll: gpui::ScrollHandle,
    /// The waiting autosave.
    save: Option<Task<()>>,
}

impl NotesState {
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The notes changed since they were last written.
    pub(crate) fn unsaved(&self) -> bool {
        self.text != self.saved
    }

    /// `written` reached the Vault.
    pub(crate) fn mark_saved(&mut self, written: String) {
        self.saved = written;
    }

    pub(crate) fn editor(&self) -> Option<&Entity<EditorState>> {
        self.editor.as_ref().map(|(editor, _)| editor)
    }
}

/// The AI Reply a document was promoted from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FromReply {
    pub(crate) id: String,
    /// Deleted since: the line stays, without a way to open it.
    pub(crate) gone: bool,
}

/// What the tab says with no notes to edit yet.
pub(crate) const EMPTY_HINT: &str =
    "Notes about this document: deadlines, who to call, what to check. Only you see them.";

impl DocumentView {
    /// The document loaded with `notes`: they show, unless notes typed
    /// here wait to be written (a move's reload), which stay.
    pub(crate) fn load_notes(&mut self, notes: String, cx: &mut Context<Self>) {
        let typed = self.notes.unsaved();
        self.notes.saved = notes.clone();
        if typed {
            return;
        }
        self.notes.text = notes.clone();
        if let Some(editor) = self.notes.editor().cloned()
            && editor.read(cx).text() != notes
        {
            editor.update(cx, |editor, cx| editor.set_text(notes, cx));
        }
    }

    /// Notes are kept only for a document in the Vault.
    pub(crate) fn notes_available(&self) -> bool {
        self.session.document.vault.is_some()
    }

    /// The notes editor, made when the Notes tab first shows: plain text,
    /// kept exactly as typed, in the panel's Tab order.
    pub(crate) fn ensure_notes_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.notes.editor.is_some() || !self.notes_available() {
            return;
        }
        let text = self.notes.text.clone();
        let scroll = self.notes.scroll.clone();
        let editor = cx.new(|cx| {
            let mut editor = EditorState::new(window, cx).with_placeholder(EMPTY_HINT);
            editor.set_plain_text(true);
            editor.set_autoscroll(scroll);
            editor.set_text(text, cx);
            editor.focus_handle(cx).tab_index(0).tab_stop(true);
            editor
        });
        let subscription = cx.subscribe_in(&editor, window, |this, editor, event, _, cx| {
            if let mdoc_editor::EditorEvent::Changed = event {
                this.notes.text = editor.read(cx).text().to_owned();
                this.schedule_notes_save(cx);
                cx.notify();
            }
        });
        self.notes.editor = Some((editor, subscription));
    }

    /// The notes changed: save them once typing pauses, as the text is.
    fn schedule_notes_save(&mut self, cx: &mut Context<Self>) {
        if !self.notes.unsaved() {
            self.notes.save = None;
            return;
        }
        self.notes.save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTOSAVE_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                this.notes.save = None;
                cx.emit(workspace::TabEvent::Save(workspace::SaveEvent::Changed));
            });
        }));
    }

    /// The writer took the notes: no save waits any more.
    pub(crate) fn notes_queued(&mut self) {
        self.notes.save = None;
    }

    /// More › Notes: the Inspector on its Notes tab, the editor focused.
    pub(crate) fn show_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inspector = Inspector::Notes;
        if !self.replacements_open() {
            self.toggle_replacements_panel(window, cx);
        }
        self.inspector = Inspector::Notes;
        self.ensure_notes_editor(window, cx);
        if let Some(editor) = self.notes.editor() {
            window.focus(&editor.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// Type `text` as the notes (tests and headless renders).
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn type_notes(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_notes_editor(window, cx);
        if let Some(editor) = self.notes.editor().cloned() {
            editor.update(cx, |editor, cx| editor.set_text(text.to_owned(), cx));
        }
        // Set by hand, the text sends no edit: count it as typed.
        self.notes.text = text.to_owned();
        self.schedule_notes_save(cx);
    }

    /// The Inspector's Notes tab.
    pub(crate) fn notes_panel(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let entities = self.replacement_entity_count(cx);
        let muted = |text: &'static str| {
            div()
                .text_size(px(12.))
                .text_color(p.header_muted)
                .child(text)
        };
        let body = match self.notes.editor().cloned() {
            Some(editor) => {
                let focus = editor.read(cx).focus_handle(cx);
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        muted("Private to this computer. Never sent with Copy for AI or Share.")
                            .when(cfg!(test), |v| v.debug_selector(|| "notes-private".into())),
                    )
                    .child(
                        // The editor tracks its own handle; the box rings it.
                        ui::focus_ring("notes-box", theme)
                            .ring_for(&focus)
                            .aria_label("Notes")
                            .when(cfg!(test), |v| v.debug_selector(|| "notes-box".into()))
                            .key_context("NotesBox")
                            .flex_1()
                            .min_h(px(120.))
                            .overflow_y_scroll()
                            .track_scroll(&self.notes.scroll)
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .border_1()
                            .border_color(p.border)
                            .bg(p.bg)
                            .text_size(px(13.))
                            .cursor_text()
                            .on_click(move |_, window, cx| window.focus(&focus, cx))
                            .child(editor),
                    )
                    .into_any_element()
            }
            None => muted("Notes are kept once the document is saved in mdoc.")
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "notes-unavailable".into())
                })
                .into_any_element(),
        };
        ui::panel("identity-panel", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "identity-panel".into()))
            .m_2()
            .rounded_lg()
            .shadow_none()
            .overflow_hidden()
            .key_context("IdentityPanel UiPanel UiMenu")
            .tab_group()
            .tab_stop(false)
            .track_focus(&self.replacements_focus())
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                this.close_replacements(window, cx);
                cx.stop_propagation();
            }))
            .flex_shrink_0()
            .min_h_0()
            .flex()
            .flex_col()
            .text_size(px(13.))
            .w(self.replacements_panel_width(window) - px(16.))
            .child(self.inspector_tabs(entities, cx))
            .child(div().h(px(1.)).flex_shrink_0().bg(p.border))
            .child(
                div()
                    .id("notes-body")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .px_3()
                    .py_3()
                    .child(body),
            )
            .into_any_element()
    }

    /// The header's "From AI reply": opens the reply on its page, or says
    /// it was deleted.
    pub(crate) fn reply_origin_line(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let from = self.from_reply.clone()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        if from.gone {
            return Some(
                div()
                    .id("from-reply")
                    .when(cfg!(test), |v| v.debug_selector(|| "from-reply".into()))
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child("From a deleted AI reply")
                    .into_any_element(),
            );
        }
        Some(
            ui::control("from-reply", "From AI reply", theme, true)
                .when(cfg!(test), |v| v.debug_selector(|| "from-reply".into()))
                .aria_label("From AI reply: open the reply")
                .text_size(px(12.))
                .text_color(theme.search_accent())
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(workspace::TabEvent::Reply(workspace::ReplyEvent::Open(
                        from.id.clone(),
                    )))
                }))
                .into_any_element(),
        )
    }
}
