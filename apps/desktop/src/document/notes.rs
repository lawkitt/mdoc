//! A document's private notes (#1091 Decision 3), in the Inspector's Notes
//! tab: kept with the Vault document, saved as the text is once typing
//! pauses, and never part of what leaves for an AI. Also the header's
//! "From AI reply" line of a document promoted from a saved reply.
use super::{AUTOSAVE_DELAY, DocumentView, Inspector};
use crate::document::text;
use crate::{ui, workspace};
use gpui::{
    AnyElement, Context, Entity, Focusable, Subscription, Task, Window, div, prelude::*, px,
};
use mdoc_editor::EditorState;

/// What the Notes tab holds.
#[derive(Default)]
pub(crate) struct NotesState {
    /// The Vault document these are the notes of.
    document: Option<String>,
    /// The notes as they are now: the editor's text once it exists.
    text: String,
    /// As last loaded or written.
    saved: String,
    /// Handed to the writer and not written yet.
    pending: Option<String>,
    /// Writes of these notes that ended, so a load that started before one
    /// does not put older notes back.
    writes: u64,
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

    /// The notes differ from what the Vault holds.
    pub(crate) fn unsaved(&self) -> bool {
        self.text != self.saved
    }

    /// The notes differ from what was last handed to the writer, or
    /// written: a write is due. An undo back to the written text while a
    /// newer one waits is one too.
    pub(crate) fn needs_write(&self) -> bool {
        self.text != *self.pending.as_ref().unwrap_or(&self.saved)
    }

    /// The writer took `text`.
    pub(crate) fn queued(&mut self, text: String) {
        self.pending = Some(text);
        self.save = None;
    }

    /// `written` reached the Vault.
    pub(crate) fn mark_saved(&mut self, written: String) {
        if self.pending.as_ref() == Some(&written) {
            self.pending = None;
        }
        self.saved = written;
        self.writes += 1;
    }

    /// The write handed over failed: the notes are due again.
    pub(crate) fn write_failed(&mut self) {
        self.pending = None;
        self.writes += 1;
    }

    /// Writes that ended so far; a load passes back what it read then.
    pub(crate) fn writes(&self) -> u64 {
        self.writes
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

impl DocumentView {
    /// Vault document `document` loaded with `notes`, read when `writes`
    /// writes of its notes had ended. Another document's notes replace
    /// these; the same document's show unless notes typed here wait to be
    /// written (a move's reload), or a write ended since the read, whose
    /// notes are newer.
    pub(crate) fn load_notes(
        &mut self,
        document: &str,
        notes: String,
        writes: u64,
        cx: &mut Context<Self>,
    ) {
        let same = self.notes.document.as_deref() == Some(document);
        if same && (self.notes.writes != writes || self.notes.pending.is_some()) {
            return;
        }
        let typed = same && self.notes.unsaved();
        if !same {
            self.notes.document = Some(document.to_string());
            self.notes.save = None;
        }
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
            let mut editor = EditorState::new(window, cx).with_placeholder(text::EMPTY_HINT);
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
        if !self.notes.needs_write() {
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

    /// More › Notes: the Inspector on its Notes tab, the editor focused.
    /// Opening it there starts no review.
    pub(crate) fn show_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inspector = Inspector::Notes;
        if !self.replacements_open() {
            self.open_inspector(window, cx);
        }
        self.ensure_notes_editor(window, cx);
        if let Some(editor) = self.notes.editor() {
            window.focus(&editor.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// "From AI reply" (the header, the palette, the menu bar): the reply
    /// on its page, while it exists.
    pub(crate) fn open_source_reply(&mut self, cx: &mut Context<Self>) {
        if let Some(from) = self.from_reply.as_ref().filter(|from| !from.gone) {
            cx.emit(workspace::TabEvent::Reply(workspace::ReplyEvent::Open(
                from.id.clone(),
            )));
        }
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
                        muted(text::NOTES_PRIVATE)
                            .when(cfg!(test), |v| v.debug_selector(|| "notes-private".into())),
                    )
                    .child(
                        // The editor tracks its own handle; the box rings it.
                        ui::focus_ring("notes-box", theme)
                            .ring_for(&focus)
                            .aria_label(text::NOTES)
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
            None => muted(text::NOTES_UNAVAILABLE)
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
                    .child(text::FROM_A_DELETED_REPLY)
                    .into_any_element(),
            );
        }
        Some(
            ui::control("from-reply", text::FROM_AI_REPLY, theme, true)
                .when(cfg!(test), |v| v.debug_selector(|| "from-reply".into()))
                .aria_label(text::OPEN_THE_REPLY)
                .text_size(px(12.))
                .text_color(theme.search_accent())
                .on_click(cx.listener(|this, _, _, cx| this.open_source_reply(cx)))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::NotesState;

    fn typed(state: &mut NotesState, text: &str) {
        state.text = text.into();
    }

    /// Notes typed back to what was written while newer ones wait in the
    /// writer are due again, so the newer ones do not win.
    #[test]
    fn an_undo_back_to_the_written_notes_while_newer_wait_is_written() {
        let mut state = NotesState {
            saved: "A".into(),
            ..NotesState::default()
        };
        typed(&mut state, "A");
        assert!(!state.needs_write());
        typed(&mut state, "B");
        assert!(state.needs_write());
        state.queued("B".into());
        assert!(!state.needs_write());
        typed(&mut state, "A");
        assert!(state.needs_write(), "A must be written after B");
        state.mark_saved("B".into());
        assert!(state.needs_write() && state.unsaved());
        state.queued("A".into());
        state.mark_saved("A".into());
        assert!(!state.needs_write() && !state.unsaved());
        assert_eq!(state.writes(), 2);
    }

    /// A failed write makes the notes due again and counts as an end.
    #[test]
    fn a_failed_write_is_due_again() {
        let mut state = NotesState::default();
        typed(&mut state, "B");
        state.queued("B".into());
        state.write_failed();
        assert!(state.needs_write() && state.unsaved());
        assert_eq!(state.writes(), 1);
    }
}
