//! Live-document PII state, scan lifecycle and replacement actions.
mod render;
#[cfg(test)]
mod tests;

use crate::{
    AcceptAllPseudonyms, AcceptPseudonymCandidate, AddPseudonymCandidate, Anonymize,
    ClosePseudonymPopup, KeepPseudonymCandidate, NextCandidate, PreviousCandidate, Pseudonymize,
    ReviewCandidate, Workspace, markdown_search,
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

pub(super) struct Popup {
    pub group: u64,
    pub mention: Range<usize>,
    pub all: bool,
    context_open: bool,
    links_open: bool,
}
/// A review scan proposes edits; the explicit Anonymize action applies them.
#[derive(Clone, Copy)]
enum ScanIntent {
    Review(Mode),
    Anonymize,
}
impl ScanIntent {
    fn mode(self) -> Mode {
        match self {
            Self::Review(mode) => mode,
            Self::Anonymize => Mode::Anonymize,
        }
    }
}
struct ScanJob {
    cancel: Arc<AtomicBool>,
    revision: u64,
    generation: u64,
    config: settings::PiiConfig,
    intent: ScanIntent,
}
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
            popup_controls: Default::default(),
        }
    }
    fn cancel(&mut self) {
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
        self.pseudonymization
            .review
            .refresh(self.editor.read(cx).text());
        self.sync_annotations(cx);
    }
    pub(super) fn sync_pseudonym_theme(&mut self, cx: &mut Context<Self>) {
        self.sync_annotations(cx);
    }
    fn sync_annotations(&mut self, cx: &mut Context<Self>) {
        let accent = style::markdown_style(self.theme.get()).alert_warning;
        let annotations = if self.pseudonymization.review.open
            && (self.pseudonymization.review.mode == Mode::Pseudonymize
                || self.pseudonymization.manual_review)
        {
            self.pseudonymization
                .review
                .groups
                .iter()
                .flat_map(|group| {
                    group
                        .mentions
                        .iter()
                        .enumerate()
                        .map(move |(index, range)| mdoc_editor::SourceAnnotation {
                            id: (group.id << 32) | index as u64,
                            range: range.clone(),
                            color: Hsla { a: 0.14, ..accent },
                            active_color: Hsla { a: 0.3, ..accent },
                        })
                })
                .collect()
        } else {
            Vec::new()
        };
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
    fn start_pii_scan(&mut self, intent: ScanIntent, cx: &mut Context<Self>) {
        if !self.can_copy_markdown() {
            return;
        }
        self.pseudonymization.cancel();
        self.pseudonymization.error = None;
        self.pseudonymization.completion = None;
        self.pseudonymization.popup = None;
        self.pseudonymization.review.open = true;
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        let source = editor.text().to_owned();
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.pseudonymization,
            Err(e) => {
                self.pseudonymization.error = Some(e);
                cx.notify();
                return;
            }
        };
        let generation = self.pseudonymization.generation;
        let identity = self.session.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.pseudonymization.job = Some(ScanJob {
            cancel: cancel.clone(),
            revision,
            generation,
            config: config.clone(),
            intent,
        });
        let task = cx
            .background_executor()
            .spawn(async move { detector::scan_config(&source, &cancel, &config) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.complete_pseudonym_scan(generation, identity, revision, result, cx)
            });
        })
        .detach();
        cx.notify();
    }
    fn complete_pseudonym_scan(
        &mut self,
        generation: u64,
        identity: u64,
        revision: u64,
        result: Result<Vec<pseudonymization::Detection>, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(job) = &self.pseudonymization.job else {
            return;
        };
        if job.generation != generation
            || job.revision != revision
            || self.session.generation != identity
            || self.editor.read(cx).revision() != revision
            || job.intent.mode() != self.pseudonymization.review.mode
            || job.cancel.load(Ordering::Relaxed)
        {
            return;
        }
        let config = job.config.clone();
        let intent = job.intent;
        self.pseudonymization.job = None;
        let source = self.editor.read(cx).text().to_owned();
        match result.and_then(|detections| self.pseudonymization.review.ingest(&source, detections))
        {
            Ok(()) => {
                self.pseudonymization.error = None;
                self.model_panel.update(cx, |panel, cx| {
                    let model = settings::Model::Pii(config.model);
                    let index = settings::Model::ALL
                        .iter()
                        .position(|m| *m == model)
                        .unwrap();
                    panel.statuses[index] = settings_ui::Status::Ready;
                    cx.notify();
                });
                self.pseudonymization.scans.push(config);
                if matches!(intent, ScanIntent::Anonymize) {
                    match self.commit_all_pii(None, cx) {
                        Ok(count) => {
                            self.pseudonymization.completion =
                                Some((count, self.editor.read(cx).revision()))
                        }
                        Err(error) => self.pseudonymization.error = Some(error),
                    }
                }
                self.sync_annotations(cx);
            }
            Err(error) => {
                self.pseudonymization.error = Some(error);
            }
        }
        cx.notify();
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
        if !self.pseudonymization.review.open {
            return;
        }
        let group_id = id >> 32;
        let index = (id & u32::MAX as u64) as usize;
        let Some(group) = self.pseudonymization.review.group(group_id) else {
            return;
        };
        let Some(mention) = group.mentions.get(index).cloned() else {
            return;
        };
        let replacement = self.pseudonymization.review.replacement(group);
        self.pseudonymization.error = None;
        self.pseudonymization.popup = Some(Popup {
            group: group_id,
            mention,
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
        if !self.pseudonymization.review.open {
            return;
        }
        let cursor = self.editor.read(cx).cursor();
        let mut mentions: Vec<_> = self
            .pseudonymization
            .review
            .groups
            .iter()
            .flat_map(|group| {
                group
                    .mentions
                    .iter()
                    .enumerate()
                    .map(move |(index, range)| ((group.id << 32) | index as u64, range.clone()))
            })
            .collect();
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
                let index = self
                    .pseudonymization
                    .review
                    .group(id)
                    .unwrap()
                    .mentions
                    .iter()
                    .position(|mention| *mention == range)
                    .unwrap_or(0);
                self.activate_annotation((id << 32) | index as u64, window, cx);
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
        let id = popup.group;
        let single = (!popup.all).then(|| popup.mention.clone());
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
            self.pseudonymization.popup.as_ref().map(|popup| {
                (
                    popup.group,
                    self.pseudonymization.input.read(cx).value().to_owned(),
                )
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
        if !self
            .editor
            .update(cx, |editor, cx| editor.replace_ranges(revision, edits, cx))
        {
            return false;
        }
        for (id, replacement) in accepted {
            self.pseudonymization
                .review
                .record_acceptance(*id, replacement);
        }
        self.pseudonymization
            .review
            .refresh_after_edits(self.editor.read(cx).text(), edits);
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
        self.pseudonymization
            .review
            .keep(popup.group, (!popup.all).then_some(popup.mention));
        self.sync_annotations(cx);
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
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
