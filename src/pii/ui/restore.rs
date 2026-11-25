//! The AI return journey: Copy for AI adds a placeholder notice; Paste AI
//! Response saves the reply to the Vault and shows it restored on its Reply
//! page (`workspace::saved_replies`); the clipboard actions.
use crate::{
    document_view::DocumentView,
    pii::ui::CopyForAi,
    workspace::{PasteAiResponse, PseudonymizeClipboard, RestoreClipboard},
};
use gpui::{App, Context, Window};
use mdoc_pii::restore::{ExportedIdentity, RestoreMap};

/// Unresolved-token annotations: bit 62 plus the token's start offset.
pub(crate) const UNRESOLVED_ID: u64 = 1 << 62;

/// How long the Pseudonymize Clipboard result stays on the document.
pub(crate) const CLIPBOARD_NOTICE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// The result of Pseudonymize Clipboard. It is transient (issue #181): it
/// times out, and goes sooner when dismissed, when the document is edited
/// or when a copy replaces the clipboard it describes.
#[derive(Default)]
pub(crate) struct ClipboardNotice {
    text: Option<String>,
    /// Dropping the task cancels the timeout.
    timeout: Option<gpui::Task<()>>,
}

impl ClipboardNotice {
    pub(crate) fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
    pub(crate) fn clear(&mut self) {
        self.text = None;
        self.timeout = None;
    }
}

