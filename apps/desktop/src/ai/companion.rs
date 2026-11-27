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
//! `native.rs` makes the window and `hotkey.rs` registers its system-wide
//! hotkey; Personal and search (ADR 0128) are not here yet.

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
#[cfg(all(windows, not(test)))]
pub(crate) mod windows;

use crate::{
    shelf::ShelfKey,
    shelf::menu_bar,
    shelf::shelf_ui::{COPIED_FOR, Fit, ShelfRows, ShelfRowsEvent},
    ui,
    ui::style::{self, Theme},
    workspace::{CompanionContent, CompanionProposal, Quit, ReviewMark, ReviewStep, Workspace},
};
use gpui::{
    AnyElement, App, Bounds, Context, DispatchPhase, DisplayId, Entity, EntityId, ExternalPaths,
    FocusHandle, MouseExitEvent, MouseMoveEvent, Pixels, Render, SharedString, Subscription, Task,
    Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div,
    prelude::*, px,
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

    /// The window changed: read it again if what it shares, what it asks
    /// to review, or that document's review changed.
    fn marks_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(marks) = self.workspace.read(cx).map(|w| w.companion_marks(cx)) else {
            return;
        };
        if marks != self.marks {
            self.marks = marks;
            self.refresh(window, cx);
        }
        self.watch_review(window, cx);
    }

    /// Watch the requested document's tab, if one has it.
    fn watch_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = (self.workspace.read(cx).ok()).and_then(|w| w.companion_review_view(cx));
        let id = view.as_ref().map(|view| view.entity_id());
        if self.review_watch.as_ref().map(|(watched, _)| *watched) == id {
            return;
        }
        self.review_watch = view.map(|view| {
            let watch = cx.observe_in(&view, window, |this: &mut Self, _, window, cx| {
                this.marks_changed(window, cx)
            });
            (view.entity_id(), watch)
        });
    }

    fn rows(
        workspace: WindowHandle<Workspace>,
        content: &CompanionContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<ShelfRows>, Subscription) {
        let documents = content.rows.clone();
        let hint = content.drag_hint;
        let rows = cx.new(|cx| ShelfRows::new(workspace, documents, hint, window, cx));
        let events = cx.subscribe_in(&rows, window, |this, _, event, window, cx| match *event {
            // "Copied" shows on the row for a moment; the companion stays.
            ShelfRowsEvent::Copied(_) => {
                this.row_copied = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(COPIED_FOR).await;
                    let _ = this.update_in(cx, |this, window, cx| this.refresh(window, cx));
                }));
            }
            ShelfRowsEvent::Dragged(_) => {}
            ShelfRowsEvent::Open(index) => this.open_row(index, None, cx),
            ShelfRowsEvent::Step(index, step) => this.open_row(index, Some(step), cx),
        });
        (rows, events)
    }

    /// Read the window again: what is shared, its documents, the latest
    /// reply and review request, and the shelf's rows.
    pub(crate) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        ))]
        if self.frozen {
            return;
        }
        let Ok(content) = self.workspace.read(cx).map(|w| w.companion_content(cx)) else {
            return;
        };
        // A drag of a row keeps its rows until it ends.
        if !self.rows.read(cx).dragging() {
            let (rows, events) = Self::rows(self.workspace, &content, window, cx);
            self.rows = rows;
            self._rows_events = events;
        }
        if content.reply.as_ref().map(|r| &r.id) != self.content.reply.as_ref().map(|r| &r.id) {
            self.copied = None;
        }
        // Another card's proposals show only once the pointer moves onto
        // them: a hover over the last card is no hover over these.
        if proposals(&content) != proposals(&self.content) {
            self.proposals_hovered = false;
        }
        self.content = content;
        self.watch_review(window, cx);
        cx.notify();
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

    /// Hide: the window closes; the item or the next delivery opens it.
    pub(crate) fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        menu_bar::companion_closed(cx);
    }

    /// Run `then` in mdoc's window, brought forward; the companion stays.
    fn in_mdoc(
        &self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    ) {
        let workspace = self.workspace;
        cx.defer(move |cx| {
            let _ = workspace.update(cx, |workspace, window, cx| {
                menu_bar::reveal_window(window, cx);
                then(workspace, window, cx);
            });
        });
    }

    /// Run `then` in mdoc's window, which stays where it is.
    fn in_window(
        &self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    ) {
        let workspace = self.workspace;
        cx.defer(move |cx| {
            let _ = workspace.update(cx, then);
        });
    }

    /// A shelf row's click, or its step: in mdoc, as the shelf opens it.
    fn open_row(
        &mut self,
        index: usize,
        step: Option<crate::review::refusal::Step>,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.rows.read(cx).documents().get(index) else {
            return;
        };
        let key: ShelfKey = document.key.clone();
        let ready = document.readiness.is_ready();
        self.in_mdoc(cx, move |workspace, window, cx| {
            workspace.open_from_shelf(&key, ready && step.is_none(), step, window, cx)
        });
    }

    /// Stop sharing: AI apps read nothing until a Project is shared again.
    pub(crate) fn stop_sharing(&mut self, cx: &mut Context<Self>) {
        self.in_window(cx, |workspace, _, cx| workspace.companion_stop_sharing(cx));
    }

    /// The reply's Copy: its restored text, read from the Vault now and
    /// marked as restored, as the Reply page's Copy is.
    pub(crate) fn copy_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reply) = &self.content.reply else {
            return;
        };
        let text = (self.workspace.read(cx).ok()).and_then(|w| w.companion_reply_text(&reply.id));
        let Some(text) = text else {
            return;
        };
        cx.write_to_clipboard(crate::review::restored_clipboard::item(text));
        self.copied = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Save as document: the reply becomes a Document in its place, once.
    pub(crate) fn save_reply(&mut self, cx: &mut Context<Self>) {
        let Some(reply) = (self.content.reply.as_mut()).filter(|r| !r.saved && !r.saving) else {
            return;
        };
        // Saved only once the promotion succeeds; the companion reads it
        // again when it ends.
        reply.saving = true;
        reply.failed = false;
        let id = reply.id.clone();
        self.in_window(cx, move |workspace, window, cx| {
            workspace.companion_save_reply(&id, window, cx)
        });
        cx.notify();
    }

    /// Open in mdoc on the review request: its review in mdoc's window,
    /// brought forward, for whatever the companion does not decide.
    pub(crate) fn open_in_mdoc(&mut self, cx: &mut Context<Self>) {
        let Some(review) = &self.content.review else {
            return;
        };
        let document = review.document.clone();
        self.in_window(cx, move |workspace, window, cx| {
            workspace.companion_review(&document, window, cx)
        });
    }

    /// Replace all on the review request: every proposal it lists, in
    /// mdoc's window, which stays where it is.
    pub(crate) fn replace_all(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.content.review.as_mut() else {
            return;
        };
        let ReviewStep::Proposals(proposals) = &review.step else {
            return;
        };
        let listed = proposals.clone();
        review.step = ReviewStep::Replacing(listed.clone());
        let document = review.document.clone();
        self.in_window(cx, move |workspace, window, cx| {
            workspace.companion_replace_all(&document, listed, window, cx)
        });
        cx.notify();
    }

    /// Files dropped on the drop target. A row's own drag coming back is
    /// no drop.
    pub(crate) fn dropped(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        if self.dropped_on_companion(cx) {
            return;
        }
        let paths = paths.clone();
        self.in_window(cx, move |workspace, window, cx| {
            workspace.companion_drop(&paths, window, cx)
        });
    }

    /// A drop anywhere on the companion: `true` when it was a row's drag.
    fn dropped_on_companion(&mut self, cx: &mut Context<Self>) -> bool {
        self.rows.update(cx, |rows, _| {
            let dragging = rows.dragging();
            if dragging {
                rows.dropped_on_shelf();
            }
            dragging
        })
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

// ---- What it shows ---------------------------------------------------------

impl CompanionView {
    /// Hears the pointer leave the window, and move in it again.
    fn pointer_watch(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity().downgrade();
        gpui::canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let left = view.clone();
                window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        let _ = left.update(cx, |this, cx| this.pointer_left(cx));
                    }
                });
                let moved = view.clone();
                window.on_mouse_event(move |_: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        let _ = moved.update(cx, |this, cx| this.pointer_moved(cx));
                    }
                });
            },
        )
        .absolute()
        .size_0()
        .into_any_element()
    }

    fn heading(&self, text: impl Into<SharedString>) -> gpui::Div {
        let p = self.theme.get().pdf_style();
        div()
            .h(px(HEADING))
            .px(px(4.))
            .flex()
            .items_end()
            .text_size(px(11.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(p.header_muted)
            .child(text.into())
    }

    fn divider(&self) -> AnyElement {
        let p = self.theme.get().pdf_style();
        div()
            .h(px(DIVIDER))
            .flex()
            .items_center()
            .px(px(4.))
            .child(div().w_full().h(px(1.)).bg(p.border))
            .into_any_element()
    }

    /// "SHARED WITH AI APPS" and Hide; then the Project with Stop
    /// sharing, or "No Project shared". Always shown: what is shared is
    /// always named.
    fn shared(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let header = div()
            .h(px(HEADER))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .px(px(4.))
                    .text_size(px(10.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(p.header_muted)
                    .child("SHARED WITH AI APPS"),
            )
            .child(
                ui::header_icon("companion-hide", "Hide", ui::Icon::Close, theme, false)
                    .track_focus(&self.hide_focus)
                    .size(px(22.))
                    .on_click(cx.listener(|this, _, window, cx| this.hide(window, cx))),
            );
        let line = div()
            .id("companion-project")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "companion-project".into())
            })
            .role(gpui::Role::Group)
            .aria_label("Shared with AI apps")
            .h(px(PROJECT_LINE))
            .px(px(4.))
            .flex()
            .items_center()
            .gap_2();
        let line = match &self.content.shared {
            Some(project) => line
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(8.))
                        .rounded_full()
                        .bg(style::project_color(project.hue, theme)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(project.name.clone()),
                )
                .child(
                    ui::link_button("companion-stop-sharing", "Stop sharing", theme, true)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "companion-stop-sharing".into())
                        })
                        .flex_shrink_0()
                        .text_size(px(12.))
                        .on_click(cx.listener(|this, _, _, cx| this.stop_sharing(cx))),
                ),
            None => line.child(div().text_color(p.header_muted).child(
                match self.content.running {
                    true => "No Project shared",
                    false => "No Project shared. Unlock mdoc to share one.",
                },
            )),
        };
        div()
            .flex()
            .flex_col()
            .child(header)
            .child(line)
            .into_any_element()
    }

    /// The latest review request: who asked for which document, then its
    /// proposals with Replace all, that it is Ready, or Open in mdoc for
    /// what only mdoc decides.
    fn review_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let review = self.content.review.as_ref()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let (title, line) = match &review.step {
            ReviewStep::Ready => (
                format!("\u{201c}{}\u{201d} is Ready", review.label),
                format!("{} can read it now.", review.from),
            ),
            ReviewStep::InMdoc(state) => (
                format!("{} asked for \u{201c}{}\u{201d}", review.from, review.label),
                match state.strip_prefix("Needs review · ") {
                    Some(why) => format!("Finish its review in mdoc · {why}"),
                    None => "Finish its review in mdoc.".to_string(),
                },
            ),
            ReviewStep::Proposals(_) | ReviewStep::Replacing(_) => (
                format!("{} asked for \u{201c}{}\u{201d}", review.from, review.label),
                "It needs review before AI apps can read it.".to_string(),
            ),
        };
        let open = |primary: bool, cx: &mut Context<Self>| {
            let id = "companion-review-open";
            let button = match primary {
                true => ui::primary_button(id, "Open in mdoc", theme, true),
                false => ui::action_control(id, "Open in mdoc", theme, true, false),
            };
            button
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "companion-review-open".into())
                })
                .on_click(cx.listener(|this, _, _, cx| this.open_in_mdoc(cx)))
        };
        let buttons = div().flex().justify_end().items_center().gap_2();
        let (proposals, buttons) = match &review.step {
            ReviewStep::Proposals(proposals) => (
                Some(self.proposals(proposals, cx)),
                Some(
                    buttons.child(open(false, cx)).child(
                        ui::primary_button("companion-replace-all", "Replace all", theme, true)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "companion-replace-all".into())
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.replace_all(cx))),
                    ),
                ),
            ),
            ReviewStep::Replacing(proposals) => (
                Some(self.proposals(proposals, cx)),
                Some(buttons.child(open(false, cx)).child(ui::primary_button(
                    "companion-replace-all",
                    "Replacing…",
                    theme,
                    false,
                ))),
            ),
            ReviewStep::InMdoc(_) => (None, Some(buttons.child(open(true, cx)))),
            ReviewStep::Ready => (None, None),
        };
        Some(
            div()
                .id("companion-review")
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "companion-review".into())
                })
                .role(gpui::Role::Group)
                .aria_label(title.clone())
                .h(px(review_height(&review.step)))
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(p.border)
                .bg(p.bg)
                .child(
                    div()
                        .truncate()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.))
                        .text_color(p.header_muted)
                        .child(line),
                )
                .children(proposals)
                .when_some(buttons, |card, buttons| {
                    card.child(div().flex_1()).child(buttons)
                })
                .into_any_element(),
        )
    }

    /// A review's proposals, each wording with the alias Replace all
    /// writes for it: a placeholder until the pointer is over them.
    fn proposals(&self, proposals: &[CompanionProposal], cx: &mut Context<Self>) -> AnyElement {
        let p = self.theme.get().pdf_style();
        let shown = self.shows_proposals();
        let names = match proposals.len() {
            1 => "1 name to replace".to_string(),
            n => format!("{n} names to replace"),
        };
        let list = div()
            .id("companion-proposals")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "companion-proposals".into())
            })
            .role(gpui::Role::List)
            .aria_label(match shown {
                true => "Proposals".to_string(),
                false => format!("{names}, hidden. Point at them to see them."),
            })
            .h(px(proposals_height(proposals)))
            .p(px(PROPOSALS_PADDING))
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .text_size(px(12.))
            .overflow_y_scroll()
            .on_hover(
                cx.listener(|this, hovered: &bool, _, cx| this.set_proposals_hovered(*hovered, cx)),
            );
        let list = match shown {
            // The names are in the window only while shown.
            true => list.children(proposals.iter().enumerate().map(|(i, proposal)| {
                div()
                    .id(("companion-proposal", i))
                    .role(gpui::Role::ListItem)
                    .aria_label(format!(
                        "{} becomes {}, {} {}",
                        proposal.original,
                        proposal.alias,
                        proposal.mentions,
                        if proposal.mentions == 1 {
                            "mention"
                        } else {
                            "mentions"
                        }
                    ))
                    .h(px(PROPOSAL_ROW))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().min_w_0().truncate().child(proposal.original.clone()))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(p.header_muted)
                            .child("\u{2192}"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(p.header_muted)
                            .child(proposal.alias.clone()),
                    )
                    .when(proposal.mentions > 1, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .text_color(p.header_muted)
                                .child(format!("\u{d7}{}", proposal.mentions)),
                        )
                    })
            })),
            false => list.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(p.header_muted)
                    .child(format!("{names}. Point here to see them.")),
            ),
        };
        #[cfg(all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        ))]
        let list = {
            let bounds = self.proposals_bounds.clone();
            list.child(
                gpui::canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
        };
        div().relative().child(list).into_any_element()
    }

    /// The AI project's documents, by AI label, with their state.
    fn documents(&self) -> Option<AnyElement> {
        self.content.shared.as_ref()?;
        let p = self.theme.get().pdf_style();
        let line = |text: String| {
            div()
                .h(px(DOCUMENT_ROW))
                .px(px(4.))
                .flex()
                .items_center()
                .text_size(px(12.))
                .text_color(p.header_muted)
                .child(text)
        };
        let list = div()
            .id("companion-documents")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "companion-documents".into())
            })
            .role(gpui::Role::List)
            .aria_label("Documents AI apps can ask for")
            .flex()
            .flex_col();
        let list = match &self.content.documents {
            Err(error) => list.child(line(format!("The documents can't be listed. {error}"))),
            Ok(documents) if documents.is_empty() => {
                list.child(line("No documents in this Project yet.".into()))
            }
            Ok(documents) => list.children(documents.iter().enumerate().map(|(i, document)| {
                div()
                    .id(("companion-document", i))
                    .h(px(DOCUMENT_ROW))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .role(gpui::Role::ListItem)
                    .aria_label(format!("{}, {}", document.label, document.state))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(document.label.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(if document.ready {
                                p.header_fg
                            } else {
                                p.header_muted
                            })
                            .child(document.state.clone()),
                    )
            })),
        };
        Some(
            div()
                .flex()
                .flex_col()
                .child(self.heading("Documents"))
                .child(list)
                .into_any_element(),
        )
    }

    /// The latest reply: a placeholder until the pointer is over it, then
    /// its restored text; Copy and Save as document either way.
    fn reply(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let reply = self.content.reply.as_ref()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let shown = self.shows_reply();
        let text = div()
            .id("companion-reply")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "companion-reply".into())
            })
            .role(gpui::Role::Group)
            .aria_label(match shown {
                true => "Reply with the names restored",
                false => "Reply hidden. Point at it to read it with the names restored.",
            })
            .h(px(REPLY_TEXT))
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .bg(p.bg)
            .overflow_y_scroll()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.set_reply_shown(*hovered, cx)))
            .child(match shown {
                // The restored text is in the window only while shown.
                true => div()
                    .text_size(px(12.))
                    .child(reply.restored.clone())
                    .into_any_element(),
                false => div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child("Point here to read it with the names restored")
                    .into_any_element(),
            });
        #[cfg(all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        ))]
        let text = {
            let bounds = self.reply_bounds.clone();
            text.child(
                gpui::canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
        };
        let copied = self.copied.is_some();
        let buttons = div()
            .h(px(BUTTONS))
            .flex()
            .items_center()
            .justify_end()
            .gap_2()
            .child(
                ui::action_control(
                    "companion-reply-copy",
                    if copied { "Copied" } else { "Copy" },
                    theme,
                    true,
                    false,
                )
                .on_click(cx.listener(|this, _, window, cx| this.copy_reply(window, cx))),
            )
            .child(
                ui::action_control(
                    "companion-reply-save",
                    match (reply.saved, reply.saving, reply.failed) {
                        (true, _, _) => "Saved as a document",
                        (false, true, _) => "Saving…",
                        (false, false, true) => "Not saved. Try again",
                        (false, false, false) => "Save as document",
                    },
                    theme,
                    !reply.saved && !reply.saving,
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| this.save_reply(cx))),
            );
        Some(
            div()
                .flex()
                .flex_col()
                .child(self.heading(format!("Latest reply · from {}", reply.from)))
                .child(div().relative().child(text))
                .child(buttons)
                .into_any_element(),
        )
    }

    /// Where files dropped are imported: into the AI project, or opened in
    /// mdoc while nothing is shared.
    fn drop_target(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let label = match &self.content.shared {
            Some(project) => format!("Drop files here to add them to {}", project.name),
            None => "Drop files here to open them in mdoc".into(),
        };
        div()
            .id("companion-drop")
            .when(cfg!(test), |v| v.debug_selector(|| "companion-drop".into()))
            .h(px(DROP))
            .px_2()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .text_size(px(12.))
            .text_color(p.header_muted)
            .drag_over::<ExternalPaths>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                cx.stop_propagation();
                this.dropped(paths, cx);
            }))
            .child(div().truncate().child(label))
            .into_any_element()
    }

    /// Open mdoc at the left; Settings and Quit at the right.
    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let row = |id: &'static str, label: &'static str| {
            ui::control(id, label, theme, true)
                .h(px(FOOTER))
                .flex()
                .items_center()
        };
        div()
            .h(px(FOOTER))
            .flex()
            .flex_row()
            .items_center()
            .child(
                row("companion-open-mdoc", "Open mdoc").on_click(cx.listener(|this, _, _, cx| {
                    this.in_mdoc(cx, |workspace, window, cx| {
                        workspace.home_from_shelf(window, cx)
                    })
                })),
            )
            .child(div().flex_1())
            .child(
                ui::header_icon(
                    "companion-settings",
                    "Settings",
                    ui::Icon::Settings,
                    theme,
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.in_mdoc(cx, |workspace, window, cx| {
                        workspace.settings_from_shelf(window, cx)
                    })
                })),
            )
            .child(
                row("companion-quit", "Quit").on_click(cx.listener(|this, _, window, cx| {
                    this.hide(window, cx);
                    // The quit flow asks about unsaved work as ⌘Q does.
                    cx.defer(|cx| cx.dispatch_action(&Quit));
                })),
            )
            .into_any_element()
    }
}

