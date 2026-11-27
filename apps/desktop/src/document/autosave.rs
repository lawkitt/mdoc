//! A document saving itself: the save that waits for typing to pause, and
//! whether the text differs from what was last written.
use super::{AUTOSAVE_DELAY, DocumentView};
use crate::workspace;
use gpui::{App, Context};

impl DocumentView {
    /// Whether the text differs from what was last written: an autosave is
    /// due. Never shown; documents save themselves.
    pub(crate) fn unsaved(&self, cx: &App) -> bool {
        self.lifecycle.ready()
            && !self.lifecycle.source_only()
            && self.session.document.id().is_some()
            && self.session.unsaved(self.editor.read(cx).text())
    }

    /// The text changed: save it once typing pauses.
    pub(crate) fn schedule_autosave(&mut self, cx: &mut Context<Self>) {
        if !self.lifecycle.ready() || self.lifecycle.source_only() {
            return;
        }
        self.autosave = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTOSAVE_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                this.autosave = None;
                cx.emit(workspace::TabEvent::Save(workspace::SaveEvent::Changed));
            });
        }));
    }

    /// An autosave waits for typing to pause.
    pub(crate) fn save_scheduled(&self) -> bool {
        self.autosave.is_some()
    }

    /// Save: write now instead of after the pause.
    pub(crate) fn save_now(&mut self, cx: &mut Context<Self>) {
        self.autosave = None;
        cx.emit(workspace::TabEvent::Save(workspace::SaveEvent::Changed));
    }

    /// Drop the waiting autosave: the caller writes the text itself.
    pub(crate) fn cancel_autosave(&mut self) {
        self.autosave = None;
    }
}