impl DocumentView {
    /// Applied aliases are in the text, so Copy for AI and Paste AI Response apply.
    pub(crate) fn has_applied_aliases(&self) -> bool {
        !self.pii.review.applied().is_empty()
    }
    /// Every name the tab knows: its mapping, its Project's names and every
    /// variant, pending and kept ones included. Any of them left in the
    /// text keeps the tab from being ready.
    pub(crate) fn known_identities(&self) -> Vec<ExportedIdentity> {
        let review = &self.pii.review;
        let mut names = review.export_identities();
        names.extend(self.project.names().iter().cloned());
        names.extend(review.variants().iter().map(|variant| ExportedIdentity {
            alias: variant.replacement.clone(),
            previous: Vec::new(),
            category: variant.category,
            custom: false,
            originals: vec![variant.original.clone()],
        }));
        names
    }
    /// How many known names the text still holds as written: what keeps an
    /// applied tab from being ready, as the shelf counts it.
    pub(crate) fn names_not_replaced(&self, cx: &App) -> usize {
        RestoreMap::new(self.known_identities()).known_names(self.editor.read(cx).text())
    }
    /// The current text after the placeholder note: what Copy for AI, the
    /// shelf and a drag hand out once the Vault's check of it passes.
    #[cfg(test)]
    pub(crate) fn text_for_ai(&self, cx: &App) -> String {
        let aliases = self
            .pii
            .review
            .applied()
            .iter()
            .map(|applied| applied.step.after.as_ref());
        mdoc_pii::handoff::for_ai(self.editor.read(cx).text(), aliases)
    }
    /// Copy for AI (the Share tab, the menu, the shortcut): the window
    /// checks the text as the shelf does, against the names its Project
    /// holds now, and copies only what the Vault's outbound check passes
    /// (#1091 Decision 19). Copy Markdown stays the deliberate raw copy.
    pub(crate) fn copy_for_ai(&mut self, _: &CopyForAi, _: &mut Window, cx: &mut Context<Self>) {
        if !self.can_copy_markdown() {
            return;
        }
        // Edited text is scanned before anything leaves; the copy waits.
        self.scan_edits_now(cx);
        cx.emit(crate::workspace::TabEvent::CopyForAi);
    }
    /// The outbound check passed: `text` is what leaves.
    pub(crate) fn copy_checked_for_ai(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        self.copied_for_ai = true;
        self.copied_reply = false;
        self.show_copy_feedback(cx);
    }
    /// The outbound check refused it: nothing is copied, and `why` says so.
    pub(crate) fn copy_for_ai_refused(&mut self, why: String, cx: &mut Context<Self>) {
        self.error = Some(why);
        cx.notify();
    }
    /// Replace the clipboard text with this tab's aliases:
    /// the Project's names, or the tab's own mapping. Exact known names only.
    pub(crate) fn pseudonymize_clipboard(
        &mut self,
        _: &PseudonymizeClipboard,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let identities = if self.project.link().is_some() {
            self.project.names().to_vec()
        } else {
            self.pii.review.export_identities()
        };
        if identities.is_empty() {
            self.show_clipboard_notice(
                "This tab has no names to replace yet. Pseudonymize a document or open a Project first."
                    .into(),
                cx,
            );
            cx.emit(crate::workspace::TabEvent::Project(
                crate::workspace::ProjectEvent::MappingMissing,
            ));
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.show_clipboard_notice("The clipboard has no text.".into(), cx);
            return;
        };
        let (replaced, count) = RestoreMap::new(identities).pseudonymize(&text);
        if count > 0 {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(replaced));
        }
        let source = self
            .project
            .link()
            .map_or_else(|| "this document".to_string(), |l| format!("“{}”", l.name));
        let notice = match count {
            0 => format!("No names from {source} were found in the clipboard. It is unchanged."),
            n => format!(
                "Replaced {n} {} from {source} in the clipboard. Names mdoc doesn't know yet \
                 were not replaced; check before pasting.",
                if n == 1 { "name" } else { "names" }
            ),
        };
        self.show_clipboard_notice(notice, cx);
    }
    /// Edit › Restore Clipboard: the shelf's Restore with this
    /// tab's names, its Project's or its live mapping's.
    pub(crate) fn restore_clipboard(
        &mut self,
        _: &RestoreClipboard,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let identities = if self.project.link().is_some() {
            self.project.names().to_vec()
        } else {
            self.pii.review.export_identities()
        };
        if identities.is_empty() {
            self.show_clipboard_notice(
                "This tab has no names to restore yet. Pseudonymize a document or open a Project first."
                    .into(),
                cx,
            );
            cx.emit(crate::workspace::TabEvent::Project(
                crate::workspace::ProjectEvent::MappingMissing,
            ));
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.show_clipboard_notice("The clipboard has no text.".into(), cx);
            return;
        };
        let restored = crate::shelf::restore_clipboard(&text, identities);
        if restored.text != text {
            cx.write_to_clipboard(crate::restored_clipboard::item(restored.text.clone()));
        }
        let source = self
            .project
            .link()
            .map_or_else(|| "this document".to_string(), |l| format!("“{}”", l.name));
        let plural =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let notice = match (restored.restored, restored.unrecognised) {
            (0, 0) => {
                format!("No aliases from {source} were found in the clipboard. It is unchanged.")
            }
            (n, 0) => format!(
                "Restored {} from {source} in the clipboard.",
                plural(n, "name", "names")
            ),
            (n, m) => format!(
                "Restored {} from {source} in the clipboard; {} unrecognized, left as is.",
                plural(n, "name", "names"),
                plural(m, "alias", "aliases")
            ),
        };
        self.show_clipboard_notice(notice, cx);
    }
    /// Show `text` as the clipboard notice; showing another restarts the timeout.
    fn show_clipboard_notice(&mut self, text: String, cx: &mut Context<Self>) {
        self.clipboard_notice.text = Some(text);
        self.clipboard_notice.timeout = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(CLIPBOARD_NOTICE_TIMEOUT)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.clipboard_notice.clear();
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(crate) fn paste_ai_response(
        &mut self,
        _: &PasteAiResponse,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The reply comes from the menu bar, ⌥⇧⌘V, the More menu or Home's
        // Restore AI reply, each with its own name, so the error names none.
        if !self.can_copy_markdown() || !self.has_identities_to_restore("Restoring an AI reply", cx)
        {
            return;
        }
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        if text.trim().is_empty() {
            self.error = Some("The clipboard has no text. Copy the AI's reply first.".into());
            cx.notify();
            return;
        }
        // Saved to the Vault, then shown on its Reply page.
        cx.emit(crate::workspace::TabEvent::Reply(
            crate::workspace::ReplyEvent::Pasted(text),
        ));
    }
    /// Restoring needs identities: without any, `action` explains why and
    /// a locked Vault gets its banner. Returns whether there are some.
    pub(crate) fn has_identities_to_restore(
        &mut self,
        action: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.pii.review.export_identities().is_empty() {
            return true;
        }
        self.error = Some(format!(
            "{action} needs a document whose names were replaced with aliases. \
             Apply replacements, copy the text for the AI, then paste its reply here."
        ));
        cx.emit(crate::workspace::TabEvent::Project(
            crate::workspace::ProjectEvent::MappingMissing,
        ));
        cx.notify();
        false
    }
}
