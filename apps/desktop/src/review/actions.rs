//! What the review does on a tab: its lifecycle with the document, the
//! model's setup, opening and stepping through mentions, adding a name by
//! hand, Apply all and committing replacements as one undo step.
use super::{
    APPLIED_ID, KEPT_ID, PiiAddCandidate, PiiApplyAll, PiiClosePopup, Popup, Pseudonymize, mapping,
    text,
};
use crate::document::DocumentView;
use gpui::{App, Context, FocusHandle, Focusable, Window, px};
use mdoc_pii::{Category, IdentitySnapshot, Review, tracking::ReplacementPlan};
use mdoc_store::settings;
use std::ops::Range;

impl DocumentView {
    /// The review as its document keeps it; `None` while it holds nothing.
    pub(crate) fn saved_review(&self) -> Option<mdoc_pii::saved::SavedReview> {
        let saved = self.pii.review.save();
        (saved != Review::default().save()).then_some(saved)
    }

    /// A document's saved review, back over the text it was saved with.
    /// A review that no longer fits the text is dropped: the document then
    /// opens as if never scanned.
    pub(crate) fn restore_review(
        &mut self,
        saved: mdoc_pii::saved::SavedReview,
        cx: &mut Context<Self>,
    ) {
        let editor = self.editor.read(cx);
        let Ok(review) = Review::restore(saved, editor.text(), editor.history_id()) else {
            return;
        };
        self.pii.review = review;
        self.pii.reviewing = true;
        self.review_changed(cx);
    }

