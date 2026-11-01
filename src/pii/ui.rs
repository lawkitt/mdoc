//! Live-document PII state, scan lifecycle and replacement actions.
mod case_sync;
mod chooser;
mod compare;
mod decisions;
pub(crate) use compare::CompareMode;
pub(crate) use compare::{CompareState, SwitcherRest};
mod discovery;
mod mapping;
mod render;
mod reply;
pub(crate) use reply::ReplyBox;
pub(crate) mod restore;
mod scan;
pub(crate) use restore::RestoredReply;
use scan::ScanJob;
#[cfg(test)]
mod tests;

use crate::{document_view::DocumentView, settings_ui};
use gpui::{App, Context, FocusHandle, Focusable, Hsla, KeyBinding, Pixels, Window, actions, px};
use mdoc_pii::{Category, IdentitySnapshot, Review, tracking::ReplacementPlan};
use mdoc_store::settings;
use std::{
    cell::Cell,
    ops::Range,
    rc::Rc,
    sync::{Arc, atomic::Ordering},
};
actions!(
    mdoc,
    [
        CopyForAi,
        CloseCompare,
        Pseudonymize,
        PiiAddCandidate,
        PiiAddCandidateAs,
        PiiReviewCandidate,
        PiiNextCandidate,
        PiiPreviousCandidate,
        PiiConfirm,
        PiiApplyAll,
        PiiNextChoice,
        PiiPreviousChoice,
        PiiOpenChoice,
        PiiClosePopup,
        ExpandAllIdentities,
        CollapseAllIdentities,
        PanelUp,
        PanelDown,
        PanelFirst,
        PanelLast,
        PanelEnterList,
        PanelEscape,
        PanelMoveUp,
        PanelMoveDown,
        PanelToggle,
        PanelOpen,
        PanelExpand,
        PanelCollapse,
        PanelKeep,
        ApplyThis,
        ApplySame,
        ApplyAll,
        KeepThis,
        KeepSame,
        KeepAll,
        PopupPreviousMention,
        PopupNextMention,
        PopupFirstMention,
        PopupLastMention,
    ]
);

