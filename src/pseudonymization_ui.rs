//! Live-document PII state, scan lifecycle and replacement actions.
mod applied;
mod chooser;
mod discovery;
mod render;
mod scan;
use scan::{ScanIntent, ScanJob};
#[cfg(test)]
mod tests;

use crate::{
    AcceptAllPseudonyms, AcceptPseudonymCandidate, AddPseudonymCandidate, Anonymize,
    ClosePseudonymPopup, KeepPseudonymCandidate, NextCandidate, PreviousCandidate, Pseudonymize,
    RestoreAllPii, RestorePii, ReviewCandidate, Workspace, markdown_search,
    pseudonymization::{self, Category, Mode, Review},
    pseudonymization_detector as detector, settings, settings_ui, style,
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

pub(super) enum PopupTarget {
    Candidate { group: u64, mention: Range<usize> },
    Applied(u64),
    Choose { ids: Arc<[u64]>, selected: usize },
}
pub(super) struct Popup {
    target: PopupTarget,
    pub all: bool,
    context_open: bool,
    links_open: bool,
}
impl Popup {
    fn candidate(&self) -> Option<(u64, Range<usize>)> {
        match &self.target {
            PopupTarget::Candidate { group, mention } => Some((*group, mention.clone())),
            _ => None,
        }
    }
}
const APPLIED_ID: u64 = 1 << 63;
pub(super) struct ReviewUi {
    pub review: Review,
    pub manual_review: bool,
    pub completion: Option<(usize, u64)>,
    mode_menu_open: bool,
    mode_menu_focus: FocusHandle,
    mode_menu_previous: Option<FocusHandle>,
    pub popup: Option<Popup>,
    accept_all_bounds: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
    pub input: Entity<markdown_search::SearchInput>,
    pub focus: FocusHandle,
    pub category: Category,
    job: Option<ScanJob>,
    discovery_job: Option<discovery::DiscoveryJob>,
    discovery_pending: bool,
    details: bool,
    pub scans: Vec<settings::PiiConfig>,
    pub error: Option<String>,
    generation: u64,
    menu_open: bool,
    menu_focus: FocusHandle,
    menu_previous: Option<FocusHandle>,
    menu_bounds: Rc<Cell<gpui::Bounds<Pixels>>>,
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
            review: {
                let mut review = Review::default();
                review.set_mode(Mode::Anonymize, "");
                review
            },
            manual_review: false,
            completion: None,
            mode_menu_open: false,
            mode_menu_focus: cx.focus_handle(),
            mode_menu_previous: None,
            popup: None,
            accept_all_bounds: Rc::new(Cell::new(None)),
            input: cx.new(|cx| {
                markdown_search::SearchInput::new(cx).with_key_context("PseudonymReplacement")
            }),
            focus: cx.focus_handle(),
            category: Category::Person,
            job: None,
            discovery_job: None,
            discovery_pending: false,
            details: false,
            scans: Vec::new(),
            error: None,
            generation: 0,
            menu_open: false,
            menu_focus: cx.focus_handle(),
            menu_previous: None,
            menu_bounds: Rc::new(Cell::new(gpui::Bounds::default())),
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
}

pub(super) fn bind_keys(cx: &mut App) {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.bind_keys([
        KeyBinding::new("down", crate::NextPiiChoice, Some("PiiChooser")),
        KeyBinding::new("up", crate::PreviousPiiChoice, Some("PiiChooser")),
        KeyBinding::new("enter", crate::OpenPiiChoice, Some("PiiChooser")),
        KeyBinding::new(&format!("{modifier}-shift-p"), Pseudonymize, None),
        KeyBinding::new(&format!("{modifier}-shift-a"), Anonymize, None),
        KeyBinding::new("alt-enter", ReviewCandidate, Some("Editor")),
        KeyBinding::new("alt-down", NextCandidate, Some("Editor")),
        KeyBinding::new("alt-up", PreviousCandidate, Some("Editor")),
        KeyBinding::new(
            &format!("{modifier}-alt-p"),
            AddPseudonymCandidate,
            Some("Editor"),
        ),
        KeyBinding::new(
            "enter",
            AcceptPseudonymCandidate,
            Some("PseudonymReview && !UiControl && !UiMenu"),
        ),
        KeyBinding::new("alt-k", KeepPseudonymCandidate, Some("PseudonymReview")),
        KeyBinding::new("escape", ClosePseudonymPopup, Some("PseudonymReview")),
    ]);
}

impl Workspace {
    pub(super) fn reset_pseudonymization(&mut self, cx: &mut Context<Self>) {
        self.pseudonymization.cancel();
        self.pseudonymization.review = Review::default();
        self.pseudonymization
            .review
            .set_mode(Mode::Anonymize, self.editor.read(cx).text());
        self.pseudonymization.manual_review = false;
        self.pseudonymization.completion = None;
        self.pseudonymization.mode_menu_open = false;
        self.pseudonymization.scans.clear();
        self.pseudonymization.details = false;
        self.pseudonymization.popup = None;
        self.pseudonymization.error = None;
        self.sync_annotations(cx);
    }
    pub(super) fn pseudonymization_edited(&mut self, cx: &mut Context<Self>) {
        if !self.pseudonymization.review.open
            && self.pseudonymization.review.groups.is_empty()
            && !self.pseudonymization.scanning()
        {
            return;
        }
        if self
            .pseudonymization
            .completion
            .is_some_and(|(_, revision)| revision != self.editor.read(cx).revision())
        {
            self.pseudonymization.completion = None;
        }
        let was_scanning = self.pseudonymization.scanning();
        self.pseudonymization.cancel();
        self.pseudonymization.popup = None;
        if was_scanning {
            self.pseudonymization.error =
                Some("Document changed; scan cancelled. Rescan when ready.".into());
        }
        self.schedule_pii_discovery(cx);
        self.sync_annotations(cx);
    }
    pub(super) fn sync_pseudonym_theme(&mut self, cx: &mut Context<Self>) {
        self.sync_annotations(cx);
    }
    fn sync_annotations(&mut self, cx: &mut Context<Self>) {
        let accent = style::markdown_style(self.theme.get()).alert_warning;
        let review = &self.pseudonymization.review;
        let show_candidates = review.open
            && (review.mode == Mode::Pseudonymize || self.pseudonymization.manual_review);
        let mut candidates = review
            .candidates
            .iter()
            .filter(|_| show_candidates)
            .peekable();
        let mut applied = review.tracking.applied.iter().peekable();
        let mut annotations = Vec::with_capacity(
            if show_candidates {
                review.candidates.len()
            } else {
                0
            } + review.tracking.applied.len(),
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
    pub(super) fn select_pii_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.pseudonymization.cancel();
        self.pseudonymization.popup = None;
        self.pseudonymization.menu_open = false;
        self.pseudonymization.manual_review = false;
        self.pseudonymization.completion = None;
        self.pseudonymization.error = None;
        self.pseudonymization
            .review
            .set_mode(mode, self.editor.read(cx).text());
        self.sync_annotations(cx);
        cx.notify();
    }
    pub(super) fn anonymize(&mut self, _: &Anonymize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_copy_markdown() {
            return;
        }
        self.select_pii_mode(Mode::Anonymize, cx);
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        self.start_pii_scan(ScanIntent::Anonymize, cx);
    }
    pub(super) fn pseudonymize(
        &mut self,
        _: &Pseudonymize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy_markdown() {
            return;
        }
        self.select_pii_mode(Mode::Pseudonymize, cx);
        self.start_pii_scan(ScanIntent::Review(self.pseudonymization.review.mode), cx);
    }
    pub(super) fn scan_pseudonyms(&mut self, cx: &mut Context<Self>) {
        self.start_pii_scan(ScanIntent::Review(self.pseudonymization.review.mode), cx);
    }
    fn setup_pseudonyms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !detector::SUPPORTED {
            return;
        }
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.pseudonymization,
            Err(e) => {
                self.pseudonymization.error = Some(e);
                cx.notify();
                return;
            }
        };
        self.model_panel.update(cx, |panel, cx| {
            panel.setup(settings::Model::Pii(config.model), false, cx);
            panel.show(window, cx);
        });
    }

    fn leave_pseudonyms(&mut self, cx: &mut Context<Self>) {
        self.pseudonymization.cancel();
        self.pseudonymization.review.open = false;
        self.pseudonymization.menu_open = false;
        self.pseudonymization.popup = None;
        self.sync_annotations(cx);
        cx.notify();
    }
    pub(super) fn activate_annotation(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if id & APPLIED_ID != 0 {
            let occurrence = id & !APPLIED_ID;
            if self
                .pseudonymization
                .review
                .tracking
                .get(occurrence)
                .is_none()
            {
                return;
            }
            self.pseudonymization.popup = Some(Popup {
                target: PopupTarget::Applied(occurrence),
                all: false,
                context_open: false,
                links_open: false,
            });
            self.pseudonymization.popup_previous = window.focused(cx);
            self.pseudonymization
                .popup_scroll
                .set_offset(gpui::point(px(0.), px(0.)));
            window.focus(&self.pseudonymization.focus, cx);
            cx.notify();
            return;
        }
        if !self.pseudonymization.review.open {
            return;
        }
        let Some(candidate) = self.pseudonymization.review.candidate(id) else {
            return;
        };
        let group_id = candidate.group;
        let mention = candidate.range.clone();
        let Some(group) = self.pseudonymization.review.group(group_id) else {
            return;
        };
        let replacement = self.pseudonymization.review.replacement(group);
        self.pseudonymization.error = None;
        self.pseudonymization.popup = Some(Popup {
            target: PopupTarget::Candidate {
                group: group_id,
                mention,
            },
            all: true,
            context_open: self.editor.read(cx).annotation_is_hidden(id),
            links_open: false,
        });
        self.pseudonymization
            .popup_scroll
            .set_offset(gpui::point(px(0.), px(0.)));
        self.pseudonymization.popup_previous = window.focused(cx);
        self.pseudonymization
            .input
            .update(cx, |input, cx| input.set_value(replacement, cx));
        if self.pseudonymization.review.mode == Mode::Anonymize {
            window.focus(&self.pseudonymization.focus, cx);
        } else {
            window.focus(&self.pseudonymization.input.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }
    pub(super) fn step_pseudonym(
        &mut self,
        backwards: bool,
        at_caret: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cursor = self.editor.read(cx).cursor();
        let mut mentions: Vec<_> = self
            .pseudonymization
            .review
            .candidates
            .iter()
            .map(|o| (o.id, o.range.clone()))
            .collect();
        mentions.extend(
            self.pseudonymization
                .review
                .tracking
                .applied
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
    pub(super) fn add_pseudonym(
        &mut self,
        _: &AddPseudonymCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy_markdown() {
            return;
        }
        let editor = self.editor.read(cx);
        let source = editor.text().to_owned();
        let range = editor.selection();
        self.pseudonymization.review.open = true;
        self.pseudonymization.manual_review = true;
        match self.pseudonymization.review.add_manual(
            &source,
            range.clone(),
            self.pseudonymization.category,
        ) {
            Ok(id) => {
                self.pseudonymization.error = None;
                self.sync_annotations(cx);
                if let Some(annotation) = self.pseudonymization.review.annotation_id(id, &range) {
                    self.activate_annotation(annotation, window, cx);
                }
            }
            Err(error) => self.pseudonymization.error = Some(error),
        }
        cx.notify();
    }
    pub(super) fn accept_pseudonym(
        &mut self,
        _: &AcceptPseudonymCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = &self.pseudonymization.popup else {
            return;
        };
        if matches!(popup.target, PopupTarget::Applied(_)) {
            self.restore_pii(&RestorePii, window, cx);
            return;
        }
        let Some((id, mention)) = popup.candidate() else {
            return;
        };
        let single = (!popup.all).then_some(mention);
        let replacement = if self.pseudonymization.review.mode == Mode::Anonymize {
            let Some(group) = self.pseudonymization.review.group(id) else {
                return;
            };
            self.pseudonymization.review.replacement(group)
        } else {
            self.pseudonymization.input.read(cx).value().to_owned()
        };
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        let plan = self
            .pseudonymization
            .review
            .plan(editor.text(), id, single, &replacement);
        match plan {
            Ok(edits) => {
                if self.commit_pii_edits(revision, &edits, &[(id, replacement)], cx) {
                    self.pseudonymization_edited(cx);
                    self.pseudonymization.error = None;
                    window.focus(&self.editor.read(cx).focus_handle(cx), cx);
                }
            }
            Err(error) => self.pseudonymization.error = Some(error),
        }
        cx.notify();
    }
    pub(super) fn accept_all_pseudonyms(
        &mut self,
        _: &AcceptAllPseudonyms,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.pseudonymization.review.open
            || !self.can_copy_markdown()
            || self.pseudonymization.scanning()
            || self.pseudonymization.review.remaining() == 0
        {
            return;
        }
        let draft = if self.pseudonymization.review.mode == Mode::Pseudonymize {
            self.pseudonymization.popup.as_ref().and_then(|popup| {
                popup.candidate().map(|(group, _)| {
                    (
                        group,
                        self.pseudonymization.input.read(cx).value().to_owned(),
                    )
                })
            })
        } else {
            None
        };
        match self.commit_all_pii(draft, cx) {
            Ok(_) => {
                self.pseudonymization.popup = None;
                self.sync_annotations(cx);
                window.focus(&self.editor.read(cx).focus_handle(cx), cx);
            }
            Err(error) => self.pseudonymization.error = Some(error),
        }
        cx.notify();
    }
    fn commit_all_pii(
        &mut self,
        draft: Option<(u64, String)>,
        cx: &mut Context<Self>,
    ) -> Result<usize, String> {
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        let edits = self.pseudonymization.review.plan_all(
            editor.text(),
            draft
                .as_ref()
                .map(|(id, replacement)| (*id, replacement.as_str())),
        )?;
        if edits.is_empty() {
            return Ok(0);
        }
        let accepted: Vec<_> = self
            .pseudonymization
            .review
            .groups
            .iter()
            .filter(|group| !group.mentions.is_empty())
            .map(|group| {
                (
                    group.id,
                    draft
                        .as_ref()
                        .filter(|(id, _)| *id == group.id)
                        .map(|(_, value)| value.clone())
                        .unwrap_or_else(|| self.pseudonymization.review.replacement(group)),
                )
            })
            .collect();
        if !self.commit_pii_edits(revision, &edits, &accepted, cx) {
            return Err("The document changed. Try again.".into());
        }
        Ok(edits.len())
    }
    /// Commit text before recording mappings or rebasing Keep exclusions.
    /// Both popup acceptance and bulk/automatic acceptance use this path.
    fn commit_pii_edits(
        &mut self,
        revision: u64,
        edits: &[(Range<usize>, String)],
        accepted: &[(u64, String)],
        cx: &mut Context<Self>,
    ) -> bool {
        let originals: std::collections::HashMap<_, _> = accepted
            .iter()
            .filter_map(|(id, value)| {
                self.pseudonymization.review.group(*id).map(|g| {
                    (
                        g.original.to_string(),
                        (
                            g.original.clone(),
                            Arc::<str>::from(value.as_str()),
                            g.category,
                        ),
                    )
                })
            })
            .collect();
        let plans: Vec<_> = edits
            .iter()
            .filter_map(|(range, _)| {
                originals
                    .get(&self.editor.read(cx).text()[range.clone()])
                    .map(|(before, after, category)| {
                        (range.clone(), before.clone(), after.clone(), *category)
                    })
            })
            .collect();
        if plans.len() != edits.len() {
            return false;
        }
        let added = self.pseudonymization.review.tracking.prepare(&plans);
        if !self
            .editor
            .update(cx, |editor, cx| editor.replace_ranges(revision, edits, cx))
        {
            return false;
        }
        let transaction = self
            .editor
            .read(cx)
            .last_transaction()
            .cloned()
            .expect("committed edit transaction");
        self.pii_transaction(&transaction, cx);
        self.pseudonymization
            .review
            .tracking
            .commit(self.editor.read(cx).history_id(), added);
        for (id, replacement) in accepted {
            self.pseudonymization
                .review
                .record_acceptance(*id, replacement);
        }
        self.pseudonymization
            .review
            .refresh(self.editor.read(cx).text());
        self.pseudonymization.error = None;
        true
    }
    pub(super) fn keep_pseudonym(
        &mut self,
        _: &KeepPseudonymCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.pseudonymization.popup.take() else {
            return;
        };
        let Some((group, mention)) = popup.candidate() else {
            return;
        };
        self.pseudonymization
            .review
            .keep(group, (!popup.all).then_some(mention));
        self.sync_annotations(cx);
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    pub(super) fn pii_transaction(
        &mut self,
        transaction: &mdoc_editor::EditorTransaction,
        cx: &mut Context<Self>,
    ) {
        self.pseudonymization
            .review
            .on_transaction(transaction, self.editor.read(cx).text());
    }
    pub(super) fn restore_pii(
        &mut self,
        _: &RestorePii,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.restore_pii_scope(false, window, cx);
    }
    pub(super) fn restore_all_pii(
        &mut self,
        _: &RestoreAllPii,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.restore_pii_scope(true, window, cx);
    }
    fn restore_pii_scope(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Popup {
            target: PopupTarget::Applied(id),
            ..
        }) = self.pseudonymization.popup.as_ref()
        else {
            return;
        };
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        if !self
            .pseudonymization
            .review
            .tracking
            .matches_history(editor.history_id())
        {
            self.pseudonymization.error =
                Some("Document changed. Review the replacement again.".into());
            cx.notify();
            return;
        }
        let plan = self
            .pseudonymization
            .review
            .tracking
            .restore_plan(editor.text(), *id, all);
        match plan {
            Ok(edits) => {
                let restored = self
                    .pseudonymization
                    .review
                    .tracking
                    .prepare_restore(&edits);
                if self
                    .editor
                    .update(cx, |e, cx| e.replace_ranges(revision, &edits, cx))
                {
                    let transaction = self.editor.read(cx).last_transaction().cloned().unwrap();
                    self.pii_transaction(&transaction, cx);
                    self.pseudonymization
                        .review
                        .tracking
                        .commit_restore(self.editor.read(cx).history_id(), restored);
                    self.pseudonymization_edited(cx);
                    self.pseudonymization.error = None;
                    window.focus(&self.editor.read(cx).focus_handle(cx), cx);
                }
            }
            Err(error) => self.pseudonymization.error = Some(error),
        }
        cx.notify();
    }
    pub(super) fn close_pseudonym_popup(
        &mut self,
        _: &ClosePseudonymPopup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pseudonymization.popup = None;
        if let Some(focus) = self.pseudonymization.popup_previous.take() {
            window.focus(&focus, cx);
        } else {
            window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }
    fn close_review_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pseudonymization.menu_open = false;
        if let Some(focus) = self.pseudonymization.menu_previous.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }
    fn close_pii_mode_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pseudonymization.mode_menu_open = false;
        if let Some(focus) = self.pseudonymization.mode_menu_previous.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }
}