    pub(crate) fn reset_pii(&mut self, cx: &mut Context<Self>) {
        self.pii.cancel();
        self.pii.review = Review::default();
        self.pii.scans.clear();
        self.pii.dismiss_popup();
        self.pii.error = None;
        self.pii.mapping = mapping::MappingUi::new(self.theme.clone(), cx);
        self.pii.decisions.clear();
        // The new review lists nothing; what the old one put on the Project's
        // lists stays there.
        self.project.forget_listed();
        self.review_changed(cx);
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
            self.pii.error = Some(text::SCAN_CANCELLED.into());
        }
        self.schedule_pii_discovery(cx);
        self.schedule_edit_scan(cx);
    }
    /// Names may still be found, or a write of them is due: a Copy for AI
    /// asked now waits for them.
    pub(crate) fn finding_names(&self) -> bool {
        self.pii.finding_names() || self.save_scheduled()
    }

    /// Domain changes schedule spellcheck and a fold into the Project. Derived review data
    /// needs no bump here: it follows `Review::revision`.
    pub(crate) fn review_changed(&mut self, cx: &mut Context<Self>) {
        self.refresh_review_exclusions(cx);
        self.schedule_project_save(cx);
    }
    pub(super) fn refresh_review_exclusions(&mut self, cx: &mut Context<Self>) {
        self.sync_annotations(cx);
        self.schedule_spellcheck(false, cx);
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
    /// What this document shows, for a crash report: words and numbers only,
    /// never its text or name.
    pub(crate) fn crash_state(&self, cx: &App) -> String {
        let kind = if self.lifecycle.source_only() {
            text::SOURCE_ONLY
        } else if self.preview.pdf().is_some() {
            text::PDF
        } else if self.preview.source().is_some() {
            text::CONVERTED
        } else {
            text::MARKDOWN
        };
        let mut parts = vec![text::crash_kind(
            kind,
            self.editor.read(cx).text().len(),
            (self.zoom * 100.).round() as i32,
        )];
        if self.preview.visible() {
            parts.push(text::SOURCE_SHOWN.into());
        }
        if self.pii.mapping.open {
            parts.push(
                match self.inspector {
                    crate::document::Inspector::Review => text::REVIEW_TAB,
                    crate::document::Inspector::Share => text::SHARE_TAB,
                    crate::document::Inspector::Notes => text::NOTES_TAB,
                }
                .into(),
            );
            parts.push(text::crash_counts(
                self.pii.review.remaining(),
                self.pii.review.applied().len(),
            ));
        }
        if self.pii.popup.is_some() {
            parts.push(text::POPUP_OPEN.into());
        }
        if self.compare.is_some() {
            parts.push(text::BEFORE_AFTER_VIEW.into());
        }
        if self.search.open() {
            parts.push(text::FIND_OPEN.into());
        }
        parts.join(", ")
    }
    /// Whether the replacements panel is open.
    /// Pseudonymize's click, and an AI notification's (Milestone 2 item
    /// 9): scan once, unless a scan runs, and open the Review panel; it
    /// never closes it.
    pub(crate) fn open_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let review = &self.pii.review();
        let fresh = !self.pii.reviewing()
            && !self.pii.scanning()
            && review.variants().is_empty()
            && review.applied().is_empty();
        if fresh {
            self.start_pii_scan(cx);
        }
        // A scan is reviewed in Review, not beside Notes.
        if self.inspector == crate::document::Inspector::Notes {
            self.inspector = crate::document::Inspector::Review;
        }
        if !self.replacements_open() {
            self.toggle_replacements_panel(window, cx);
        }
    }
    pub(crate) fn replacements_open(&self) -> bool {
        self.pii.mapping.open
    }
    /// The replacements panel's focus group, an F6 region.
    pub(crate) fn replacements_focus(&self) -> FocusHandle {
        self.pii.mapping.focus.clone()
    }
    /// Download the selected model, then scan this document once if it is
    /// still the same document. Settings stays closed.
    pub(super) fn setup_pii_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !mdoc_detect::SUPPORTED {
            return;
        }
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.pseudonymization,
            Err(e) => {
                self.pii.error = Some(e.to_string());
                cx.notify();
                return;
            }
        };
        let model = settings::Model::Pii(config.model);
        self.pii.error = None;
        let started = self.model_panel.update(cx, |panel, cx| {
            panel.setup(model, false, cx);
            panel.working == Some(model)
        });
        if !started {
            self.pii.error = Some(text::ANOTHER_MODEL_JOB.into());
            cx.notify();
            return;
        }
        self.watch_pii_setup(model, window, cx);
        cx.notify();
    }
    /// Scan once when `model`'s setup succeeds, only for the same document.
    pub(super) fn watch_pii_setup(
        &mut self,
        model: settings::Model,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let identity = self.session.generation;
        self.pii.setup_subscription = Some(cx.subscribe_in(
            &self.model_panel.clone(),
            window,
            move |this, _, event, _, cx| {
                let crate::settings::Event::Finished(finished, result) = event else {
                    return;
                };
                if *finished != model {
                    return;
                }
                this.pii.setup_subscription = None;
                match result {
                    Ok(_) if this.pii.setup && this.session.generation == identity => {
                        this.pii.setup = false;
                        this.start_pii_scan(cx);
                    }
                    Ok(_) => {}
                    Err(e) => this.pii.error = Some(e.clone()),
                }
                cx.notify();
            },
        ));
    }

    pub(crate) fn activate_annotation(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let review = &self.pii.review;
        let available = if id & KEPT_ID != 0 {
            self.pii.reviewing && self.kept_selection((id & !KEPT_ID) as usize).is_some()
        } else if id & APPLIED_ID != 0 {
            review.applied_occurrence(id & !APPLIED_ID).is_some()
        } else {
            self.pii.reviewing && review.candidate(id).is_some()
        };
        if !available {
            return;
        }
        self.sync_replacement_annotation(id, cx);
        // Editor selections use the popup; the panel row follows.
        self.pii.mapping.panel_selected = false;
        self.pii.mapping.cursor = Some(mapping::PanelCursor::Mention(id));
        if !std::mem::take(&mut self.pii.manual_activation) {
            self.pii.mapping.request_cue(false, false);
        }
        self.pii.error = None;
        self.pii.show_popup(Popup::Selection, window.focused(cx));
        self.pii
            .popup_scroll
            .set_offset(gpui::point(px(0.), px(0.)));
        self.remember_active_replacement(cx);
        window.focus(&self.pii.focus, cx);
        cx.notify();
    }
    /// Show the document's earliest pending proposal, as a shelf click on a
    /// document that is not ready does. Returns whether there
    /// was one.
    pub(crate) fn select_first_proposal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(start) = self
            .pii
            .review
            .candidates()
            .iter()
            .map(|candidate| candidate.range.start)
            .min()
        else {
            return false;
        };
        self.pii.reviewing = true;
        self.editor
            .update(cx, |editor, cx| editor.set_cursor(start, cx));
        self.step_pii_highlight(false, true, window, cx);
        true
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
    /// Pseudonymize has run in this tab, so manual additions are offered. A
    /// review opened without a finished scan counts too: where scanning is
    /// unavailable or failed, marking names by hand is the way in.
    fn has_pii_review(&self) -> bool {
        let review = &self.pii.review;
        self.pii.reviewing
            || !self.pii.scans.is_empty()
            || !review.variants().is_empty()
            || !review.applied().is_empty()
    }
    /// The span a manual addition would replace for the editor selection.
    pub(super) fn manual_target(&self, cx: &App) -> Option<Result<Range<usize>, String>> {
        let editor = self.editor.read(cx);
        let selection = editor.selection();
        (!selection.is_empty()
            && self.has_pii_review()
            && !self.pii.scanning()
            && self.can_copy_markdown())
        .then(|| self.pii.review.manual_target(editor.text(), selection))
    }
    /// The category Replace gives `range`, and whether another can be
    /// picked: a proposed wording keeps its own (the popup corrects it),
    /// new wording takes a guess.
    pub(super) fn manual_category(&self, source: &str, range: Range<usize>) -> (Category, bool) {
        match self
            .pii
            .review
            .proposed_wording_category(&source[range.clone()])
        {
            Some(category) => (category, false),
            None => (mdoc_detect::guess_category(source, range), true),
        }
    }
    /// Offer "Replace · Person ▾" beside the settled editor selection.
    pub(crate) fn sync_selection_action(&mut self, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        // The editor hides the pill while the popup holds focus.
        let target = (!editor.is_selecting())
            .then(|| self.manual_target(cx))
            .flatten();
        let selection = editor.selection();
        let (category, pickable) = match &target {
            Some(Ok(range)) => {
                let (category, pickable) = self.manual_category(editor.text(), range.clone());
                (Some(category), pickable)
            }
            _ => (None, false),
        };
        // The menu belongs to the selection it opened for.
        if !pickable || self.pii.selection_menu.as_ref() != Some(&selection) {
            self.pii.selection_menu = None;
        }
        let theme = self.theme.get();
        let open = self.pii.selection_menu.is_some();
        let action = target.map(|target| mdoc_editor::SelectionAction {
            range: selection,
            label: text::REPLACE.into(),
            menu_label: text::REPLACE_WITH_ALIAS.into(),
            shortcut: if cfg!(target_os = "macos") {
                "⌘⌥P"
            } else {
                "Ctrl+Alt+P"
            }
            .into(),
            disabled: target.err().map(Into::into),
            accent: category.map_or(theme.proposed(), |c| theme.category(c)),
            choice: category
                .filter(|_| pickable)
                .map(|category| mdoc_editor::SelectionChoice {
                    label: category.label().into(),
                    accent: theme.category(category),
                    open,
                }),
        });
        self.editor
            .update(cx, |editor, cx| editor.set_selection_action(action, cx));
    }
    /// Turn the editor selection into proposed replacements of every exact
    /// repeat, with a guessed category, and open the popup on it.
    pub(crate) fn add_pii_candidate(
        &mut self,
        _: &PiiAddCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_manual_candidate(None, window, cx);
    }
    /// Add the editor selection in `picked`, or the guessed category, and
    /// open the popup on it as one undo step. A proposed wording
    /// keeps its own category, so only new wording takes a pick.
    pub(crate) fn add_manual_candidate(
        &mut self,
        picked: Option<Category>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Ok(range)) = self.manual_target(cx) else {
            return;
        };
        self.pii.selection_menu = None;
        let source = self.editor.read(cx).text().to_owned();
        let guess = mdoc_detect::guess_category(&source, range.clone());
        let (_, pickable) = self.manual_category(&source, range.clone());
        let picked = picked.filter(|_| pickable);
        let category = picked.unwrap_or(guess);
        let before = self.editor.read(cx).history_id();
        let Some(id) = self.checkpoint_review(cx, |review| {
            review
                .add_manual(&source, range.clone(), category)
                .expect("validated unchanged selection")
        }) else {
            return;
        };
        let original = source[range.clone()].into();
        self.record_decision(
            before,
            mdoc_pii::decisions::Decision::Add { original, category },
            cx,
        );
        self.pii.reviewing = true;
        if !self.pii.mapping.open {
            self.pii.mapping.open_panel(window.focused(cx));
        }
        let history = self.editor.read(cx).history_id();
        self.pii
            .mapping
            .record_added(source[range.clone()].into(), history);
        self.flash_notice(cx);
        self.pii.error = None;
        self.review_changed(cx);
        self.editor
            .update(cx, |editor, cx| editor.set_cursor(range.start, cx));
        if let Some(annotation) = self.pii.review.annotation_id(id, &range) {
            self.pii.manual_activation = true;
            self.activate_annotation(annotation, window, cx);
            self.pii.manual_activation = false;
            self.pii.enter_applies = self.pii.popup.is_some();
            // A fallback guess also cues the category chip; a pick does not.
            self.pii.mapping.request_cue(
                true,
                picked.is_none() && matches!(category, Category::Person | Category::Other),
            );
        }
        self.sync_selection_action(cx);
        cx.notify();
    }
    pub(crate) fn apply_all_pii(
        &mut self,
        _: &PiiApplyAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pii.reviewing && self.pii.review.remaining() > 0 {
            self.apply_replacements(cx);
            // Inspector: next comes sharing.
            if self.pii.review.remaining() == 0 && self.has_applied_aliases() {
                // Share unmounts Review's fields. Neither current focus nor a
                // closing popup's return target may stay on those controls.
                let panel = &self.pii.mapping.focus;
                if self
                    .pii
                    .popup_previous
                    .as_ref()
                    .is_some_and(|previous| panel.contains(previous, window))
                {
                    self.pii.popup_previous = Some(panel.clone());
                }
                if panel.contains_focused(window, cx) {
                    window.focus(panel, cx);
                }
                self.inspector = crate::document::Inspector::Share;
            }
        }
        // Nothing is left to review, so the popup closes.
        if self.pii.reviewing && self.pii.popup.is_some() && self.pii.review.remaining() == 0 {
            self.close_pii_popup(&PiiClosePopup, window, cx);
        }
    }
    /// The review is settled for the companion's Replace all (Milestone 2
    /// item 16): loaded, no scan or name finding running that may still
    /// add proposals.
    pub(crate) fn review_settled(&self) -> bool {
        self.lifecycle.ready() && !self.pii.scanning() && !self.pii.finding_names()
    }

    /// The companion's Replace all: this review's own Replace all, run as
    /// its button runs it, when it would replace every pending proposal
    /// and nothing else. `false`, with nothing changed, when only this
    /// window can go on: nothing is pending, a scan runs, or an alias the
    /// popup shows is not confirmed yet.
    pub(crate) fn replace_all_from_companion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.pii.reviewing
            || self.pii.review.remaining() == 0
            || !self.review_settled()
            || !self.can_copy_markdown()
            || self.alias_draft_changed(cx)
        {
            return false;
        }
        self.apply_all_pii(&PiiApplyAll, window, cx);
        true
    }
    /// Record a metadata-only review change as its own undo step. The checkpoint
    /// commits first so Keep exclusions join the new step's journal entry.
    pub(super) fn checkpoint_review<R>(
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
    pub(super) fn commit_plans(
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
        let flash: std::collections::HashSet<_> = added.iter().map(|a| APPLIED_ID | a.id).collect();
        let edits: Vec<_> = plans
            .iter()
            .map(|p| (p.range.clone(), p.after.to_string()))
            .collect();
        let source = self.editor.read(cx).text().to_owned();
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
        self.flash_applied(flash, cx);
        self.roll_replaced(&edits, &source, cx);
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
        let path = self.editor.read(cx).history_path();
        self.pii.decisions.follow(&path);
        if transaction
            .changes
            .iter()
            .any(|change| change.edits.is_empty())
        {
            // Text history travel leaves proposals awaiting discovery. The
            // open popup needs them now to follow its restored occurrence
            // before Changed treats it as a stale source-edit popup (#178).
            if self.pii.popup.is_some() {
                self.pii.review.refresh(self.editor.read(cx).text());
            }
            self.sync_identity_selection_after_history(cx);
        }
        self.sync_annotations(cx);
        // Text changes emit Changed next and get its one debounced request.
        if transaction.changes.iter().all(|change| {
            change.edits.is_empty()
                || change
                    .edits
                    .iter()
                    .all(|edit| edit.range.is_empty() && edit.new_len == 0)
        }) {
            self.schedule_spellcheck(false, cx);
            self.schedule_project_save(cx);
        }
        cx.notify();
    }
    /// A popup closed by an edit (an undo, a scan, Apply elsewhere) would
    /// leave focus on its unrendered card, where shortcuts reach nothing.
    /// Focus returns as Escape returns it (#79). Called while rendering.
    pub(crate) fn release_closed_popup_focus(&mut self, window: &mut Window, cx: &mut App) {
        if self.pii.popup.is_some()
            || !(self.pii.focus.is_focused(window) || self.pii.focus.contains_focused(window, cx))
        {
            return;
        }
        match self.pii.popup_previous.take() {
            Some(focus) => window.focus(&focus, cx),
            None => window.focus(&self.editor.read(cx).focus_handle(cx), cx),
        }
    }
    pub(crate) fn close_pii_popup(
        &mut self,
        _: &PiiClosePopup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = self.pii.close_popup();
        // The next popup starts on the whole entity again.
        self.pii.mapping.set_scope(mapping::Scope::Entity);
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