impl Render for CompanionView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        // What is shared stays in view while the rest scrolls.
        let header = div()
            .flex_shrink_0()
            .px(px(PADDING))
            .pt(px(PADDING))
            .child(self.shared(cx));
        let body = div()
            .relative()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(SECTION_GAP))
            .px(px(PADDING))
            .pt(px(SECTION_GAP))
            .pb(px(PADDING))
            .children(self.review_card(cx))
            .children(self.documents())
            .children(self.reply(cx))
            .child(self.drop_target(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(self.divider())
                    .child(self.rows.clone())
                    .child(self.divider())
                    .child(self.footer(cx)),
            )
            .when_some(self.fit.clone(), |body, fit| {
                body.child(
                    gpui::canvas(
                        move |bounds, window, cx| {
                            let height = HEADER_TOTAL + bounds.size.height.as_f32();
                            fit.measured(height, window, cx)
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            });
        div()
            .id("companion")
            .when(cfg!(test), |v| v.debug_selector(|| "companion".into()))
            .key_context("Companion")
            // Tab reaches every control, as in the shelf.
            .on_action(
                cx.listener(|_, _: &ui::NextControl, window, cx| {
                    ui::cycle(window, cx, None, false)
                }),
            )
            .on_action(cx.listener(|_, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, None, true)
            }))
            // A row's drag released over the companion is no drop.
            .on_drop(cx.listener(|this, _: &ExternalPaths, _, cx| {
                this.dropped_on_companion(cx);
            }))
            // Coming back to it, the pointer finds it up to date.
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                if *hovered {
                    this.refresh(window, cx);
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(crate::shelf::shelf_ui::surface(theme))
            .text_color(p.header_fg)
            .text_size(px(13.))
            .child(self.pointer_watch(cx))
            .child(header)
            .child(
                div()
                    .id("companion-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
    }
}
