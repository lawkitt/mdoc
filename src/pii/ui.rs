//! Live-document PII state, scan lifecycle and replacement actions.
mod case_sync;
mod chooser;
mod compare;
pub(crate) use compare::{CompareState, SwitcherRest};
mod discovery;
mod mapping;
mod render;
pub(crate) mod restore;
mod scan;
pub(crate) use restore::RestoredReply;
use scan::ScanJob;
#[cfg(test)]
mod tests;

use crate::{
    DocumentView, PiiAddCandidate, PiiApplyAll, PiiClosePopup, PiiConfirm, PiiNextCandidate,
    PiiPreviousCandidate, PiiReviewCandidate, Pseudonymize, markdown_search,
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
/// Window origin of the popup layer and the scroll offset it was laid out at.
type PopupLayer = Rc<Cell<Option<(gpui::Point<Pixels>, gpui::Point<Pixels>)>>>;
/// Annotation ids of kept-mention markers carry this bit over the start offset.
const KEPT_ID: u64 = 1 << 61;
pub(crate) struct ReviewUi {
    /// Derived data (panel projection, before/after view, spelling
    /// exclusions) is keyed on `review.revision()`, which its mutators advance.
    pub review: Review,
    /// Candidate highlights are shown; Close review hides them.
    pub reviewing: bool,
    pub popup: Option<Popup>,
    /// The popup was opened by a manual addition, so Enter applies (ADR 0024).
    pub enter_applies: bool,
    /// The next activation comes from a manual addition (alias cue choice).
    manual_activation: bool,
    pub focus: FocusHandle,
    job: Option<ScanJob>,
    /// When the running scan started, for the toolbar stage label.
    scan_started: std::time::Instant,
    discovery_job: Option<discovery::DiscoveryJob>,
    discovery_pending: bool,
    pub scans: Vec<settings::PiiConfig>,
    pub error: Option<String>,
    /// The panel shows the setup consent card instead of a review (ADR 0026).
    pub setup: bool,
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
    pub(crate) popup_layer: PopupLayer,
    chooser_scroll: gpui::UniformListScrollHandle,
    popup_controls: std::cell::RefCell<std::collections::HashMap<gpui::ElementId, FocusHandle>>,
    /// Set in a tab opened by Paste AI Response (ADR 0041).
    pub(crate) restore: Option<restore::RestoreState>,
}
impl Drop for ReviewUi {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl ReviewUi {
    pub fn new(cx: &mut Context<DocumentView>) -> Self {
        Self {
            review: Review::default(),
            reviewing: false,
            popup: None,
            enter_applies: false,
            manual_activation: false,
            focus: cx.focus_handle(),
            job: None,
            scan_started: std::time::Instant::now(),
            discovery_job: None,
            discovery_pending: false,
            scans: Vec::new(),
            error: None,
            setup: false,
            setup_subscription: None,
            mapping: mapping::MappingUi::new(cx),
            generation: 0,
            popup_previous: None,
            popup_scroll: gpui::ScrollHandle::new(),
            popup_anchor: Cell::new(None),
            popup_layer: Default::default(),
            popup_size: Rc::new(Cell::new(gpui::size(px(480.), px(36.)))),
            chooser_scroll: gpui::UniformListScrollHandle::new(),
            popup_controls: Default::default(),
            restore: None,
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

pub(crate) fn bind_keys(cx: &mut App) {
    const STEP: &str = "PseudonymReview && !PseudonymReplacement && !PiiChooser && !UiMenu";
    // The alias field keeps its own text undo.
    const POPUP_UNDO: &str = "PseudonymReview && !PseudonymReplacement";
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.bind_keys(decision_bindings(modifier));
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
        // In the alias field Enter confirms the draft; elsewhere in the popup
        // the shared decision keys apply (ADR 0033).
        KeyBinding::new("enter", PiiConfirm, Some("PseudonymReplacement")),
        // ← → step through the entity's mentions, ⌘← ⌘→ to its ends; the
        // alias field and menus keep their own arrows (ADR 0055).
        KeyBinding::new("left", crate::PopupPreviousMention, Some(STEP)),
        KeyBinding::new("right", crate::PopupNextMention, Some(STEP)),
        KeyBinding::new("up", crate::PopupPreviousMention, Some(STEP)),
        KeyBinding::new("down", crate::PopupNextMention, Some(STEP)),
        KeyBinding::new(
            &format!("{modifier}-up"),
            crate::PopupFirstMention,
            Some(STEP),
        ),
        KeyBinding::new(
            &format!("{modifier}-down"),
            crate::PopupLastMention,
            Some(STEP),
        ),
        KeyBinding::new(
            &format!("{modifier}-left"),
            crate::PopupFirstMention,
            Some(STEP),
        ),
        KeyBinding::new(
            &format!("{modifier}-right"),
            crate::PopupLastMention,
            Some(STEP),
        ),
        KeyBinding::new("escape", PiiClosePopup, Some("PseudonymReview")),
        // Esc leaves before/after, also from inside its read-only columns.
        KeyBinding::new("escape", crate::CloseCompare, Some("CompareView")),
        KeyBinding::new("escape", crate::CloseCompare, Some("CompareView > Editor")),
        KeyBinding::new("escape", crate::PanelEscape, Some("IdentityPanel")),
        KeyBinding::new("down", crate::PanelEnterList, Some("ReplacementSearch")),
        // Document undo from anywhere in the replacements panel (ADR 0033).
        KeyBinding::new("cmd-z", mdoc_editor::Undo, Some("IdentityPanel")),
        KeyBinding::new("ctrl-z", mdoc_editor::Undo, Some("IdentityPanel")),
        KeyBinding::new("cmd-shift-z", mdoc_editor::Redo, Some("IdentityPanel")),
        KeyBinding::new("ctrl-shift-z", mdoc_editor::Redo, Some("IdentityPanel")),
        KeyBinding::new("ctrl-y", mdoc_editor::Redo, Some("IdentityPanel")),
        KeyBinding::new("cmd-z", mdoc_editor::Undo, Some(POPUP_UNDO)),
        KeyBinding::new("ctrl-z", mdoc_editor::Undo, Some(POPUP_UNDO)),
        KeyBinding::new("cmd-shift-z", mdoc_editor::Redo, Some(POPUP_UNDO)),
        KeyBinding::new("ctrl-shift-z", mdoc_editor::Redo, Some(POPUP_UNDO)),
        KeyBinding::new("ctrl-y", mdoc_editor::Redo, Some(POPUP_UNDO)),
    ]);
}

/// The panel list's movement keys and the decision keys shared by the panel
/// list and the popup (ADR 0033): Enter applies, Delete keeps the original;
/// ⌘ widens to the same text, ⇧⌘ to the whole entity.
fn decision_bindings(modifier: &str) -> Vec<KeyBinding> {
    const LIST: &str = "IdentityPanel && !ReplacementSearch && !SettingsInput && !UiControl";
    const POPUP: &str = "PseudonymReview && !PseudonymReplacement && !UiControl && !UiMenu";
    // The checklist (ADR 0048): Space ticks, Enter jumps to the mention,
    // →/← show and hide mentions, ⌫ keeps the original; ⌘↩ applies all.
    let mut bindings = vec![
        KeyBinding::new("up", crate::PanelUp, Some(LIST)),
        KeyBinding::new("down", crate::PanelDown, Some(LIST)),
        KeyBinding::new(&format!("{modifier}-up"), crate::PanelFirst, Some(LIST)),
        KeyBinding::new(&format!("{modifier}-down"), crate::PanelLast, Some(LIST)),
        KeyBinding::new("alt-up", crate::PanelMoveUp, Some(LIST)),
        KeyBinding::new("alt-down", crate::PanelMoveDown, Some(LIST)),
        KeyBinding::new("alt-escape", crate::PanelEscape, Some("IdentityPanel")),
        KeyBinding::new("space", crate::PanelToggle, Some(LIST)),
        KeyBinding::new("enter", crate::PanelOpen, Some(LIST)),
        KeyBinding::new("right", crate::PanelExpand, Some(LIST)),
        KeyBinding::new("left", crate::PanelCollapse, Some(LIST)),
        KeyBinding::new("backspace", crate::PanelKeep, Some(LIST)),
        KeyBinding::new("delete", crate::PanelKeep, Some(LIST)),
        KeyBinding::new(&format!("{modifier}-enter"), PiiApplyAll, None),
    ];
    // The mention bar: Enter applies in the selected scope and moves on,
    // Space unticks in it, ⌘↩ applies everything (ADR 0055).
    bindings.extend([
        KeyBinding::new("enter", crate::ApplyThis, Some(POPUP)),
        KeyBinding::new("space", crate::PanelKeep, Some(POPUP)),
    ]);
    for key in ["backspace", "delete"] {
        bindings.extend([
            KeyBinding::new(key, crate::KeepThis, Some(POPUP)),
            KeyBinding::new(&format!("{modifier}-{key}"), crate::KeepSame, Some(POPUP)),
            KeyBinding::new(
                &format!("{modifier}-shift-{key}"),
                crate::KeepAll,
                Some(POPUP),
            ),
        ]);
    }
    bindings
}

impl DocumentView {
    pub(crate) fn reset_pii(&mut self, cx: &mut Context<Self>) {
        self.pii.cancel();
        self.pii.review = Review::default();
        self.pii.scans.clear();
        self.pii.dismiss_popup();
        self.pii.error = None;
        self.pii.mapping = mapping::MappingUi::new(cx);
        self.pii.restore = None;
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
                    text_color: Some(applied_accent),
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
        } else if self.source_only {
            "source only"
        } else if self.preview.pdf.is_some() {
            "PDF"
        } else if self.preview.source.is_some() {
            "converted"
        } else {
            "Markdown"
        };
        let mut parts = vec![format!(
            "{kind}, {} bytes, zoom {}%",
            self.editor.read(cx).text().len(),
            (self.zoom * 100.).round() as i32
        )];
        if self.preview.visible {
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
        if self.search.open {
            parts.push("find open".into());
        }
        parts.join(", ")
    }
    /// Whether the replacements panel is open (ADR 0048 header toggle).
    pub(crate) fn replacements_open(&self) -> bool {
        self.pii.mapping.open
    }
    /// Download the selected model, then scan this document once if it is
    /// still the same document. Settings stays closed.
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
    /// Offer Replace beside the settled editor selection (ADR 0024).
    pub(crate) fn sync_selection_action(&mut self, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        // The editor hides the pill while the popup holds focus.
        let action = (!editor.is_selecting())
            .then(|| self.manual_target(cx))
            .flatten()
            .map(|target| mdoc_editor::SelectionAction {
                range: editor.selection(),
                label: "Replace".into(),
                menu_label: "Replace with placeholder".into(),
                shortcut: if cfg!(target_os = "macos") {
                    "⌘⌥P"
                } else {
                    "Ctrl+Alt+P"
                }
                .into(),
                disabled: target.err().map(Into::into),
                accent: self.theme.get().proposed(),
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
        let Some(Ok(range)) = self.manual_target(cx) else {
            return;
        };
        let source = self.editor.read(cx).text().to_owned();
        let category = detector::guess_category(&source, range.clone());
        let Some(id) = self.checkpoint_review(cx, |review| {
            review
                .add_manual(&source, range.clone(), category)
                .expect("validated unchanged selection")
        }) else {
            return;
        };
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
            // A fallback guess also cues the category chip.
            self.pii
                .mapping
                .request_cue(true, matches!(category, Category::Person | Category::Other));
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
            // Nothing is left to review, so the popup closes (ADR 0055).
            if self.pii.popup.is_some() && self.pii.review.remaining() == 0 {
                self.close_pii_popup(&PiiClosePopup, window, cx);
            }
            // Inspector (ADR 0048 variant C): next comes sharing.
            if self.pii.review.remaining() == 0 && self.has_applied_aliases() {
                self.inspector_share = true;
            }
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
        if transaction
            .changes
            .iter()
            .any(|change| change.edits.is_empty())
        {
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
