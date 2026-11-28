//! The AI return journey: Copy for AI adds a placeholder notice; Paste AI
//! Response saves the reply to the Vault and shows it restored on its Reply
//! page (`workspace::replies`); the clipboard actions.
use super::text;
use crate::{
    document::{DocumentView, Inspector},
    review::refusal::{Refusal, Step},
    review::{CopyForAi, KEPT_ID, PiiApplyAll},
    shelf::Readiness,
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
        self.copy_refusal = None;
        self.copied_for_ai = true;
        self.copied_reply = false;
        self.show_copy_feedback(cx);
    }
    /// The outbound check refused it: nothing is copied, and the "Not
    /// copied" notice says why, with the step that resolves it.
    pub(crate) fn copy_for_ai_refused(&mut self, refusal: Refusal, cx: &mut Context<Self>) {
        self.copy_refusal = Some(refusal);
        cx.notify();
    }
    /// The step Copy for AI would need first, as the Share tab's button
    /// shows it before the click; `None` when it copies (the Vault's check
    /// still decides) or can't run yet.
    pub(crate) fn copy_step(&self, cx: &App) -> Option<Step> {
        if !self.can_copy_markdown()
            || self.lifecycle.source_only()
            || self.pii.scanning()
            || self.editor.read(cx).text().trim().is_empty()
        {
            return None;
        }
        let review = &self.pii.review;
        if review.remaining() > 0 {
            return Some(Step::ReplaceAndCopy(review.remaining()));
        }
        if review.applied().is_empty() {
            // Every proposal kept as written: nothing to replace, nothing to
            // find (Vault & Projects item 39).
            // Scanned with nothing found, it may leave as it is.
            let scanned = !self.pii.scans().is_empty() || self.session.document.scanned;
            return (review.variants().is_empty() && !scanned).then_some(Step::FindNames);
        }
        (self.names_not_replaced(cx) > 0).then_some(Step::Review)
    }
    /// Why Copy for AI refuses a tab with no applied alias.
    pub(crate) fn no_alias_refusal(&self) -> Refusal {
        if self.pii.scanning() {
            return Refusal::of(Readiness::Checking, true, false);
        }
        let review = &self.pii.review;
        Refusal::nothing_applied(review.remaining(), !review.variants().is_empty())
    }
    /// Run a refusal's step. "Replace N and copy" replaces the pending names
    /// as Apply does, then asks Copy for AI, whose check decides what
    /// leaves; "Review" opens the review at its first proposal; "Find names"
    /// scans as Pseudonymize does. Returns whether the step found what it
    /// shows: "Review" with no proposal yet (a document still loading its
    /// Project's names) may be asked again.
    pub(crate) fn run_copy_step(
        &mut self,
        step: Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.copy_refusal = None;
        let shown = match step {
            // A button drawn for another count (a notice shown before more
            // names were marked) replaces nothing: it shows the count now.
            Step::ReplaceAndCopy(shown) if shown != self.pii.review.remaining() => {
                let pending = self.pii.review.remaining();
                self.copy_refusal =
                    Some(Refusal::of(Readiness::LeftToReview(pending), true, false));
                true
            }
            Step::ReplaceAndCopy(_) => {
                self.pii.reviewing = true;
                self.apply_all_pii(&PiiApplyAll, window, cx);
                self.copy_for_ai(&CopyForAi, window, cx);
                true
            }
            Step::Review => {
                self.show_review(window, cx);
                self.select_first_proposal(window, cx) || self.select_first_kept(window, cx)
            }
            Step::FindNames => {
                // Aliases a reopened document relinks as applied are no scan.
                if !self.pii.scanning() {
                    self.start_pii_scan(cx);
                }
                self.show_review(window, cx);
                true
            }
        };
        cx.notify();
        shown
    }
    /// Open the popup on the earliest mention of a known name kept as
    /// written: Replace or Not a name is its way out (a Keep refuses).
    /// Returns whether there was one.
    fn select_first_kept(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.pii.reviewing = true;
        let Some(start) = (self.kept_entries().into_iter())
            .filter(|(_, _, identity)| identity.is_some())
            .map(|(range, _, _)| range.start)
            .min()
        else {
            return false;
        };
        self.editor
            .update(cx, |editor, cx| editor.set_cursor(start, cx));
        self.activate_annotation(KEPT_ID | start as u64, window, cx);
        self.active_is_kept()
    }
    /// A scan was asked for: running, waiting on its model's setup, failed,
    /// or done. For tests.
    #[cfg(test)]
    pub(crate) fn scan_asked(&self) -> bool {
        self.pii.scanning()
            || self.pii.setup
            || self.pii.error.is_some()
            || !self.pii.review.variants().is_empty()
    }
    /// The Inspector open on Review.
    fn show_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inspector = Inspector::Review;
        if !self.replacements_open() {
            self.toggle_replacements_panel(window, cx);
        }
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
            self.show_clipboard_notice(text::NO_NAMES_TO_REPLACE.into(), cx);
            cx.emit(crate::workspace::TabEvent::Project(
                crate::workspace::ProjectEvent::MappingMissing,
            ));
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.show_clipboard_notice(text::CLIPBOARD_HAS_NO_TEXT.into(), cx);
            return;
        };
        let (replaced, count) = RestoreMap::new(identities).pseudonymize(&text);
        if count > 0 {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(replaced));
        }
        let source = self.project.link().map_or_else(
            || text::THIS_DOCUMENT.to_string(),
            |l| format!("“{}”", l.name),
        );
        let notice = text::pseudonymized_clipboard(count, &source);
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
            self.show_clipboard_notice(text::NO_NAMES_TO_RESTORE.into(), cx);
            cx.emit(crate::workspace::TabEvent::Project(
                crate::workspace::ProjectEvent::MappingMissing,
            ));
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.show_clipboard_notice(text::CLIPBOARD_HAS_NO_TEXT.into(), cx);
            return;
        };
        let restored = crate::shelf::restore_clipboard(&text, identities);
        if restored.text != text {
            cx.write_to_clipboard(crate::review::restored_clipboard::item(
                restored.text.clone(),
            ));
        }
        let source = self.project.link().map_or_else(
            || text::THIS_DOCUMENT.to_string(),
            |l| format!("“{}”", l.name),
        );
        let notice = text::restored_clipboard(restored.restored, restored.unrecognised, &source);
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
        if !self.can_copy_markdown()
            || !self.has_identities_to_restore(text::RESTORING_AN_AI_REPLY, cx)
        {
            return;
        }
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        if text.trim().is_empty() {
            self.error = Some(text::CLIPBOARD_HAS_NO_REPLY.into());
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
        self.error = Some(text::needs_aliases(action));
        cx.emit(crate::workspace::TabEvent::Project(
            crate::workspace::ProjectEvent::MappingMissing,
        ));
        cx.notify();
        false
    }

    /// Share's copy button: the step a refusal needs, or Copy for AI.
    pub(super) fn share_copy(
        &mut self,
        step: Option<Step>,
        copy_for_ai: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match step {
            Some(step) => cx.emit(crate::workspace::TabEvent::CopyStep(step)),
            None if copy_for_ai => self.copy_for_ai(&CopyForAi, window, cx),
            None => {}
        }
    }
}