/// The word popup edits the workspace selection; the chooser disambiguates
/// several hidden fields that share one visual row.
pub(crate) enum Popup {
    Selection,
    Choose { ids: Arc<[u64]>, selected: usize },
}
pub(crate) const APPLIED_ID: u64 = 1 << 63;
/// Window origin of the popup layer and the scroll offset it was laid out at.
type PopupLayer = Rc<Cell<Option<(gpui::Point<Pixels>, gpui::Point<Pixels>)>>>;
/// Annotation ids of kept-mention markers carry this bit over the start offset.
const KEPT_ID: u64 = 1 << 61;
pub(crate) struct ReviewUi {
    /// Derived data (panel projection, before/after view, spelling
    /// exclusions) is keyed on `review.revision()`, which its mutators advance.
    review: Review,
    /// Candidate highlights are shown; Close review hides them.
    reviewing: bool,
    popup: Option<Popup>,
    /// The popup was opened by a manual addition, so Enter applies (ADR 0024).
    enter_applies: bool,
    /// The next activation comes from a manual addition (alias cue choice).
    manual_activation: bool,
    /// The selection pill's category menu is open for this selection
    /// (ADR 0024); it closes when the selection changes.
    selection_menu: Option<Range<usize>>,
    selection_menu_focus: FocusHandle,
    focus: FocusHandle,
    job: Option<ScanJob>,
    /// When the running scan started, for the toolbar stage label.
    scan_started: std::time::Instant,
    discovery_job: Option<discovery::DiscoveryJob>,
    discovery_pending: bool,
    scans: Vec<settings::PiiConfig>,
    error: Option<String>,
    /// The panel shows the setup consent card instead of a review (ADR 0026).
    setup: bool,
    /// Download & scan was chosen here: scan this document once setup succeeds.
    setup_subscription: Option<gpui::Subscription>,
    mapping: mapping::MappingUi,
    generation: u64,
    popup_previous: Option<FocusHandle>,
    popup_scroll: gpui::ScrollHandle,
    /// The popup's last measured anchor. Right after an edit the editor has
    /// not measured the new chip yet; holding this keeps the popup in place.
    popup_anchor: Cell<Option<gpui::Bounds<Pixels>>>,
    /// The popup layer's window origin and the document scroll offset when it
    /// was last laid out; with the current offset this places the mention on
    /// screen in the frame being built (ADR 0055).
    /// The mention bar's size at its last layout, for placing it.
    popup_size: Rc<Cell<gpui::Size<Pixels>>>,
    popup_layer: PopupLayer,
    chooser_scroll: gpui::UniformListScrollHandle,
    popup_controls: std::cell::RefCell<std::collections::HashMap<gpui::ElementId, FocusHandle>>,
    /// Set in a tab opened by Paste AI Response (ADR 0041).
    restore: Option<restore::RestoreState>,
    /// What you did (ADR 0079): memory only, as long as the tab.
    decisions: mdoc_pii::decisions::DecisionLog,
    /// Writes an entry's time; renders and tests fix it.
    clock: fn() -> String,
}
impl Drop for ReviewUi {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl ReviewUi {
    pub fn new(theme: Rc<Cell<crate::style::Theme>>, cx: &mut Context<DocumentView>) -> Self {
        Self {
            review: Review::default(),
            reviewing: false,
            popup: None,
            enter_applies: false,
            manual_activation: false,
            selection_menu: None,
            selection_menu_focus: cx.focus_handle(),
            focus: cx.focus_handle(),
            job: None,
            scan_started: std::time::Instant::now(),
            discovery_job: None,
            discovery_pending: false,
            scans: Vec::new(),
            error: None,
            setup: false,
            setup_subscription: None,
            mapping: mapping::MappingUi::new(theme, cx),
            generation: 0,
            popup_previous: None,
            popup_scroll: gpui::ScrollHandle::new(),
            popup_anchor: Cell::new(None),
            popup_layer: Default::default(),
            popup_size: Rc::new(Cell::new(gpui::size(px(480.), px(36.)))),
            chooser_scroll: gpui::UniformListScrollHandle::new(),
            popup_controls: Default::default(),
            restore: None,
            decisions: Default::default(),
            clock: decisions::local_time,
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
    /// The document's review: identities, decisions and their history.
    pub(crate) fn review(&self) -> &Review {
        &self.review
    }

    pub(crate) fn review_mut(&mut self) -> &mut Review {
        &mut self.review
    }

    #[cfg(test)]
    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Candidate highlights are shown; Close review hides them.
    pub(crate) fn reviewing(&self) -> bool {
        self.reviewing
    }

    #[cfg(test)]
    pub(crate) fn set_reviewing(&mut self, reviewing: bool) {
        self.reviewing = reviewing;
    }

    #[cfg(test)]
    pub(crate) fn popup(&self) -> Option<&Popup> {
        self.popup.as_ref()
    }

    /// The panel shows the setup consent card instead of a review (ADR 0026).
    #[cfg(test)]
    pub(crate) fn setup(&self) -> bool {
        self.setup
    }

    /// Show or hide the setup card, for renders of it.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn show_setup(&mut self, setup: bool) {
        self.setup = setup;
    }

    /// Show a scan running with `done` of `windows` model windows read, or
    /// stop it, for renders of it (#309). No detector runs.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn show_scan(&mut self, count: Option<(usize, usize)>) {
        self.cancel();
        let Some((done, windows)) = count else {
            return;
        };
        let progress = std::sync::Arc::new(scan::ScanCount::default());
        progress.set(done, windows);
        self.reviewing = true;
        self.mapping.begin_review();
        self.scan_started = std::time::Instant::now();
        self.job = Some(ScanJob {
            cancel: Default::default(),
            revision: 0,
            generation: self.generation,
            config: Default::default(),
            progress,
        });
    }

    pub(crate) fn scans(&self) -> &[settings::PiiConfig] {
        &self.scans
    }

