//! The companion window ([ADR 0128](../docs/adr/0128-floating-companion-window.md);
//! Milestone 2 item 14), macOS, while "AI connector (preview)" is on: one
//! small window floating above other apps, under the menu bar item, which
//! toggles it in place of the shelf popup. It holds a fixed set:
//!
//! - the AI project, with Stop sharing, or that nothing is shared;
//! - the latest review request: its proposals, real names shown only while
//!   the pointer is over them as the reply's are, and Replace all, or Open
//!   in mdoc for the rest (item 16);
//! - the AI project's documents by AI label, with their state;
//! - the latest reply, restored, with Copy and Save as document;
//! - a drop target, which imports into the AI project;
//! - the shelf's rows ([`ShelfRows`]), with their Copy for AI and drag;
//! - Open mdoc, Settings and Quit, as the shelf has them.
//!
//! It opens by itself only for a delivery or a review request, and never
//! takes focus: a click on it leaves the app the user is in active and
//! keeps its keyboard, and only what opens mdoc (a row, Open in mdoc,
//! Open mdoc, Settings, a drop) brings mdoc forward; Replace all runs in
//! mdoc's window behind other apps. It asks to be left out
//! of screen capture (`NSWindowSharingNone`, which not every capture path
//! honours), and the restored reply shows only while the pointer is over
//! it: until then the window holds a placeholder, not the text.
//! `view.rs` draws it, `actions.rs` carries out what it does, `native.rs`
//! makes the window and `hotkey.rs` registers its system-wide hotkey;
//! Personal and search (ADR 0128) are not here yet.

mod actions;
pub(crate) mod hotkey;
#[cfg(all(target_os = "macos", debug_assertions, not(test)))]
pub(crate) mod hotkey_probe;
#[cfg(all(target_os = "macos", not(test)))]
mod native;
#[cfg(all(target_os = "macos", debug_assertions, not(test)))]
pub(crate) mod probe;
// Windows (item 17): the same window by the tray icon ([`crate::shelf::tray`]).
#[cfg(all(windows, debug_assertions, not(test)))]
pub(crate) mod probe_windows;
mod view;
#[cfg(all(windows, not(test)))]
pub(crate) mod windows;

use crate::{
    shelf::shelf_ui::{Fit, ShelfRows},
    ui::style::Theme,
    workspace::{CompanionContent, CompanionProposal, ReviewMark, ReviewStep, Workspace},
};
use gpui::{
    App, Bounds, Context, DisplayId, Entity, EntityId, FocusHandle, Pixels, Render, Subscription,
    Task, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};
use std::{cell::Cell, rc::Rc};

/// The companion's width: the shelf's.
pub(crate) const COMPANION_WIDTH: f32 = crate::shelf::menu_bar::SHELF_WIDTH;

const PADDING: f32 = 8.;
const HEADER: f32 = 22.;
const PROJECT_LINE: f32 = 30.;
const SECTION_GAP: f32 = 6.;
const HEADING: f32 = 22.;
const DOCUMENT_ROW: f32 = 22.;
const REVIEW: f32 = 100.;
/// A review card with nothing to do: its title and one line.
const REVIEW_READY: f32 = 60.;
const PROPOSAL_ROW: f32 = 20.;
/// Proposals shown at once; more scroll.
const PROPOSAL_ROWS: usize = 5;
const PROPOSALS_PADDING: f32 = 6.;
/// A review's proposals are real names: they show only while the pointer
/// is over them, as the restored reply does. The one switch for it.
const PROPOSALS_HIDDEN_UNTIL_HOVER: bool = true;
const REPLY_TEXT: f32 = 120.;
const BUTTONS: f32 = 32.;
const DROP: f32 = 44.;
const DIVIDER: f32 = 9.;
const FOOTER: f32 = 30.;
/// What stays in view above the scrolling part.
const HEADER_TOTAL: f32 = PADDING + HEADER + PROJECT_LINE;

/// Open a companion window: a non-activating panel above other apps that
/// never takes focus, excluded from capture (`native.rs`), at `bounds` on
/// `display`. The menu bar item's companion and the probe open it here.
pub(crate) fn open<V: Render + 'static>(
    bounds: Bounds<Pixels>,
    display: DisplayId,
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> gpui::Result<WindowHandle<V>> {
    let window = cx.open_window(window_options(bounds, display), move |window, cx| {
        // Before its first frame (`windows.rs`).
        #[cfg(all(windows, not(test)))]
        windows::float(window, bounds);
        build(window, cx)
    })?;
    // Never up where a capture would see it: notifications and the Reply
    // page remain.
    #[cfg(all(windows, not(test)))]
    if !window.update(cx, |_, window, _| windows::excluded(window))? {
        window.update(cx, |_, window, _| window.remove_window())?;
        return Err(std::io::Error::other("the companion cannot be left out of capture").into());
    }
    #[cfg(all(target_os = "macos", not(test)))]
    window.update(cx, |_, window, _| native::float(window))?;
    Ok(window)
}

