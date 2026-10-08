//! Live-document PII state, scan lifecycle and replacement actions.
mod chooser;
mod discovery;
mod mapping;
mod render;
mod scan;
use scan::ScanJob;
#[cfg(test)]
mod tests;

use crate::{
    PiiAddCandidate, PiiApplyAll, PiiClosePopup, PiiConfirm, PiiNextCandidate,
    PiiPreviousCandidate, PiiRestore, PiiRestoreAll, PiiReviewCandidate, Pseudonymize, Workspace,
    markdown_search,
    pii::detector,
    pii::{self, Category, IdentitySnapshot, Review, tracking::ReplacementPlan},
    settings, settings_ui, style,
};
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, Hsla, KeyBinding, Pixels, Window, px,
};
use std::{
    cell::Cell,
    ops::Range,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// The word popup edits the workspace selection; the chooser disambiguates
/// several hidden fields that share one visual row.
pub(crate) enum Popup {
    Selection,
    Choose { ids: Arc<[u64]>, selected: usize },
}
const APPLIED_ID: u64 = 1 << 63;
pub(crate) struct ReviewUi {
    pub review: Review,
    /// Candidate highlights are shown; Close review hides them.
    pub reviewing: bool,
    pub popup: Option<Popup>,
    pub focus: FocusHandle,
    pub category: Category,
    job: Option<ScanJob>,
    discovery_job: Option<discovery::DiscoveryJob>,
    discovery_pending: bool,
    pub scans: Vec<settings::PiiConfig>,
    pub error: Option<String>,
    mapping: mapping::MappingUi,
    generation: u64,
    popup_previous: Option<FocusHandle>,
    popup_scroll: gpui::ScrollHandle,
    chooser_scroll: gpui::UniformListScrollHandle,
    popup_controls: std::cell::RefCell<std::collections::HashMap<gpui::ElementId, FocusHandle>>,
}
impl Drop for ReviewUi {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl ReviewUi {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        Self {
            review: Review::default(),
            reviewing: false,
            popup: None,
            focus: cx.focus_handle(),
            category: Category::Person,
            job: None,
            discovery_job: None,
            discovery_pending: false,
            scans: Vec::new(),
            error: None,
            mapping: mapping::MappingUi::new(cx),
            generation: 0,
            popup_previous: None,
            popup_scroll: gpui::ScrollHandle::new(),
            chooser_scroll: gpui::UniformListScrollHandle::new(),
            popup_controls: Default::default(),
        }
    }
    fn cancel(&mut self) {
        if let Some(job) = &self.discovery_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.discovery_pending = false;
        if let Some(job) = self.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.generation = self.generation.wrapping_add(1);
    }
    fn scanning(&self) -> bool {
        self.job.is_some()
    }
    /// Open a popup, remembering where focus returns when it closes.
    fn show_popup(&mut self, popup: Popup, previous_focus: Option<FocusHandle>) {
        self.popup = Some(popup);
        self.popup_previous = previous_focus;
    }
    /// Re-show the selection popup after an edit, keeping the original return focus.
    fn keep_selection_popup(&mut self) {
        self.popup = Some(Popup::Selection);
    }
    /// Move the chooser highlight cyclically, returning the new index.
    fn step_choice(&mut self, backwards: bool) -> Option<usize> {
        let Some(Popup::Choose { ids, selected }) = &mut self.popup else {
            return None;
        };
        *selected = if backwards {
            selected.checked_sub(1).unwrap_or(ids.len() - 1)
        } else {
            (*selected + 1) % ids.len()
        };
        Some(*selected)
    }
    pub(crate) fn dismiss_popup(&mut self) {
        self.popup = None;
    }
    /// Close the popup, returning the focus it should restore.
    fn close_popup(&mut self) -> Option<FocusHandle> {
        self.popup = None;
        self.popup_previous.take()
    }
}

pub(crate) fn bind_keys(cx: &mut App) {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.bind_keys([
        KeyBinding::new("down", crate::PiiNextChoice, Some("PiiChooser")),
        KeyBinding::new("up", crate::PiiPreviousChoice, Some("PiiChooser")),
        KeyBinding::new("enter", crate::PiiOpenChoice, Some("PiiChooser")),
        KeyBinding::new(&format!("{modifier}-shift-p"), Pseudonymize, None),
        KeyBinding::new("alt-enter", PiiReviewCandidate, Some("Editor")),
        KeyBinding::new("alt-down", PiiNextCandidate, Some("Editor")),
        KeyBinding::new("alt-up", PiiPreviousCandidate, Some("Editor")),
        KeyBinding::new(
            &format!("{modifier}-alt-p"),
            PiiAddCandidate,
            Some("Editor"),
        ),
        KeyBinding::new(
            "enter",
            PiiConfirm,
            Some("PseudonymReview && !UiControl && !UiMenu"),
        ),
        KeyBinding::new("escape", PiiClosePopup, Some("PseudonymReview")),
    ]);
}

impl Workspace {
    pub(crate) fn reset_pii(&mut self, cx: &mut Context<Self>) {
        self.pii.cancel();
        self.pii.review = Review::default();
        self.pii.scans.clear();
        self.pii.dismiss_popup();
        self.pii.error = None;
        self.pii.mapping = mapping::MappingUi::new(cx);
        self.sync_annotations(cx);
    }
    pub(crate) fn pii_edited(&mut self, cx: &mut Context<Self>) {
        if !self.pii.reviewing && self.pii.review.variants().is_empty() && !self.pii.scanning() {
            return;
        }
        let was_scanning = self.pii.scanning();
        self.pii.cancel();
        if self.pii.mapping.popup_revision != Some(self.editor.read(cx).revision()) {
            self.pii.dismiss_popup();
            self.pii.mapping.invalidate_source_edit(cx);
            self.editor
                .update(cx, |e, cx| e.set_active_annotation(None, cx));
        }
        if was_scanning {
            self.pii.error = Some("Document changed; scan cancelled. Rescan when ready.".into());
        }
        self.schedule_pii_discovery(cx);
        self.sync_annotations(cx);
    }
    pub(crate) fn sync_pii_theme(&mut self, cx: &mut Context<Self>) {
        self.sync_annotations(cx);
    }
    fn sync_annotations(&mut self, cx: &mut Context<Self>) {
        let accent = style::markdown_style(self.theme.get()).alert_warning;
        let review = &self.pii.review;
        let show_candidates = self.pii.reviewing;
        let mut candidates = review
            .candidates()
            .iter()
            .filter(|_| show_candidates)
            .peekable();
        let mut applied = review.applied().iter().peekable();
        let mut annotations = Vec::with_capacity(
            if show_candidates {
                review.candidates().len()
            } else {
                0
            } + review.applied().len(),
        );
        // Both inputs already follow source order; ordinary edits need no new sort.
        while candidates.peek().is_some() || applied.peek().is_some() {
            let candidate_first = match (candidates.peek(), applied.peek()) {
                (Some(candidate), Some(applied)) => candidate.range.start <= applied.range.start,
                (Some(_), None) => true,
                _ => false,
            };
            annotations.push(if candidate_first {
                let occurrence = candidates.next().unwrap();
                mdoc_editor::SourceAnnotation {
                    id: occurrence.id,
                    range: occurrence.range.clone(),
                    color: Hsla { a: 0.14, ..accent },
                    active_color: Hsla { a: 0.3, ..accent },
                }
            } else {
                let occurrence = applied.next().unwrap();
                mdoc_editor::SourceAnnotation {
                    id: APPLIED_ID | occurrence.id,
                    range: occurrence.range.clone(),
                    color: Hsla { a: 0.1, ..accent },
                    active_color: Hsla { a: 0.24, ..accent },
                }
            });
        }
        self.editor.update(cx, |editor, cx| {
            editor.set_annotations(editor.revision(), annotations, cx)
        });
    }
    pub(crate) fn pseudonymize(
        &mut self,
        _: &Pseudonymize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_pii_scan(cx);
    }
    /// The toolbar scans once, then shows or hides the existing review.
    pub(crate) fn toggle_pii_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let review = &self.pii.review;
        if self.pii.mapping.open
            || self.pii.reviewing
            || !review.variants().is_empty()
            || !review.applied().is_empty()
        {
            self.toggle_replacements_panel(window, cx);
        } else {
            self.start_pii_scan(cx);
        }
    }
    fn setup_pii_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !detector::SUPPORTED {
            return;
        }
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.pseudonymization,
            Err(e) => {
                self.pii.error = Some(e);
                cx.notify();
                return;
            }
        };
        self.model_panel.update(cx, |panel, cx| {
            panel.setup(settings::Model::Pii(config.model), false, cx);
            panel.show(window, cx);
        });
    }

    pub(crate) fn activate_annotation(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let review = &self.pii.review;
        let available = if id & APPLIED_ID != 0 {
            review.applied_occurrence(id & !APPLIED_ID).is_some()
        } else {
            self.pii.reviewing && review.candidate(id).is_some()
        };
        if !available {
            return;
        }
        self.sync_replacement_annotation(id, cx);
        self.pii.error = None;
        self.pii.show_popup(Popup::Selection, window.focused(cx));
        self.pii
            .popup_scroll
            .set_offset(gpui::point(px(0.), px(0.)));
        self.remember_active_replacement(cx);
        window.focus(&self.pii.focus, cx);
        cx.notify();
    }
    pub(crate) fn step_pii_highlight(
        &mut self,
        backwards: bool,
        at_caret: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cursor = self.editor.read(cx).cursor();
        let mut mentions: Vec<_> = self
            .pii
            .review
            .candidates()
            .iter()
            .map(|o| (o.id, o.range.clone()))
            .collect();
        mentions.extend(
            self.pii
                .review
                .applied()
                .iter()
                .map(|o| (APPLIED_ID | o.id, o.range.clone())),
        );
        mentions.sort_by_key(|(_, range)| range.start);
        let choice = if at_caret {
            mentions
                .iter()
                .find(|(_, range)| range.start <= cursor && cursor < range.end)
        } else if backwards {
            mentions
                .iter()
                .rev()
                .find(|(_, range)| range.start < cursor)
                .or(mentions.last())
        } else {
            mentions
                .iter()
                .find(|(_, range)| range.start > cursor)
                .or(mentions.first())
        };
        if let Some((id, range)) = choice {
            let (id, offset) = (*id, range.start);
            self.editor
                .update(cx, |editor, cx| editor.set_cursor(offset, cx));
            if let Some(top) = self.editor.read(cx).offset_screen_top(offset) {
                let viewport = self.scroll.bounds();
                let delta = top - viewport.top() - px(24.);
                let mut scroll = self.scroll.offset();
                scroll.y = (scroll.y - delta).clamp(-self.scroll.max_offset().y, px(0.));
                self.scroll.set_offset(scroll);
            }
            self.activate_annotation(id, window, cx);
            let weak = cx.entity().downgrade();
            window.on_next_frame(move |window, _| {
                window.on_next_frame(move |_, cx| {
                    let _ = weak.update(cx, |_, cx| cx.notify());
                });
            });
        }
    }
    pub(crate) fn add_pii_candidate(
        &mut self,
        _: &PiiAddCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy_markdown() {
            return;
        }
        let editor = self.editor.read(cx);
        let source = editor.text().to_owned();
        let range = editor.selection();
        self.pii.reviewing = true;
        if let Err(error) = Review::validate_manual(&source, range.clone()) {
            self.pii.error = Some(error);
            cx.notify();
            return;
        }
        let category = self.pii.category;
        let Some(id) = self.checkpoint_review(cx, |review| {
            review
                .add_manual(&source, range.clone(), category)
                .expect("validated unchanged selection")
        }) else {
            return;
        };
        self.pii.error = None;
        self.sync_annotations(cx);
        if let Some(annotation) = self.pii.review.annotation_id(id, &range) {
            self.activate_annotation(annotation, window, cx);
        }
        cx.notify();
    }
    pub(crate) fn apply_all_pii(
        &mut self,
        _: &PiiApplyAll,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pii.reviewing && self.pii.review.remaining() > 0 {
            self.apply_replacements(cx);
        }
    }
    /// Record a metadata-only review change as its own undo step. The checkpoint
    /// commits first so Keep exclusions join the new step's journal entry.
    fn checkpoint_review<R>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Review) -> R,
    ) -> Option<R> {
        let old = self.pii.review.identity_snapshot();
        let before = self.editor.read(cx).history_id();
        let revision = self.editor.read(cx).revision();
        if !self
            .editor
            .update(cx, |e, cx| e.checkpoint_metadata(revision, cx))
        {
            return None;
        }
        let tx = self.editor.read(cx).last_transaction().cloned().unwrap();
        self.pii_transaction(&tx, cx);
        let result = change(&mut self.pii.review);
        let after = self.editor.read(cx).history_id();
        self.pii.review.commit_identity_snapshot(before, after, old);
        Some(result)
    }
    /// Commit planned replacements as one undo step, or a metadata checkpoint when
    /// there are none. `policy_before` is the identity policy preceding staged
    /// mapping changes; Undo returns to it. Returns the new history id.
    fn commit_plans(
        &mut self,
        revision: u64,
        plans: &[ReplacementPlan],
        policy_before: Option<IdentitySnapshot>,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let before = self.editor.read(cx).history_id();
        let review = &mut self.pii.review;
        let staged = policy_before.as_ref().map(|old| {
            let next = review.identity_snapshot();
            review.restore_identity_snapshot(old.clone());
            next
        });
        let added = review.prepare_replacements(plans);
        let edits: Vec<_> = plans
            .iter()
            .map(|p| (p.range.clone(), p.after.to_string()))
            .collect();
        let committed = self.editor.update(cx, |e, cx| {
            if edits.is_empty() {
                e.checkpoint_metadata(revision, cx)
            } else {
                e.replace_ranges(revision, &edits, cx)
            }
        });
        if !committed {
            return None;
        }
        let transaction = self.editor.read(cx).last_transaction().cloned().unwrap();
        self.pii_transaction(&transaction, cx);
        let after = self.editor.read(cx).history_id();
        let review = &mut self.pii.review;
        if let (Some(old), Some(next)) = (policy_before, staged) {
            review.restore_identity_snapshot(next);
            review.commit_identity_snapshot(before, after, old);
        }
        review.commit_replacements(after, added);
        Some(after)
    }
    pub(crate) fn pii_transaction(
        &mut self,
        transaction: &mdoc_editor::EditorTransaction,
        cx: &mut Context<Self>,
    ) {
        self.pii
            .review
            .on_transaction(transaction, self.editor.read(cx).text());
        if transaction
            .changes
            .iter()
            .any(|change| change.edits.is_empty())
        {
            self.sync_identity_selection_after_history(cx);
        }
        self.sync_annotations(cx);
        cx.notify();
    }
    pub(crate) fn restore_pii(
        &mut self,
        _: &PiiRestore,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.restore_pii_scope(false, window, cx);
    }
    pub(crate) fn restore_all_pii(
        &mut self,
        _: &PiiRestoreAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.restore_pii_scope(true, window, cx);
    }
    fn restore_pii_scope(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.pii.popup.as_ref().and(self.selected_applied()) else {
            return;
        };
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        if !self.pii.review.matches_history(editor.history_id()) {
            self.pii.error = Some("Document changed. Review the replacement again.".into());
            cx.notify();
            return;
        }
        let plan = self.pii.review.restoration_edits(editor.text(), id, all);
        match plan {
            Ok(edits) => {
                let restored = self.pii.review.prepare_restore(&edits);
                if self
                    .editor
                    .update(cx, |e, cx| e.replace_ranges(revision, &edits, cx))
                {
                    let transaction = self.editor.read(cx).last_transaction().cloned().unwrap();
                    self.pii_transaction(&transaction, cx);
                    self.pii
                        .review
                        .commit_restore(self.editor.read(cx).history_id(), restored);
                    self.pii_edited(cx);
                    self.pii.error = None;
                    window.focus(&self.editor.read(cx).focus_handle(cx), cx);
                }
            }
            Err(error) => self.pii.error = Some(error),
        }
        cx.notify();
    }
    pub(crate) fn close_pii_popup(
        &mut self,
        _: &PiiClosePopup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = self.pii.close_popup();
        self.editor
            .update(cx, |e, cx| e.set_active_annotation(None, cx));
        if let Some(focus) = previous {
            window.focus(&focus, cx);
        } else {
            window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }
}
