//! A selection the user copies by hand (#1209, ADR 0109 as amended): the
//! user's own copy, never blocked and not an exit (`crate::exits`). A copy
//! of the document's text, in Edit, Original, Pseudonymized or Side by
//! side, is marked as a restored reply's copy is (`crate::restored_clipboard`)
//! unless the core finds it clean (`mdoc_pii::Review::check_selection`):
//! the document was reviewed and that text has not changed since its scan,
//! nothing is pending in it, and no name the tab knows is written out in
//! it. ⌘C stays silent. When proposals are pending in it, the right-click
//! menu offers "Replace N and Copy" in Edit and the Pseudonymized column.
use crate::document_view::DocumentView;
use gpui::{App, ClipboardItem, Context, WeakEntity};
use mdoc_editor::CopyCheck;
use mdoc_pii::{
    decisions::{Decision, Mentions},
    restore::RestoreMap,
    selection::SelectionCheck,
};
use std::ops::Range;

/// The text a copy is taken from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyFrom {
    /// The editor: Edit, or the whole text's Copy Markdown.
    Editor,
    /// The Original column: the editor's text with its applied names restored.
    Original,
    /// The Pseudonymized column: the editor's text, read-only.
    Pseudonymized,
}

/// A copy not known to be clean.
fn marked() -> CopyCheck {
    CopyCheck {
        metadata: Some(crate::restored_clipboard::TAG.into()),
        action: None,
    }
}

/// Every copy from `editor`, which shows `from`'s text of `view`, is
/// checked as a hand copy; marked once the view is gone.
pub(crate) fn check_hand_copies(
    editor: &mut mdoc_editor::EditorState,
    view: WeakEntity<DocumentView>,
    from: CopyFrom,
) {
    editor.on_copy_check(move |text, range, cx| match view.upgrade() {
        Some(view) => view.read(cx).hand_copy_check(from, text, range, cx),
        None => marked(),
    });
}

impl DocumentView {
    /// What a copy of `range` of `text`, the text `from` shows, carries:
    /// marked unless clean, and "Replace N and Copy" while N proposals are
    /// pending in it. Runs inside an editor's copy, so it reads the editor
    /// only for the compare columns, whose text is not the editor's own.
    pub(crate) fn hand_copy_check(
        &self,
        from: CopyFrom,
        text: &str,
        range: Range<usize>,
        cx: &App,
    ) -> CopyCheck {
        let check = match from {
            CopyFrom::Editor => self.selection_check(text, range),
            CopyFrom::Pseudonymized if self.editor.read(cx).text() == text => {
                self.selection_check(text, range)
            }
            CopyFrom::Pseudonymized => None,
            CopyFrom::Original => {
                let editor = self.editor.read(cx);
                let now = (editor.revision(), self.pii.review.revision());
                // A restored name in it is written out: always marked.
                let Some(range) = (self.compare.as_ref())
                    .and_then(|state| state.original_in_editor(text, range, now))
                else {
                    return marked();
                };
                return match self.selection_check(editor.text(), range) {
                    Some(SelectionCheck::Clean) => CopyCheck::default(),
                    _ => marked(),
                };
            }
        };
        match check {
            Some(SelectionCheck::Clean) => CopyCheck::default(),
            Some(SelectionCheck::NotClean(not)) if not.proposals > 0 && self.can_replace() => {
                CopyCheck {
                    action: Some(format!("Replace {} and Copy", not.proposals).into()),
                    ..marked()
                }
            }
            _ => marked(),
        }
    }

    /// What `range` of the editor's `text` holds; `None` while that can't
    /// be known (loading, or a scan running).
    fn selection_check(&self, text: &str, range: Range<usize>) -> Option<SelectionCheck> {
        if !self.lifecycle.ready() || self.pii.scanning() {
            return None;
        }
        let review = &self.pii.review;
        // Reviewed as the shelf reads a tab (`shelf_source::open_tab`), so
        // a Ready document copies clean.
        let reviewed = !review.applied().is_empty()
            || !self.pii.scans().is_empty()
            || self.session.document.scanned;
        let known = RestoreMap::new(self.known_identities());
        review.check_selection(text, range, reviewed, &known).ok()
    }

    /// Replacing is allowed now, as for Replace all.
    fn can_replace(&self) -> bool {
        !self.pii.scanning() && self.can_copy_markdown()
    }

    /// The clipboard item for a hand copy of `range` of the editor's text.
    pub(crate) fn hand_copy_item(&self, range: Range<usize>, cx: &App) -> ClipboardItem {
        let editor = self.editor.read(cx);
        let text = editor.text();
        let copy = text[range.clone()].to_owned();
        match self
            .hand_copy_check(CopyFrom::Editor, text, range, cx)
            .metadata
        {
            Some(metadata) => ClipboardItem::new_string_with_metadata(copy, metadata),
            None => ClipboardItem::new_string(copy),
        }
    }

    /// The menu's "Replace N and Copy" (`label`) on `range` of the text
    /// `from` shows (`editor`): replaced and copied only while the check
    /// still offers that label, so a menu left open while proposals
    /// changed replaces nothing it did not name.
    pub(crate) fn copy_action(
        &mut self,
        from: CopyFrom,
        editor: &gpui::Entity<mdoc_editor::EditorState>,
        range: Range<usize>,
        label: &str,
        cx: &mut Context<Self>,
    ) {
        let text = editor.read(cx).text().to_owned();
        let offered = self.hand_copy_check(from, &text, range.clone(), cx).action;
        if offered.as_deref() == Some(label) {
            self.replace_and_copy(range, cx);
        }
    }

    /// "Replace N and Copy" on `range` of the editor's text: its proposals
    /// replaced as Replace all replaces them (`Review::selection_plans`),
    /// with the aliases there whose identity was renamed since, as one
    /// undo step, then that selection, widened over a proposal across its
    /// edge, copied and marked unless now clean. Silent, as ⌘C is.
    fn replace_and_copy(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        if !self.can_replace() {
            return;
        }
        let editor = self.editor.read(cx);
        let (source, revision, before) = (
            editor.text().to_owned(),
            editor.revision(),
            editor.history_id(),
        );
        let planned = match self.pii.review.selection_plans(&source, range) {
            Ok(planned) => planned,
            Err(error) => {
                self.pii.error = Some(error);
                cx.notify();
                return;
            }
        };
        if !planned.plans.is_empty() {
            let review = &self.pii.review;
            // A proposal's own wording; a corrected alias's original.
            let originals: Vec<_> = (planned.plans.iter())
                .map(|plan| {
                    (review.applied().iter())
                        .find(|applied| applied.range == plan.range)
                        .map_or_else(
                            || plan.before.clone(),
                            |applied| applied.step.original_shared().clone(),
                        )
                })
                .collect();
            let count = originals.len();
            let Some(after) = self.commit_plans(revision, &planned.plans, None, cx) else {
                return;
            };
            self.record_decision(before, Decision::Apply(Mentions::of(originals)), cx);
            self.pii.mapping.record_applied(count, after);
            self.flash_notice(cx);
            self.pii.reviewing = true;
            self.pii.review.refresh(self.editor.read(cx).text());
            self.review_changed(cx);
        }
        self.editor
            .update(cx, |editor, cx| editor.set_selection(planned.after, cx));
        let range = self.editor.read(cx).copy_selection_range();
        cx.write_to_clipboard(self.hand_copy_item(range, cx));
        cx.notify();
    }
}