/// The companion's window: a popup kind (on macOS GPUI's non-activating
/// `NSPanel` above other apps) that never asks for focus.
pub(crate) fn window_options(bounds: Bounds<Pixels>, display: DisplayId) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        focus: false,
        // Shown by `native::float` once it cannot be captured.
        show: !cfg!(all(target_os = "macos", not(test))),
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        display_id: Some(display),
        window_background: WindowBackgroundAppearance::Blurred,
        ..Default::default()
    }
}

/// The review card's height in `step`.
fn review_height(step: &ReviewStep) -> f32 {
    match step {
        ReviewStep::Proposals(proposals) | ReviewStep::Replacing(proposals) => {
            REVIEW + SECTION_GAP + proposals_height(proposals)
        }
        ReviewStep::Ready => REVIEW_READY,
        ReviewStep::InMdoc(_) => REVIEW,
    }
}

/// The proposals' box, hidden or shown alike, so a hover never moves it.
fn proposals_height(proposals: &[CompanionProposal]) -> f32 {
    2. * PROPOSALS_PADDING + proposals.len().clamp(1, PROPOSAL_ROWS) as f32 * PROPOSAL_ROW
}

/// The review card's document and the proposals it lists, if any.
fn proposals(content: &CompanionContent) -> Option<(&str, &[CompanionProposal])> {
    let review = content.review.as_ref()?;
    match &review.step {
        ReviewStep::Proposals(proposals) | ReviewStep::Replacing(proposals) => {
            Some((review.document.as_str(), proposals))
        }
        ReviewStep::Ready | ReviewStep::InMdoc(_) => None,
    }
}

/// The companion's height for `content`, before the screen limits it and
/// before its first layout corrects it.
pub(crate) fn height(content: &CompanionContent) -> f32 {
    let section = |shown: bool, height: f32| {
        if shown { SECTION_GAP + height } else { 0. }
    };
    let documents = match &content.documents {
        Ok(documents) => documents.len().max(1),
        Err(_) => 1,
    };
    2. * PADDING
        + HEADER
        + PROJECT_LINE
        + (content.review.as_ref()).map_or(0., |review| SECTION_GAP + review_height(&review.step))
        + section(
            content.shared.is_some(),
            HEADING + documents as f32 * DOCUMENT_ROW,
        )
        + section(content.reply.is_some(), HEADING + REPLY_TEXT + BUTTONS)
        + section(true, DROP)
        + DIVIDER
        + crate::shelf::shelf_ui::rows_height(&content.rows, content.drag_hint)
        + DIVIDER
        + FOOTER
}

