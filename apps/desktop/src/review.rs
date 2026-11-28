//! The pseudonymization review UI. The review model is `mdoc_pii`; offline
//! detection and its setup are `mdoc_detect`.
//! The review's state on a tab, its popup and its keys; what it does is
//! in `actions`, its highlights in `highlights`, its toolbar in `view`.
mod actions;
mod chooser;
mod compare;
mod decisions;
mod project_sync;
pub(crate) use compare::CompareMode;
pub(crate) use compare::{CompareState, SwitcherRest};
mod discovery;
mod edit_scan;
mod hand_copy;
mod highlights;
#[cfg(test)]
pub(crate) use edit_scan::TEST_SCAN;
pub(crate) use hand_copy::{CopyFrom, check_hand_copies};
mod mapping;
pub(crate) mod refusal;
mod reply;
mod view;
pub(crate) use reply::{ReplyBox, SavedReplyRow, reply_marks};
pub(crate) mod restore;
pub(crate) mod restored_clipboard;
mod scan;
mod text;
pub(crate) use restore::UNRESOLVED_ID;
use scan::ScanJob;
#[cfg(test)]
mod tests;

use crate::document::DocumentView;
use gpui::{App, Context, FocusHandle, KeyBinding, Pixels, actions, px};
use mdoc_pii::Review;
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
    /// The popup was opened by a manual addition, so Enter applies.
    enter_applies: bool,
    /// The next activation comes from a manual addition (alias cue choice).
    manual_activation: bool,
    /// The selection pill's category menu is open for this selection;
    /// it closes when the selection changes.
    selection_menu: Option<Range<usize>>,
    selection_menu_focus: FocusHandle,
    focus: FocusHandle,
    job: Option<ScanJob>,
    /// When the running scan started, for the toolbar stage label.
    scan_started: std::time::Instant,
    discovery_job: Option<discovery::DiscoveryJob>,
    discovery_pending: bool,
    edit_scan: edit_scan::EditScan,
    scans: Vec<settings::PiiConfig>,
    error: Option<String>,
    /// The panel shows the setup consent card instead of a review.
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
    /// screen in the frame being built.
    /// The mention bar's size at its last layout, for placing it.
    popup_size: Rc<Cell<gpui::Size<Pixels>>>,
    popup_layer: PopupLayer,
    chooser_scroll: gpui::UniformListScrollHandle,
    popup_controls: std::cell::RefCell<std::collections::HashMap<gpui::ElementId, FocusHandle>>,
    /// What you did: memory only, as long as the tab.
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
    pub fn new(theme: Rc<Cell<crate::ui::style::Theme>>, cx: &mut Context<DocumentView>) -> Self {
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
            edit_scan: Default::default(),
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
            decisions: Default::default(),
            clock: decisions::local_time,
        }
    }
    fn cancel(&mut self) {
        if let Some(job) = &self.discovery_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.discovery_pending = false;
        self.edit_scan.cancel();
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

    /// The panel shows the setup consent card instead of a review.
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

    pub(crate) fn scanning(&self) -> bool {
        self.job.is_some()
    }
    /// Discovery or the edited paragraphs' scan is waiting or running: the
    /// review may still gain proposals.
    pub(crate) fn finding_names(&self) -> bool {
        self.discovery_job.is_some() || self.discovery_pending || self.edit_scan.pending()
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

/// The Replacements checklist: Space ticks, Enter jumps to the
/// mention, →/← show and hide mentions, ⌫ keeps the original.
pub(crate) const LIST: &str =
    "IdentityPanel && !ReplacementSearch && !SettingsInput && !UiControl && !ReplyBox && !NotesBox";
/// The mention bar: Enter applies in the selected scope and moves on, Space
/// unticks in it, ⌫ / ⌘⌫ / ⇧⌘⌫ keep this / same wording / all.
pub(crate) const POPUP: &str = "PseudonymReview && !PseudonymReplacement && !UiControl && !UiMenu";
/// ← → step through the entity's mentions, ⌘← ⌘→ to its ends; the alias
/// field and menus keep their own arrows.
pub(crate) const STEP: &str = "PseudonymReview && !PseudonymReplacement && !PiiChooser && !UiMenu";
/// Document undo in the popup; the alias field keeps its own text undo.
pub(crate) const POPUP_UNDO: &str = "PseudonymReview && !PseudonymReplacement";

/// The review keys come from the shortcuts table; moving inside
/// the chooser and the popup's and panel's menus stays here. Registered
/// after the table's popup keys: the chooser is also `PseudonymReview`, and
/// the later binding wins its Enter.
pub(crate) fn bind_keys(cx: &mut App) {
    let bindings = crate::commands::shortcuts::bindings(crate::commands::shortcuts::Owner::Pii, cx);
    cx.bind_keys(bindings);
    cx.bind_keys([
        // This multiline field is a Share control: Tab moves between
        // controls instead of indenting the pasted reply.
        KeyBinding::new("tab", crate::ui::NextControl, Some("ReplyBox > Editor")),
        KeyBinding::new(
            "shift-tab",
            crate::ui::PreviousControl,
            Some("ReplyBox > Editor"),
        ),
        // The Notes field too: Tab and Shift+Tab move on (Esc closes the
        // Inspector, from the shortcuts table).
        KeyBinding::new("tab", crate::ui::NextControl, Some("NotesBox > Editor")),
        KeyBinding::new(
            "shift-tab",
            crate::ui::PreviousControl,
            Some("NotesBox > Editor"),
        ),
        KeyBinding::new("down", PiiNextChoice, Some("PiiChooser")),
        KeyBinding::new("up", PiiPreviousChoice, Some("PiiChooser")),
        KeyBinding::new("enter", PiiOpenChoice, Some("PiiChooser")),
        // In the popup's and panel's menus ↑ ↓ move between rows, like Tab.
        KeyBinding::new("up", crate::ui::PreviousControl, Some("PiiMenu")),
        KeyBinding::new("down", crate::ui::NextControl, Some("PiiMenu")),
    ]);
}
