//! Copy Markdown: the whole text to the clipboard, and the Copied feedback
//! that follows a copy.
use super::{CopyMarkdown, DocumentView};
use gpui::{Context, Window};

impl DocumentView {
    pub(crate) fn can_copy_markdown(&self) -> bool {
        !self.lifecycle.loading() && !self.lifecycle.unavailable() && !self.lifecycle.source_only()
    }

    pub(crate) fn copy_markdown(
        &mut self,
        _: &CopyMarkdown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy_markdown() {
            return;
        }
        // The whole text, copied by hand: marked unless all of it is clean.
        let all = 0..self.editor.read(cx).text().len();
        cx.write_to_clipboard(self.hand_copy_item(all, cx));
        self.copied_for_ai = false;
        self.copied_reply = false;
        self.show_copy_feedback(cx);
    }

    pub(crate) fn show_copy_feedback(&mut self, cx: &mut Context<Self>) {
        // Replacing the task restarts feedback; typing and document transitions cancel it.
        // The copy replaces the clipboard a Pseudonymize Clipboard notice described.
        self.clipboard_notice.clear();
        self.copy_feedback = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.copy_feedback = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
}