    #[cfg(test)]
    pub(crate) fn add_scan(&mut self, scan: settings::PiiConfig) {
        self.scans.push(scan);
    }

    /// Set in a tab opened by Paste AI Response (ADR 0041).
    pub(crate) fn restore(&self) -> Option<&restore::RestoreState> {
        self.restore.as_ref()
    }

    pub(crate) fn scanning(&self) -> bool {
        self.job.is_some()
    }
    /// Open a popup, remembering where focus returns when it closes.
    /// Retargeting an open popup (stepping, a click on another mention, a
    /// chooser pick) keeps the focus it returns to: the focus at that moment
    /// is the popup's own.
    fn show_popup(&mut self, popup: Popup, previous_focus: Option<FocusHandle>) {
        self.enter_applies = false;
        self.popup_anchor.set(None);
        if self.popup.is_none() {
            self.popup_previous = previous_focus;
        }
        self.popup = Some(popup);
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
        self.enter_applies = false;
        self.popup = None;
    }
    /// Close the popup, returning the focus it should restore.
    fn close_popup(&mut self) -> Option<FocusHandle> {
        self.enter_applies = false;
        self.popup = None;
        self.popup_previous.take()
    }
}

/// The Replacements checklist (ADR 0048): Space ticks, Enter jumps to the
/// mention, →/← show and hide mentions, ⌫ keeps the original.
pub(crate) const LIST: &str =
    "IdentityPanel && !ReplacementSearch && !SettingsInput && !UiControl && !ReplyBox";
/// The mention bar: Enter applies in the selected scope and moves on, Space
/// unticks in it, ⌫ / ⌘⌫ / ⇧⌘⌫ keep this / same wording / all (ADR 0055).
pub(crate) const POPUP: &str = "PseudonymReview && !PseudonymReplacement && !UiControl && !UiMenu";
/// ← → step through the entity's mentions, ⌘← ⌘→ to its ends; the alias
/// field and menus keep their own arrows (ADR 0055).
pub(crate) const STEP: &str = "PseudonymReview && !PseudonymReplacement && !PiiChooser && !UiMenu";
/// Document undo in the popup; the alias field keeps its own text undo.
pub(crate) const POPUP_UNDO: &str = "PseudonymReview && !PseudonymReplacement";

/// The review keys come from the shortcuts table (ADR 0073); moving inside
/// the chooser and the popup's and panel's menus stays here. Registered
/// after the table's popup keys: the chooser is also `PseudonymReview`, and
/// the later binding wins its Enter.
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys(crate::shortcuts::bindings(crate::shortcuts::Owner::Pii));
    cx.bind_keys([
        // This multiline field is a Share control: Tab moves between
        // controls instead of indenting the pasted reply.
        KeyBinding::new("tab", crate::ui::NextControl, Some("ReplyBox > Editor")),
        KeyBinding::new(
            "shift-tab",
            crate::ui::PreviousControl,
            Some("ReplyBox > Editor"),
        ),
        KeyBinding::new("down", PiiNextChoice, Some("PiiChooser")),
        KeyBinding::new("up", PiiPreviousChoice, Some("PiiChooser")),
        KeyBinding::new("enter", PiiOpenChoice, Some("PiiChooser")),
        // In the popup's and panel's menus ↑ ↓ move between rows, like Tab
        // (ADR 0062).
        KeyBinding::new("up", crate::ui::PreviousControl, Some("PiiMenu")),
        KeyBinding::new("down", crate::ui::NextControl, Some("PiiMenu")),
    ]);
}