pub(crate) struct CompanionView {
    workspace: WindowHandle<Workspace>,
    theme: Rc<Cell<Theme>>,
    content: CompanionContent,
    rows: Entity<ShelfRows>,
    _rows_events: Subscription,
    /// The pointer is over the reply: its restored text shows, unless
    /// the pointer has left the window since (`pointer_away`).
    reply_shown: bool,
    /// The pointer left the window and has not moved inside it again:
    /// GPUI keeps its last place, so a redraw could find the reply under
    /// it and show it again.
    pointer_away: bool,
    /// The pointer is over the review's proposals: they show, unless the
    /// pointer has left the window since.
    proposals_hovered: bool,
    /// What the window last shared and asked to review, and that
    /// document's review: when any changes, the companion reads the
    /// window again.
    marks: (Option<String>, Option<String>, ReviewMark),
    _workspace_changes: Option<Subscription>,
    /// The requested document's tab, watched while a tab has it: a scan
    /// there changes what the card can do.
    review_watch: Option<(EntityId, Subscription)>,
    /// "Copied" shows on the reply's Copy until this ends.
    copied: Option<Task<()>>,
    /// A row's "Copied" shows until this ends.
    row_copied: Option<Task<()>>,
    /// Hide, where focus starts inside the window. The panel never takes
    /// the keyboard from other apps; this keeps its Tab order whole for
    /// VoiceOver and the renders' Tab walk.
    hide_focus: FocusHandle,
    /// Set when the companion is its own window: it follows its content.
    fit: Option<Rc<Fit>>,
    /// Renders show the content they were given: nothing reads the window
    /// again.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    frozen: bool,
    /// Where the reply's text box was last painted, for renders to hover.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    reply_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Where the review's proposals were last painted, for renders to
    /// hover.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    proposals_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl CompanionView {
    pub(crate) fn new(
        workspace: WindowHandle<Workspace>,
        content: CompanionContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = workspace
            .read(cx)
            .map(|workspace| workspace.theme_cell())
            .unwrap_or_default();
        let (rows, rows_events) = Self::rows(workspace, &content, window, cx);
        let hide_focus = cx.focus_handle().tab_index(0).tab_stop(true);
        window.focus(&hide_focus, cx);
        let marks = workspace
            .read(cx)
            .map(|workspace| workspace.companion_marks(cx))
            .unwrap_or_default();
        // Opening a requested document from the sidebar, or a share from
        // the Project page, changes what it shows.
        let workspace_changes = workspace.entity(cx).ok().map(|root| {
            cx.observe_in(&root, window, |this: &mut Self, _, window, cx| {
                this.marks_changed(window, cx)
            })
        });
        let mut view = Self {
            workspace,
            theme,
            content,
            rows,
            _rows_events: rows_events,
            reply_shown: false,
            pointer_away: false,
            proposals_hovered: false,
            marks,
            _workspace_changes: workspace_changes,
            review_watch: None,
            copied: None,
            row_copied: None,
            hide_focus,
            fit: None,
            #[cfg(all(
                debug_assertions,
                feature = "headless-renders",
                any(target_os = "macos", target_os = "windows")
            ))]
            frozen: false,
            #[cfg(all(
                debug_assertions,
                feature = "headless-renders",
                any(target_os = "macos", target_os = "windows")
            ))]
            reply_bounds: Rc::default(),
            #[cfg(all(
                debug_assertions,
                feature = "headless-renders",
                any(target_os = "macos", target_os = "windows")
            ))]
            proposals_bounds: Rc::default(),
        };
        view.watch_review(window, cx);
        view
    }

    /// The companion is its own window, `height` tall with at most `max`
    /// under the item: from now on it follows its content's height.
    pub(crate) fn fit_to_screen(&mut self, height: f32, max: f32) {
        self.fit = Some(Rc::new(Fit::new(height, max)));
    }

    /// What it shows now, for tests and renders.
    #[cfg(any(
        all(test, target_os = "macos"),
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn content(&self) -> &CompanionContent {
        &self.content
    }

    /// Whether the review's proposals show.
    #[cfg(any(
        all(test, target_os = "macos"),
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn proposals_shown(&self) -> bool {
        self.shows_proposals()
    }

    /// The middle of the review's proposals as last painted, for renders.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn proposals_center(&self) -> Option<gpui::Point<Pixels>> {
        self.proposals_bounds.get().map(|bounds| bounds.center())
    }

    /// Whether the restored reply shows (the pointer is over it).
    #[cfg(any(
        all(test, target_os = "macos"),
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn reply_shown(&self) -> bool {
        self.shows_reply()
    }

    /// Keep the content given, for renders.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn freeze(&mut self) {
        self.frozen = true;
    }

    /// The middle of the reply's text box as last painted, for renders.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn reply_center(&self) -> Option<gpui::Point<Pixels>> {
        self.reply_bounds.get().map(|bounds| bounds.center())
    }

    /// The rows, for tests.
    #[cfg(test)]
    pub(crate) fn shelf_rows(&self) -> &Entity<ShelfRows> {
        &self.rows
    }

    fn set_reply_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        if self.reply_shown != shown {
            self.reply_shown = shown;
            cx.notify();
        }
    }

    /// Whether the restored reply is in the window now.
    fn shows_reply(&self) -> bool {
        self.reply_shown && !self.pointer_away
    }

    fn set_proposals_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.proposals_hovered != hovered {
            self.proposals_hovered = hovered;
            cx.notify();
        }
    }

    /// Whether the review's proposals are in the window now.
    fn shows_proposals(&self) -> bool {
        !PROPOSALS_HIDDEN_UNTIL_HOVER || (self.proposals_hovered && !self.pointer_away)
    }

    /// The pointer left the window: the reply hides until it moves inside
    /// again.
    fn pointer_left(&mut self, cx: &mut Context<Self>) {
        self.pointer_away = true;
        self.reply_shown = false;
        self.proposals_hovered = false;
        cx.notify();
    }

    fn pointer_moved(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.pointer_away) {
            cx.notify();
        }
    }
}