impl DocumentView {
    pub(crate) fn reset_pii(&mut self, cx: &mut Context<Self>) {
        self.pii.cancel();
        self.pii.review = Review::default();
        self.pii.scans.clear();
        self.pii.dismiss_popup();
        self.pii.error = None;
        self.pii.mapping = mapping::MappingUi::new(self.theme.clone(), cx);
        self.pii.restore = None;
        self.editor
            .update(cx, |editor, _| editor.set_copy_metadata(None));
        self.pii.decisions.clear();
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
            self.pii.error = Some("Document changed; scan cancelled. Rescan when ready.".into());
        }
        self.schedule_pii_discovery(cx);
    }
    pub(crate) fn sync_pii_theme(&mut self, cx: &mut Context<Self>) {
        self.sync_annotations(cx);
    }
    /// Domain changes schedule spellcheck and a case save. Derived review data
    /// needs no bump here: it follows `Review::revision`.
    pub(crate) fn review_changed(&mut self, cx: &mut Context<Self>) {
        self.refresh_review_exclusions(cx);
        self.schedule_case_save(cx);
    }
    fn refresh_review_exclusions(&mut self, cx: &mut Context<Self>) {
        self.sync_annotations(cx);
        self.schedule_spellcheck(false, cx);
    }
    /// Paint only: opening a panel or changing theme must not schedule work.
    fn sync_annotations(&mut self, cx: &mut Context<Self>) {
        // Proposals take their category's colour; applied replacements share
        // the alias blue (ADR 0032, ADR 0048).
        let theme = self.theme.get();
        let accent = theme.proposed();
        let applied_accent = self.theme.get().applied();
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
                // A proposal: a light tint of its category with the category's
                // underline; the glyphs keep their colour (ADR 0048).
                let category = review
                    .occurrence_identity(occurrence.variant, &occurrence.range)
                    .and_then(|id| review.identity(id))
                    .map(|identity| identity.category)
                    .or_else(|| review.variant(occurrence.variant).map(|v| v.category));
                let tint = category.map_or(accent, |c| theme.category(c));
                mdoc_editor::SourceAnnotation {
                    id: occurrence.id,
                    range: occurrence.range.clone(),
                    color: Hsla { a: 0.12, ..tint },
                    active_color: Hsla { a: 0.28, ..tint },
                    border: gpui::transparent_black(),
                    text_color: None,
                    underline: Some(Hsla { a: 0.9, ..tint }),
                    dotted: false,
                }
            } else {
                let occurrence = applied.next().unwrap();
                mdoc_editor::SourceAnnotation {
                    id: APPLIED_ID | occurrence.id,
                    range: occurrence.range.clone(),
                    color: Hsla {
                        a: 0.16,
                        ..applied_accent
                    },
                    active_color: Hsla {
                        a: 0.34,
                        ..applied_accent
                    },
                    border: Hsla {
                        a: 0.5,
                        ..applied_accent
                    },
                    text_color: Some(theme.applied_text()),
                    underline: None,
                    dotted: false,
                }
            });
        }
        // Kept originals keep a quiet dotted underline while reviewing; a
        // click reopens their controls (ADR 0055).
        if show_candidates {
            let muted = theme.pdf_style().header_muted;
            annotations.extend(self.kept_entries().into_iter().map(|(range, _, _)| {
                mdoc_editor::SourceAnnotation {
                    id: KEPT_ID | range.start as u64,
                    range,
                    color: gpui::transparent_black(),
                    active_color: Hsla { a: 0.14, ..muted },
                    border: gpui::transparent_black(),
                    text_color: None,
                    underline: Some(Hsla { a: 0.75, ..muted }),
                    dotted: true,
                }
            }));
        }
        // A restored AI reply also marks placeholders it could not restore.
        annotations.extend(self.unresolved_annotations(cx));
        self.editor.update(cx, |editor, cx| {
            editor.set_annotations(editor.revision(), annotations, cx)
        });
        self.sync_selection_action(cx);
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
        let kind = if self.pii.restore.is_some() {
            "restored reply"
        } else if self.lifecycle.source_only() {
            "source only"
        } else if self.preview.pdf().is_some() {
            "PDF"
        } else if self.preview.source().is_some() {
            "converted"
        } else {
            "Markdown"
        };
        let mut parts = vec![format!(
            "{kind}, {} bytes, zoom {}%",
            self.editor.read(cx).text().len(),
            (self.zoom * 100.).round() as i32
        )];
        if self.preview.visible() {
            parts.push("source shown".into());
        }
        if self.pii.mapping.open {
            parts.push(
                if self.inspector_share {
                    "Share tab"
                } else {
                    "Review tab"
                }
                .into(),
            );
            parts.push(format!(
                "{} proposed, {} applied",
                self.pii.review.remaining(),
                self.pii.review.applied().len()
            ));
        }
        if self.pii.popup.is_some() {
            parts.push("popup open".into());
        }
        if self.compare.is_some() {
            parts.push("before/after view".into());
        }
        if self.search.open() {
            parts.push("find open".into());
        }
        parts.join(", ")
    }
    /// Whether the replacements panel is open (ADR 0048 header toggle).
    pub(crate) fn replacements_open(&self) -> bool {
        self.pii.mapping.open
    }
    /// The replacements panel's focus group, an F6 region (ADR 0062).
    pub(crate) fn replacements_focus(&self) -> FocusHandle {
        self.pii.mapping.focus.clone()
    }
    /// Download the selected model, then scan this document once if it is
    /// still the same document. Settings stays closed.
    fn setup_pii_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            self.pii.error = Some("Another model job is running. Retry when it finishes.".into());
            cx.notify();
            return;
        }
        self.watch_pii_setup(model, window, cx);
        cx.notify();
    }
    /// Scan once when `model`'s setup succeeds, only for the same document.
    fn watch_pii_setup(
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
                let settings_ui::Event::Finished(finished, result) = event else {
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
        if id & APPLIED_ID == 0 && id & restore::UNRESOLVED_ID != 0 {
            self.activate_unresolved((id & !restore::UNRESOLVED_ID) as usize, window, cx);
            return;
        }
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
        // Editor selections use the popup; the panel row follows (ADR 0033).
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
    /// document that is not ready does (ADR 0084). Returns whether there
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
    fn manual_target(&self, cx: &App) -> Option<Result<Range<usize>, String>> {
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
    /// new wording takes a guess (ADR 0024, ADR 0025).
    fn manual_category(&self, source: &str, range: Range<usize>) -> (Category, bool) {
        match self
            .pii
            .review
            .proposed_wording_category(&source[range.clone()])
        {
            Some(category) => (category, false),
            None => (mdoc_detect::guess_category(source, range), true),
        }
    }
    /// Offer "Replace · Person ▾" beside the settled editor selection
    /// (ADR 0024).
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
            label: "Replace".into(),
            menu_label: "Replace with placeholder".into(),
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
    /// open the popup on it as one undo step (ADR 0024). A proposed wording
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
        self.record_decision(before, mdoc_pii::decisions::Decision::Add { original }, cx);
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
            // Inspector (ADR 0048 variant C): next comes sharing.
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
                self.inspector_share = true;
            }
        }
        // Nothing is left to review, so the popup closes (ADR 0055).
        if self.pii.reviewing && self.pii.popup.is_some() && self.pii.review.remaining() == 0 {
            self.close_pii_popup(&PiiClosePopup, window, cx);
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
    /// Replacement motion (ADR 0032): a just-applied chip eases from the
    /// proposal gold through a soft blue glow to rest.
    fn flash_applied(&mut self, ids: std::collections::HashSet<u64>, cx: &mut Context<Self>) {
        if cx.reduce_motion() {
            return;
        }
        let gold = self.theme.get().proposed();
        let blue = self.theme.get().applied();
        self.editor.update(cx, |editor, cx| {
            editor.flash_annotations(
                mdoc_editor::AnnotationFlash {
                    ids,
                    from: (Hsla { a: 0.22, ..gold }, Hsla { a: 0.6, ..gold }),
                    peak: (Hsla { a: 0.3, ..blue }, Hsla { a: 0.85, ..blue }),
                    duration: std::time::Duration::from_millis(700),
                    mark: None,
                },
                cx,
            )
        });
    }
    /// Roll each replaced span's words inside its chip (ADR 0032). `edits`
    /// are (range in `before`, new text), applied to `before` just now.
    pub(super) fn roll_replaced(
        &mut self,
        edits: &[(Range<usize>, String)],
        before: &str,
        cx: &mut Context<Self>,
    ) {
        if cx.reduce_motion() || edits.is_empty() {
            return;
        }
        let mut sorted: Vec<_> = edits.iter().collect();
        sorted.sort_by_key(|(r, _)| r.start);
        let mut shift = 0isize;
        let items = sorted
            .into_iter()
            .filter_map(|(r, text)| {
                let start = r.start.saturating_add_signed(shift);
                shift += text.len() as isize - r.len() as isize;
                let previous = before.get(r.clone())?;
                (previous != text).then(|| (start..start + text.len(), previous.to_string().into()))
            })
            .collect();
        let surface = self.theme.get().pdf_style().bg;
        self.editor.update(cx, |editor, cx| {
            let revision = editor.revision();
            editor.roll_text(
                mdoc_editor::TextRoll {
                    revision,
                    items,
                    surface,
                    duration: std::time::Duration::from_millis(600),
                },
                cx,
            )
        });
    }
    /// Undo replacement: the restored proposal eases from blue back to gold.
    fn flash_unapplied(&mut self, ids: std::collections::HashSet<u64>, cx: &mut Context<Self>) {
        if cx.reduce_motion() {
            return;
        }
        let gold = self.theme.get().proposed();
        let blue = self.theme.get().applied();
        self.editor.update(cx, |editor, cx| {
            editor.flash_annotations(
                mdoc_editor::AnnotationFlash {
                    ids,
                    from: (Hsla { a: 0.22, ..blue }, Hsla { a: 0.6, ..blue }),
                    peak: (Hsla { a: 0.28, ..gold }, Hsla { a: 0.85, ..gold }),
                    duration: std::time::Duration::from_millis(650),
                    mark: None,
                },
                cx,
            )
        });
    }
    /// Keep original: the text loses its chip, so a ghost of the chip (blue
    /// for a reverted replacement, gold for a declined proposal) fades out.
    fn fade_out_kept(&mut self, ranges: Vec<(Range<usize>, bool)>, cx: &mut Context<Self>) {
        if cx.reduce_motion() {
            return;
        }
        let gold = self.theme.get().proposed();
        let blue = self.theme.get().applied();
        let ranges = ranges
            .into_iter()
            .map(|(range, applied)| {
                let c = if applied { blue } else { gold };
                (range, Hsla { a: 0.22, ..c }, Hsla { a: 0.7, ..c })
            })
            .collect();
        self.editor.update(cx, |editor, cx| {
            let revision = editor.revision();
            editor.flash_ranges(
                mdoc_editor::RangeFlash {
                    revision,
                    ranges,
                    duration: std::time::Duration::from_millis(700),
                },
                cx,
            )
        });
    }
    /// Scan results fade in together as the sweep stops (ADR 0032).
    fn fade_in_candidates(&mut self, cx: &mut Context<Self>) {
        if cx.reduce_motion() {
            return;
        }
        let gold = self.theme.get().proposed();
        let ids = self.pii.review.candidates().iter().map(|c| c.id).collect();
        let clear = Hsla { a: 0., ..gold };
        self.editor.update(cx, |editor, cx| {
            editor.flash_annotations(
                mdoc_editor::AnnotationFlash {
                    ids,
                    from: (clear, clear),
                    peak: (Hsla { a: 0.1, ..gold }, Hsla { a: 0.35, ..gold }),
                    duration: std::time::Duration::from_millis(250),
                    mark: None,
                },
                cx,
            )
        });
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
            self.schedule_case_save(cx);
        }
        cx.notify();
    }
    /// A popup closed by an edit (an undo, a scan, Apply elsewhere) would
    /// leave focus on its unrendered card, where shortcuts reach nothing.
    /// Focus returns as Escape returns it (#79). Called while rendering.
    pub(crate) fn release_closed_popup_focus(&mut self, window: &mut Window, cx: &mut App) {
        // The restore picker shares the popup's focus (ADR 0041).
        if self.pii.popup.is_some()
            || self.pii.restore.as_ref().is_some_and(|r| r.popup.is_some())
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
        // The next popup starts on the whole entity again (ADR 0055).
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
