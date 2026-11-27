//! The sidebar: its state (expanded or collapsed to the rail, collapsed by
//! itself in a narrow window, its dragged width, the rail's reveal of the
//! full tree, OS files dragged over it), generic over the focus handle so its
//! rules run without a window, and its render with the top and foot.
use crate::ui::{self, style, style::Theme};
use crate::workspace::{
    New, RAIL_WIDTH, RESIZE_GRIP, REVEAL_DELAY, REVEAL_GRACE, ToggleSidebar, Workspace,
    activate_for_drop, palette,
};
use gpui::prelude::*;
use gpui::{Animation, AnimationExt, AnyElement, App, Context, Window, div, px};
use mdoc_import::import;

pub(super) mod focus_regions;
pub(super) mod navigation;
pub(super) mod navigation_ui;
pub(super) mod switcher;
pub(super) mod switcher_ui;
mod text;
pub(super) mod tree;
pub(super) mod tree_ui;

use super::home::ShowHome;
use super::palette::OpenPalette;
use super::palette::key;
use crate::workspace::Settings;
use mdoc_store::session_store::ThemeChoice;
use std::time::{Duration, Instant};

/// How long a collapse or expand slides.
pub(super) const SLIDE: Duration = Duration::from_millis(150);
/// How long the rail's hover reveal slides out or back.
pub(super) const REVEAL_SLIDE: Duration = Duration::from_millis(120);

/// The expanded sidebar's width range and default, in points.
pub(super) const MIN_WIDTH: f32 = 160.;
pub(super) const MAX_WIDTH: f32 = 320.;
pub(super) const DEFAULT_WIDTH: f32 = 240.;
/// Below this window width the sidebar collapses by itself.
pub(super) const NARROW_WINDOW: f32 = 900.;

/// The collapsed rail's hover reveal of the full document list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reveal {
    Hidden,
    Shown,
    /// Sliding back; unmounted when the animation ends.
    Hiding,
}

/// What the workspace view carries out for a sidebar transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SidebarEffect {
    /// The sidebar looks different now.
    Repaint,
    /// Repaint once the collapse or expand slide has ended.
    EndSlide,
    /// Open (after hover intent) or close (after the grace period) the
    /// reveal; only the timer with the latest ticket acts (`fire`).
    Schedule { open: bool, ticket: u64 },
    /// Repaint, and unmount the reveal when its slide back ends
    /// (`finish_hiding`).
    FinishHiding(u64),
}

pub(super) struct Sidebar<H> {
    visible: bool,
    /// The user's pick, saved with the session; none: expanded.
    choice: Option<bool>,
    /// The window is narrower than `NARROW_WINDOW`.
    narrow: bool,
    /// Expanded by the user while the window is narrow; the window widening
    /// ends it, so narrowing again collapses it by itself.
    held_open: bool,
    /// The expanded width, within `MIN_WIDTH..=MAX_WIDTH`.
    width: f32,
    /// A drag of the edge: the width it started from, and whether it moved.
    resize: Option<(f32, bool)>,
    /// Bumped by explicit toggles so only they animate the sidebar width.
    generation: u64,
    /// When the current toggle's slide ends.
    slide: Option<Instant>,
    reveal: Reveal,
    reveal_generation: u64,
    /// The reveal's running slide out or back: its generation, and whether
    /// the workspace has scheduled its end yet.
    reveal_slide: Option<(u64, bool)>,
    /// Opened by keyboard focus, so focus inside it also keeps it open.
    keyboard: bool,
    /// Pointer over (rail, reveal).
    hover: (bool, bool),
    /// The timer still allowed to open (`true`) or close (`false`) the reveal.
    pending: Option<(bool, u64)>,
    ticket: u64,
    /// The rail control that keyboard focus came from; Escape returns there.
    opener: Option<H>,
    /// A rail control focused by Escape that must not reveal again.
    escaped: Option<H>,
    /// An OS file drag over the sidebar: the list under the pointer, and
    /// the collapsed rail under the pointer.
    drop_list: bool,
    /// Files over the sidebar, none of which mdoc opens.
    drop_refused: bool,
    drag_rail: bool,
}

impl<H> Default for Sidebar<H> {
    fn default() -> Self {
        Self {
            visible: true,
            choice: None,
            narrow: false,
            held_open: false,
            width: DEFAULT_WIDTH,
            resize: None,
            generation: 0,
            slide: None,
            reveal: Reveal::Hidden,
            reveal_generation: 0,
            reveal_slide: None,
            keyboard: false,
            hover: (false, false),
            pending: None,
            ticket: 0,
            opener: None,
            escaped: None,
            drop_list: false,
            drop_refused: false,
            drag_rail: false,
        }
    }
}

impl<H: Clone + PartialEq> Sidebar<H> {
    pub(super) fn visible(&self) -> bool {
        self.visible
    }

    pub(super) fn choice(&self) -> Option<bool> {
        self.choice
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn reveal(&self) -> Reveal {
        self.reveal
    }

    pub(super) fn reveal_generation(&self) -> u64 {
        self.reveal_generation
    }

    /// The rail control the keyboard reveal was opened from.
    pub(super) fn opener(&self) -> Option<&H> {
        self.opener.as_ref()
    }

    /// The saved pick from the last session.
    pub(super) fn restore(&mut self, visible: bool, choice: Option<bool>) {
        self.visible = visible;
        self.choice = choice;
    }

    /// Set the user's pick; it takes effect at the next `settle`. Expanded,
    /// it holds in a narrow window, as a toggle there does.
    #[cfg(test)]
    pub(super) fn choose(&mut self, expanded: Option<bool>) {
        self.choice = expanded;
        self.held_open = expanded == Some(true);
    }

    /// Each frame, for a window `window_width` wide: expanded unless the
    /// user collapsed it, or the window is narrow and the user has not
    /// expanded it since it narrowed. Collapsing or expanding by itself
    /// does not slide and leaves the user's pick alone. Returns whether it
    /// collapsed now.
    pub(super) fn settle(&mut self, window_width: f32) -> bool {
        self.narrow = window_width < NARROW_WINDOW;
        if !self.narrow {
            self.held_open = false;
        }
        let visible = match self.choice {
            Some(false) => false,
            _ => !self.narrow || self.held_open,
        };
        if visible && !self.visible {
            self.end_reveal();
        }
        let collapsed = self.visible && !visible;
        self.visible = visible;
        collapsed
    }

    /// Expand or collapse at `now`; any reveal ends at once. Expanding in a
    /// narrow window holds it open until the window widens.
    pub(super) fn toggle(&mut self, now: Instant) -> SidebarEffect {
        self.visible = !self.visible;
        self.choice = Some(self.visible);
        self.held_open = self.visible && self.narrow;
        self.generation += 1;
        // Only the sidebar layer animates; one repaint ends it.
        self.slide = Some(now + SLIDE);
        self.end_reveal();
        SidebarEffect::EndSlide
    }

    fn end_reveal(&mut self) {
        self.reveal = Reveal::Hidden;
        self.reveal_slide = None;
        self.pending = None;
        self.hover = (false, false);
        self.keyboard = false;
        self.opener = None;
    }

    /// The expanded width in points.
    pub(super) fn width(&self) -> f32 {
        self.width
    }

    /// The width saved in settings; none or out of range is clamped.
    pub(super) fn restore_width(&mut self, saved: Option<u16>) {
        self.width = saved.map_or(DEFAULT_WIDTH, |width| clamp_width(f32::from(width)));
    }

    /// A drag of the expanded sidebar's edge began.
    pub(super) fn begin_resize(&mut self) {
        self.resize = Some((self.width, false));
    }

    pub(super) fn resizing(&self) -> bool {
        self.resize.is_some()
    }

    /// The edge moved to `width` points from the sidebar's left; whether the
    /// sidebar looks different now.
    pub(super) fn resize_to(&mut self, width: f32) -> bool {
        let Some((_, moved)) = &mut self.resize else {
            return false;
        };
        let width = clamp_width(width);
        if width == self.width {
            return false;
        }
        *moved = true;
        self.width = width;
        true
    }

    /// The drag ended: the whole-point width to save, if it changed.
    pub(super) fn end_resize(&mut self) -> Option<u16> {
        let (start, moved) = self.resize.take()?;
        self.width = self.width.round();
        (moved && self.width != start).then_some(self.width as u16)
    }

    /// Whether a toggle's slide is still running at `now`.
    pub(super) fn sliding(&self, now: Instant) -> bool {
        self.slide.is_some_and(|until| now < until)
    }

    /// Whether a toggle's slide or the reveal's slide is running at `now`:
    /// the content beside the sidebar does not change meanwhile, so the
    /// workspace reuses its last frame. The reveal's slide runs
    /// until `end_reveal_slide`, which the workspace calls with a repaint.
    pub(super) fn animating(&self, now: Instant) -> bool {
        self.sliding(now) || self.reveal_slide.is_some()
    }

    /// The generation of a reveal slide whose end is not scheduled yet; it
    /// counts as scheduled from now on.
    pub(super) fn reveal_slide_to_schedule(&mut self) -> Option<u64> {
        match &mut self.reveal_slide {
            Some((generation, scheduled)) if !*scheduled => {
                *scheduled = true;
                Some(*generation)
            }
            _ => None,
        }
    }

    /// The reveal's slide of `generation` ended: whether that changes what
    /// the workspace draws.
    pub(super) fn end_reveal_slide(&mut self, generation: u64) -> bool {
        if self
            .reveal_slide
            .is_some_and(|(running, _)| running == generation)
        {
            self.reveal_slide = None;
            return true;
        }
        false
    }

    /// The pointer entered or left the rail (`panel` false) or the reveal.
    pub(super) fn hover(&mut self, panel: bool, hovered: bool) -> Option<SidebarEffect> {
        if panel {
            self.hover.1 = hovered;
        } else {
            self.hover.0 = hovered;
        }
        self.update_reveal()
    }

    /// Open or close the reveal for the pointer, or for OS files dragged over
    /// the rail, which GPUI does not report as hover.
    pub(super) fn update_reveal(&mut self) -> Option<SidebarEffect> {
        if self.visible {
            return None;
        }
        let pending = self.pending.map(|(open, _)| open);
        let hovered = self.hover.0 || self.hover.1 || self.drag_rail;
        match (hovered, self.reveal) {
            (true, Reveal::Shown) => self.pending = None,
            (true, Reveal::Hiding) => {
                self.show(false);
                return Some(SidebarEffect::Repaint);
            }
            (true, Reveal::Hidden) if pending != Some(true) => return Some(self.schedule(true)),
            (false, Reveal::Shown) if pending != Some(false) => {
                return Some(self.schedule(false));
            }
            (false, Reveal::Hidden) => self.pending = None,
            _ => {}
        }
        None
    }

    /// Open after hover intent, or close after the grace period. Only the
    /// most recently scheduled timer acts.
    pub(super) fn schedule(&mut self, open: bool) -> SidebarEffect {
        self.ticket += 1;
        self.pending = Some((open, self.ticket));
        SidebarEffect::Schedule {
            open,
            ticket: self.ticket,
        }
    }

    /// A scheduled timer ran out: whether it is still the one to act.
    pub(super) fn fire(&mut self, open: bool, ticket: u64) -> bool {
        if self.pending != Some((open, ticket)) {
            return false;
        }
        self.pending = None;
        true
    }

    /// The pointer, or keyboard focus inside a keyboard reveal, keeps the
    /// reveal open; the view adds menus and drags.
    pub(super) fn holds(&self, focus_inside: impl FnOnce() -> bool) -> bool {
        self.hover.0 || self.hover.1 || (self.keyboard && focus_inside())
    }

    pub(super) fn show(&mut self, keyboard: bool) {
        self.pending = None;
        if self.reveal != Reveal::Shown {
            self.reveal_generation += 1;
            self.reveal = Reveal::Shown;
            self.reveal_slide = Some((self.reveal_generation, false));
            self.keyboard = keyboard;
        }
    }

    /// Slide the reveal back; with reduced motion it unmounts at once.
    /// Nothing to do unless it is shown.
    pub(super) fn dismiss(&mut self, reduce_motion: bool) -> Option<SidebarEffect> {
        self.pending = None;
        if self.reveal != Reveal::Shown {
            return None;
        }
        self.keyboard = false;
        self.opener = None;
        self.reveal_generation += 1;
        if reduce_motion {
            self.reveal = Reveal::Hidden;
            self.reveal_slide = None;
            self.hover.1 = false;
            Some(SidebarEffect::Repaint)
        } else {
            self.reveal = Reveal::Hiding;
            self.reveal_slide = Some((self.reveal_generation, false));
            Some(SidebarEffect::FinishHiding(self.reveal_generation))
        }
    }

    /// The slide back ended: unmount unless the reveal reopened since.
    pub(super) fn finish_hiding(&mut self, generation: u64) -> Option<SidebarEffect> {
        if self.reveal == Reveal::Hiding && self.reveal_generation == generation {
            self.reveal = Reveal::Hidden;
            self.reveal_slide = None;
            self.hover.1 = false;
            Some(SidebarEffect::Repaint)
        } else {
            None
        }
    }

    /// Whether keyboard focus on a rail control may open the reveal: the
    /// sidebar is collapsed, the reveal is not shown, and Escape did not just
    /// return focus to a control that is still focused.
    pub(super) fn keyboard_may_open(&mut self, focused: impl Fn(&H) -> bool) -> bool {
        if self.visible {
            return false;
        }
        if self.escaped.as_ref().is_some_and(|focus| !focused(focus)) {
            self.escaped = None;
        }
        self.reveal != Reveal::Shown && self.escaped.is_none()
    }

    /// Keyboard focus on rail control `opener` reveals the full list.
    pub(super) fn open_for_keyboard(&mut self, opener: H) {
        self.show(true);
        self.keyboard = true;
        self.opener = Some(opener);
    }

    /// Whether focus leaving a keyboard reveal closes it.
    pub(super) fn keyboard_reveal(&self) -> bool {
        !self.visible && self.reveal == Reveal::Shown && self.keyboard
    }

    /// Escape returned focus to rail control `focus`; it must not reveal
    /// again while it keeps focus.
    pub(super) fn escape_to(&mut self, focus: H) {
        self.escaped = Some(focus);
    }

    /// OS files moved over the list, `refused` when mdoc opens none of them.
    pub(super) fn drag_over_list(&mut self, over: bool, refused: bool) -> Option<SidebarEffect> {
        if self.drop_list == over && self.drop_refused == refused {
            return None;
        }
        self.drop_list = over;
        self.drop_refused = refused;
        Some(SidebarEffect::Repaint)
    }

    /// OS files moved onto or off the collapsed rail: they reveal the list
    /// like the pointer.
    pub(super) fn drag_over_rail(&mut self, over: bool) -> Option<SidebarEffect> {
        if self.drag_rail == over {
            return None;
        }
        self.drag_rail = over;
        self.update_reveal()
    }

    pub(super) fn drop_refused(&self) -> bool {
        self.drop_refused
    }

    /// Whether an OS drag left a refusal or rail hover.
    pub(super) fn dragging(&self) -> bool {
        self.drag_rail || self.drop_list || self.drop_refused
    }

    /// The drag ended; one over the rail may close the reveal.
    pub(super) fn clear_drag(&mut self) -> Option<SidebarEffect> {
        self.drop_list = false;
        self.drop_refused = false;
        if std::mem::take(&mut self.drag_rail) {
            self.update_reveal()
        } else {
            None
        }
    }
}

fn clamp_width(width: f32) -> f32 {
    if width.is_finite() {
        width.clamp(MIN_WIDTH, MAX_WIDTH)
    } else {
        DEFAULT_WIDTH
    }
}

/// A drag of the expanded sidebar's edge; the sidebar itself follows it.
#[derive(Clone)]
pub(in crate::workspace) struct SidebarResize;

impl Render for SidebarResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

/// A tab name that fits by losing its middle, never its extension, so
/// names with a shared start still differ. Drawn whole, so the
/// slack a cut leaves falls after the extension rather than before it.
pub(in crate::workspace) fn middle_truncated(name: &str) -> gpui::Div {
    let name = gpui::SharedString::from(name.to_string());
    div()
        .w_full()
        .relative()
        .overflow_hidden()
        // A line's height for the row; the canvas draws the name over it.
        .child("\u{200b}")
        .child(
            gpui::canvas(
                move |bounds, window, _| {
                    let style = window.text_style();
                    let size = style.font_size.to_pixels(window.rem_size());
                    let shape = |text: gpui::SharedString, window: &mut Window| {
                        let run = style.to_run(text.len());
                        window.text_system().shape_line(text, size, &[run], None)
                    };
                    let (stem, extension) = split_extension(&name);
                    let whole = shape(name.clone(), window);
                    if whole.width() <= bounds.size.width {
                        return whole;
                    }
                    let stem_line = shape(stem.to_string().into(), window);
                    let tail = shape(format!("\u{2026}{extension}").into(), window);
                    let room = (bounds.size.width - tail.width()) / px(1.);
                    let (front, back) = middle_cut(
                        stem,
                        |index| stem_line.x_for_index(index) / px(1.),
                        stem_line.width() / px(1.),
                        room,
                    );
                    let cut = format!("{}\u{2026}{}{extension}", &stem[..front], &stem[back..]);
                    shape(cut.into(), window)
                },
                |bounds, line, window, cx| {
                    line.paint(
                        bounds.origin,
                        bounds.size.height,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
                },
            )
            .absolute()
            .inset_0(),
        )
}

/// Where to cut `stem`, whose characters start at `x` and which is `width`
/// wide, to fit `room`: its start gets up to two thirds, its end whatever
/// the start left, at character boundaries. Returns the start's end and the
/// end's start.
pub(in crate::workspace) fn middle_cut(
    stem: &str,
    x: impl Fn(usize) -> f32,
    width: f32,
    room: f32,
) -> (usize, usize) {
    let room = room.max(0.);
    let front = stem
        .char_indices()
        .map(|(index, _)| index)
        .chain([stem.len()])
        .take_while(|&index| x(index) <= room * 2. / 3.)
        .last()
        .unwrap_or(0);
    let left = room - x(front);
    let back = stem[front..]
        .char_indices()
        .map(|(index, _)| front + index)
        .chain([stem.len()])
        .find(|&index| width - x(index) <= left)
        .unwrap_or(stem.len());
    (front, back)
}

/// A name's stem and its extension with the dot: a short final part
/// without spaces after a dot that does not start the name.
pub(in crate::workspace) fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot)
            if dot > 0
                && dot + 1 < name.len()
                && name.len() - dot <= 6
                && !name[dot..].contains(char::is_whitespace) =>
        {
            name.split_at(dot)
        }
        _ => (name, ""),
    }
}

/// A small mark on a sidebar tab for work done in it (OCR, pseudonymization).
pub(in crate::workspace) fn tab_badge(icon: ui::Icon, color: gpui::Hsla) -> gpui::Svg {
    gpui::svg()
        .data(icon.data())
        .size(px(12.))
        .flex_shrink_0()
        .text_color(color)
}

pub(in crate::workspace) fn sidebar_toggle(
    focus: &gpui::FocusHandle,
    expanded: bool,
    theme: Theme,
) -> ui::Control {
    let label = if expanded {
        text::COLLAPSE_SIDEBAR
    } else {
        text::EXPAND_SIDEBAR
    };
    // Quiet, like every other rail and title-bar icon.
    ui::header_icon_untipped("sidebar-toggle", label, ui::Icon::Sidebar, theme, false)
        .tooltip(style::tooltip(
            crate::commands::shortcuts::tip(label, &ToggleSidebar),
            theme,
        ))
        .size(px(32.))
        .track_focus(focus)
        .when(cfg!(test), |v| v.debug_selector(|| "sidebar-toggle".into()))
        .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleSidebar), cx))
}

/// Whether a file drag holds anything mdoc opens (by extension, or a folder).
pub(in crate::workspace) fn openable_drag(paths: &gpui::ExternalPaths) -> bool {
    paths.paths().iter().any(|path| import::droppable(path))
}

/// The sidebar takes only drags it can open; others fall through untouched.
pub(in crate::workspace) fn can_drop_files(
    drag: &dyn std::any::Any,
    _: &mut Window,
    _: &mut App,
) -> bool {
    drag.downcast_ref::<gpui::ExternalPaths>()
        .is_none_or(openable_drag)
}

impl Workspace {
    /// Track the pointer over the collapsed rail or its reveal.
    pub(in crate::workspace) fn hover_sidebar(
        &mut self,
        panel: bool,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effect = self.sidebar.hover(panel, hovered);
        self.carry_out_sidebar(effect, window, cx);
    }

    pub(in crate::workspace) fn carry_out_sidebar(
        &mut self,
        effect: Option<SidebarEffect>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            None => {}
            Some(SidebarEffect::Repaint) => cx.notify(),
            Some(SidebarEffect::EndSlide) => {
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(SLIDE + Duration::from_millis(16))
                        .await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                })
                .detach();
            }
            Some(SidebarEffect::Schedule { open, ticket }) => {
                let delay = if open { REVEAL_DELAY } else { REVEAL_GRACE };
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        if !this.sidebar.fire(open, ticket) {
                            return;
                        }
                        if open {
                            this.show_reveal(false, cx);
                        } else if this.reveal_held(window, cx) {
                            let effect = this.sidebar.schedule(false);
                            this.carry_out_sidebar(Some(effect), window, cx);
                        } else {
                            let focused = this.reveal_focus.contains_focused(window, cx);
                            this.dismiss_reveal(window, cx);
                            if focused {
                                this.focus_active(window, cx);
                            }
                        }
                    });
                })
                .detach();
            }
            Some(SidebarEffect::FinishHiding(generation)) => {
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(REVEAL_SLIDE).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        let effect = this.sidebar.finish_hiding(generation);
                        this.carry_out_sidebar(effect, window, cx);
                    });
                })
                .detach();
                cx.notify();
            }
        }
    }

    /// A tab's or a Project's context menu is open. The sidebar then shows no
    /// tooltips (#310): GPUI keeps a shown tooltip while the pointer rests
    /// on it, draws it over the menu, and shows a new one when the pointer
    /// slips off the menu onto a row. Rendering a control without its
    /// tooltip is GPUI's one way to dismiss it.
    pub(crate) fn context_menu_open(&self) -> bool {
        self.document_menu.is_some() || self.project_ui.header_menu.is_some()
    }

    /// A context menu, drag, the pointer, or keyboard focus keeps the reveal open.
    pub(in crate::workspace) fn reveal_held(&self, window: &Window, cx: &App) -> bool {
        self.document_menu.is_some()
            || cx.has_active_drag()
            || self
                .sidebar
                .holds(|| self.reveal_focus.contains_focused(window, cx))
    }

    pub(in crate::workspace) fn show_reveal(&mut self, keyboard: bool, cx: &mut Context<Self>) {
        self.sidebar.show(keyboard);
        cx.notify();
    }

    /// A reveal slide that started ends after `REVEAL_SLIDE` with a repaint,
    /// so the document beside it is drawn afresh, uncached.
    pub(in crate::workspace) fn end_reveal_slide_later(&mut self, cx: &mut Context<Self>) {
        let Some(generation) = self.sidebar.reveal_slide_to_schedule() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REVEAL_SLIDE).await;
            let _ = this.update(cx, |this, cx| {
                if this.sidebar.end_reveal_slide(generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Slide the reveal back; it unmounts once the animation has finished.
    pub(in crate::workspace) fn dismiss_reveal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effect = self.sidebar.dismiss(cx.reduce_motion());
        self.carry_out_sidebar(effect, window, cx);
    }

    /// Keyboard focus on a rail control reveals the full list at once and moves
    /// focus to the matching control there; focus leaving the reveal closes it.
    pub(in crate::workspace) fn reveal_for_keyboard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar.visible() {
            return;
        }
        let may_open = self
            .sidebar
            .keyboard_may_open(|focus| focus.is_focused(window));
        let focused = [
            (&self.rail_toggle_focus, &self.panel_toggle_focus),
            (&self.rail_new_focus, &self.panel_new_focus),
            (&self.rail_home_focus, &self.panel_home_focus),
        ]
        .into_iter()
        .find(|(rail, _)| rail.is_focused(window))
        .map(|(rail, panel)| (rail.clone(), panel.clone()));
        match focused {
            Some((rail, panel)) if may_open && window.last_input_was_keyboard() => {
                self.sidebar.open_for_keyboard(rail);
                cx.notify();
                window.defer(cx, move |window, cx| window.focus(&panel, cx));
            }
            None if self.sidebar.keyboard_reveal() && !self.reveal_held(window, cx) => {
                self.dismiss_reveal(window, cx);
            }
            _ => {}
        }
    }

    pub(in crate::workspace) fn clear_file_drag(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effect = self.sidebar.clear_drag();
        self.carry_out_sidebar(effect, window, cx);
        cx.notify();
    }

    /// Import OS files dropped on the tree or the rail into Unfiled.
    pub(in crate::workspace) fn drop_files(
        &mut self,
        paths: &gpui::ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_file_drag(window, cx);
        // A drop from Finder leaves mdoc in the background; what it opens
        // (a dialog among them) needs the keyboard.
        activate_for_drop(window, cx);
        if !self.quit_flow.quitting() {
            self.open_paths(paths.paths().to_vec(), window, cx);
        }
    }

    /// OS files dropped on a Project header: they are imported into that
    /// Project, and a folder among them offers Add to it.
    pub(in crate::workspace) fn drop_on_project(
        &mut self,
        paths: &gpui::ExternalPaths,
        project: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_file_drag(window, cx);
        activate_for_drop(window, cx);
        if !self.quit_flow.quitting() {
            let paths = paths.paths().to_vec();
            self.open_paths_into(paths, Some(project.to_string()), window, cx);
        }
    }

    pub(in crate::workspace) fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let expanded = self.sidebar.visible();
        let revealed = !expanded && self.sidebar.reveal() != Reveal::Hidden;
        let panel_width = self.sidebar.width();
        let menu_open = self.context_menu_open();
        let rows = self.tree_view(cx);
        let sliding = self.sidebar.sliding(std::time::Instant::now()) && !cx.reduce_motion();
        let collapsing = sliding && !expanded && !revealed;
        let mut panel = (expanded || revealed || collapsing).then(|| {
            div()
                .id("sidebar-panel")
                .w(px(panel_width - 1.))
                .h_full()
                .flex()
                .flex_col()
                .on_drag_move(cx.listener(
                    |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, window, cx| {
                        let over = event.bounds.contains(&event.event.position);
                        let refused = over && !openable_drag(event.drag(cx));
                        let effect = this.sidebar.drag_over_list(over, refused);
                        this.carry_out_sidebar(effect, window, cx);
                    },
                ))
                .can_drop(can_drop_files)
                .on_drop(
                    cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                        this.drop_files(paths, window, cx)
                    }),
                )
                .child(
                    self.sidebar_top(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(sidebar_toggle(&self.panel_toggle_focus, expanded, theme))
                            .when(expanded || revealed, |v| {
                                v.child(self.navigation_buttons(cx))
                            })
                            .into_any_element(),
                    ),
                )
                .child(self.sidebar_nav(cx))
                // Above the scrolling list, so it shows however long it is.
                .when(self.sidebar.drop_refused(), |v| {
                    v.child(
                        div()
                            .when(cfg!(test), |v| v.debug_selector(|| "drop-refused".into()))
                            .px_4()
                            .py_1()
                            .text_color(style::markdown_style(theme).alert_caution)
                            .child(text::FILES_CANNOT_OPEN),
                    )
                })
                .child(
                    div()
                        .id("sidebar-tree")
                        .role(gpui::Role::Tree)
                        .aria_label(text::PROJECTS_AND_DOCUMENTS)
                        .track_scroll(&self.sidebar_scroll)
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px_2()
                        .pb_1()
                        .children(rows),
                )
                .child(self.sidebar_foot(cx))
        });
        let content = if expanded {
            // Expanding: the list is revealed inside its final width, so the
            // content beside it is laid out once, not on every frame.
            let list = div().size_full().children(panel.take());
            if sliding {
                div()
                    .size_full()
                    .child(
                        div()
                            .h_full()
                            .overflow_hidden()
                            .child(div().w(px(panel_width)).h_full().child(list))
                            .with_animation(
                                ("sidebar-open", self.sidebar.generation()),
                                Animation::new(SLIDE).with_easing(gpui::ease_out_quint()),
                                move |v, delta| {
                                    v.w(px(RAIL_WIDTH + (panel_width - RAIL_WIDTH) * delta))
                                },
                            ),
                    )
                    .into_any_element()
            } else {
                list.into_any_element()
            }
        } else {
            div()
                .w(px(RAIL_WIDTH - 1.))
                .h_full()
                .flex()
                .flex_col()
                // The rail starts below the title row, which holds the traffic
                // lights, the expand toggle and the document header.
                .child(ui::title_drag(
                    div()
                        .flex_shrink_0()
                        .h(px(style::CARD_MARGIN + style::HEADER_HEIGHT)),
                    self.title.armed.clone(),
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(2.))
                        .pb_2()
                        .flex_shrink_0()
                        .child(
                            ui::header_icon(
                                "rail-search",
                                text::SEARCH_OR_RUN,
                                ui::Icon::Search,
                                theme,
                                false,
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(palette::OpenPalette), cx)
                            }),
                        )
                        .child(
                            ui::header_icon(
                                "rail-home",
                                "Home",
                                ui::Icon::Home,
                                theme,
                                self.showing_home(cx),
                            )
                            .track_focus(&self.rail_home_focus.clone().tab_stop(!revealed))
                            .when(cfg!(test), |v| v.debug_selector(|| "rail-home".into()))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.show_home(window, cx)),
                            ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .flex_1()
                        .min_h_0()
                        .child(
                            ui::header_icon_untipped(
                                "sidebar-new-collapsed",
                                text::NEW_DOCUMENT,
                                ui::Icon::Plus,
                                theme,
                                false,
                            )
                            .when(!menu_open, |v| {
                                v.tooltip(style::tooltip(
                                    crate::commands::shortcuts::tip(text::NEW_DOCUMENT, &New),
                                    theme,
                                ))
                            })
                            .track_focus(&self.rail_new_focus.clone().tab_stop(!revealed))
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "sidebar-new-collapsed".into())
                            })
                            .size(px(32.))
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(New), cx)),
                        ),
                )
                .child(self.rail_foot(cx))
                .into_any_element()
        };
        let width = |open: bool| if open { panel_width } else { RAIL_WIDTH };
        let mut sidebar = div()
            .id("sidebar")
            .track_focus(self.focus_regions.sidebar())
            .key_context("SidebarRegion")
            .relative()
            .w(px(width(expanded)))
            .flex_shrink_0()
            .h_full()
            .text_size(px(13.))
            // Keep the collapsed title controls in the sidebar's focus region
            // and before its list in Tab order. Deferred painting leaves them
            // above the adjacent content card without clipping to the rail.
            .when(!expanded && !revealed, |v| {
                v.child(gpui::deferred(
                    div()
                        .id("collapsed-title-controls")
                        .tab_index(-1)
                        .tab_group()
                        .tab_stop(false)
                        .absolute()
                        .left(px(style::TRAFFIC_LIGHT_INSET))
                        .top(px(style::CARD_MARGIN + (style::HEADER_HEIGHT - 32.) / 2.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(sidebar_toggle(&self.rail_toggle_focus, false, theme))
                        .child(self.navigation_buttons(cx)),
                ))
            })
            .child(div().size_full().overflow_hidden().child(content))
            // The expanded sidebar's edge drags its width; the
            // grip straddles the gap beside the content card.
            .when(expanded && !sliding, |v| {
                v.child(
                    div()
                        .id("sidebar-resize")
                        .when(cfg!(test), |v| v.debug_selector(|| "sidebar-resize".into()))
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right(px(-RESIZE_GRIP))
                        .w(px(2. * RESIZE_GRIP))
                        .cursor_col_resize()
                        .on_drag(SidebarResize, |drag, _, _, cx| {
                            cx.stop_propagation();
                            cx.new(|_| drag.clone())
                        }),
                )
                .on_drag_move(cx.listener(
                    |this, event: &gpui::DragMoveEvent<SidebarResize>, _, cx| {
                        if !this.sidebar.resizing() {
                            this.sidebar.begin_resize();
                        }
                        let width = (event.event.position.x - event.bounds.left()) / px(1.);
                        if this.sidebar.resize_to(width) {
                            cx.notify();
                        }
                    },
                ))
            })
            .when(!expanded, |v| {
                v.on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                    this.hover_sidebar(false, *hovered, window, cx)
                }))
                .on_drag_move(cx.listener(
                    |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, window, cx| {
                        let over = event.bounds.contains(&event.event.position);
                        let effect = this.sidebar.drag_over_rail(over);
                        this.carry_out_sidebar(effect, window, cx);
                    },
                ))
                .can_drop(can_drop_files)
                .on_drop(cx.listener(
                    |this, paths: &gpui::ExternalPaths, window, cx| {
                        this.drop_files(paths, window, cx)
                    },
                ))
            });
        if revealed && let Some(panel) = panel.take() {
            let (from, to) = if self.sidebar.reveal() == Reveal::Shown {
                (RAIL_WIDTH, panel_width)
            } else {
                (panel_width, RAIL_WIDTH)
            };
            sidebar = sidebar.child(gpui::deferred(
                div()
                    .id("sidebar-reveal")
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .w(px(to))
                    .when(cfg!(test), |v| v.debug_selector(|| "sidebar-reveal".into()))
                    .key_context("UiPanel UiMenu")
                    .track_focus(&self.reveal_focus)
                    // Ahead of the editor in tab order, as the rail it covers.
                    .tab_index(-1)
                    .tab_group()
                    .tab_stop(false)
                    .occlude()
                    .overflow_hidden()
                    .bg(theme.sidebar_bg())
                    .rounded_r_lg()
                    .shadow_lg()
                    .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                        this.hover_sidebar(true, *hovered, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                        match this.sidebar.opener().cloned() {
                            Some(focus) => {
                                window.focus(&focus, cx);
                                this.sidebar.escape_to(focus);
                            }
                            None => this.focus_active(window, cx),
                        }
                        this.dismiss_reveal(window, cx);
                    }))
                    .child(panel)
                    .with_animation(
                        ("sidebar-reveal", self.sidebar.reveal_generation()),
                        Animation::new(REVEAL_SLIDE).with_easing(gpui::ease_out_quint()),
                        move |v, delta| v.w(px(from + (to - from) * delta)),
                    ),
            ));
        }
        // Collapsing: the content already has its final width; the full list
        // narrows over it to the rail, drawn above the card.
        if collapsing && let Some(list) = panel.take() {
            sidebar = sidebar.child(gpui::deferred(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .overflow_hidden()
                    .bg(theme.sidebar_bg())
                    .child(div().w(px(panel_width)).h_full().child(list))
                    .with_animation(
                        ("sidebar-close", self.sidebar.generation()),
                        Animation::new(SLIDE).with_easing(gpui::ease_out_quint()),
                        move |v, delta| v.w(px(panel_width - (panel_width - RAIL_WIDTH) * delta)),
                    ),
            ));
        }
        sidebar.into_any_element()
    }

    /// The sidebar's edge drag ended: keep the width it left in settings.
    pub(in crate::workspace) fn end_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if let Some(width) = self.sidebar.end_resize() {
            // Kept for this window when the save fails; the error is logged.
            let _ = self.update_general(move |general| general.sidebar_width = Some(width), cx);
        }
        cx.notify();
    }
}

impl Workspace {
    // ---- Sidebar ------------------------------------------------------------

    /// The sidebar's first row: room for the traffic lights, then the toggle.
    /// It moves the window like a title bar.
    pub(super) fn sidebar_top(&self, toggle: AnyElement) -> AnyElement {
        ui::title_drag(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(style::CARD_MARGIN + style::HEADER_HEIGHT))
                .pt(px(style::CARD_MARGIN))
                .pl(px(style::TRAFFIC_LIGHT_INSET))
                .pr_2(),
            self.title.armed.clone(),
        )
        .child(ui::no_drag(div().child(toggle)))
        .into_any_element()
    }
    /// Search ⌘K and Home under the top row.
    pub(super) fn sidebar_nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let home = self.showing_home(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px_2()
            .pb_2()
            .flex_shrink_0()
            .child(
                crate::ui::focus_ring("sidebar-search", theme)
                    .when(cfg!(test), |v| v.debug_selector(|| "sidebar-search".into()))
                    .role(gpui::Role::Button)
                    .aria_label(text::SEARCH_OR_RUN)
                    .key_context("UiControl")
                    .tab_index(0)
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .px_2()
                    .mb_1()
                    .rounded_md()
                    .border_1()
                    .border_color(p.border)
                    .bg(p.bg)
                    .text_color(p.header_muted)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(p.header_muted))
                    .child(
                        gpui::svg()
                            .data(ui::Icon::Search.data())
                            .size(px(14.))
                            .text_color(p.header_muted),
                    )
                    .child(div().flex_1().child(text::SEARCH))
                    .child(div().text_size(px(11.)).child(key(&OpenPalette)))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenPalette), cx)),
            )
            .child(
                nav_row("sidebar-home", ui::Icon::Home, "Home", theme, home)
                    .track_focus(&self.panel_home_focus)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .child(key(&ShowHome)),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.show_home(window, cx))),
            )
            .into_any_element()
    }
    /// A small section label, e.g. "Open" over tabs without a Project.
    pub(super) fn section_label(&self, label: &'static str) -> AnyElement {
        let p = self.theme.get().pdf_style();
        div()
            .px_2()
            .pt_3()
            .pb_1()
            .text_size(px(11.))
            .text_color(p.header_muted)
            .child(label)
            .into_any_element()
    }
    /// The update card and the Settings · theme · Help row.
    pub(super) fn sidebar_foot(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let theme_icon = match self.theme_choice {
            ThemeChoice::System => ui::Icon::ThemeSystem,
            ThemeChoice::Light => ui::Icon::Sun,
            ThemeChoice::Dark => ui::Icon::Moon,
        };
        let card = self.update_card(cx);
        // The card carries its own dot; Settings marks an update only without it.
        let dot = card.is_none() && crate::app::updater::state(cx).marks_settings();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .pb_2()
            .pt_1()
            .flex_shrink_0()
            .children(self.ai_indicator(cx))
            .children(card)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        nav_row("Settings", ui::Icon::Settings, "Settings", theme, false)
                            .track_focus(&self.settings_focus)
                            .flex_1()
                            .when(dot, |v| {
                                v.child(
                                    div()
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "update-dot".into())
                                        })
                                        .size(px(7.))
                                        .rounded_full()
                                        .bg(style::markdown_style(theme).alert_warning),
                                )
                            })
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(Settings), cx)
                            }),
                    )
                    .child(
                        ui::header_icon_untipped(
                            "theme-toggle",
                            style::theme_label(self.theme_choice),
                            theme_icon,
                            theme,
                            false,
                        )
                        // Clicking changes the choice under the pointer; the
                        // tooltip follows it instead of the value at hover.
                        .tooltip({
                            self.theme_tip.set(self.theme_choice);
                            let (choice, theme) = (self.theme_tip.clone(), self.theme.clone());
                            style::live_tooltip(move || {
                                (style::theme_label(choice.get()).into(), theme.get())
                            })
                        })
                        .when(cfg!(test), |v| v.debug_selector(|| "theme-toggle".into()))
                        .on_click(cx.listener(|this, _, window, cx| this.cycle_theme(window, cx))),
                    )
                    .child(
                        ui::header_icon("sidebar-help", "Help ↗", ui::Icon::Help, theme, false)
                            .on_click(|_, _, cx| cx.open_url(crate::commands::menus::HELP_URL)),
                    ),
            )
            .child(
                div()
                    .h(px(1.))
                    .bg(gpui::transparent_black())
                    .text_color(p.header_muted),
            )
            .into_any_element()
    }
    /// The collapsed rail's foot: Settings only, an update shown as a dot on
    /// it. Theme and Help live in the open sidebar.
    pub(super) fn rail_foot(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let update = crate::app::updater::state(cx).marks_settings();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .pt_1()
            .pb_3()
            .flex_shrink_0()
            .children(self.ai_rail_icon(cx))
            .child(
                div()
                    .relative()
                    .child(
                        ui::header_icon(
                            "rail-settings",
                            if update {
                                text::SETTINGS_UPDATE_AVAILABLE
                            } else {
                                "Settings"
                            },
                            ui::Icon::Settings,
                            theme,
                            false,
                        )
                        .when(cfg!(test), |v| v.debug_selector(|| "rail-settings".into()))
                        .size(px(32.))
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(Settings), cx)),
                    )
                    .when(update, |v| {
                        v.child(
                            div()
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "rail-update-dot".into())
                                })
                                .absolute()
                                .top(px(6.))
                                .right(px(6.))
                                .size(px(6.))
                                .rounded_full()
                                .bg(style::markdown_style(theme).alert_warning),
                        )
                    }),
            )
            .into_any_element()
    }
}

fn nav_row(
    id: &'static str,
    icon: ui::Icon,
    label: &'static str,
    theme: Theme,
    selected: bool,
) -> ui::Control {
    let p = theme.pdf_style();
    crate::ui::focus_ring(id, theme)
        .when(cfg!(test), |v| v.debug_selector(move || id.into()))
        .role(gpui::Role::Button)
        .aria_label(label)
        .key_context("UiControl")
        .tab_index(0)
        .flex()
        .items_center()
        .gap_2()
        .h(px(30.))
        .px_2()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |v| v.bg(theme.sidebar_selected()))
        .hover(move |s| s.bg(p.placeholder_bg))
        .child(
            gpui::svg()
                .data(icon.data())
                .size(px(15.))
                .flex_shrink_0()
                .text_color(if selected {
                    p.header_fg
                } else {
                    p.header_muted
                }),
        )
        .child(div().flex_1().child(label))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: f32 = 1000.;
    const NARROW: f32 = 720.;

    fn collapsed() -> Sidebar<u8> {
        let mut sidebar = Sidebar::default();
        sidebar.restore(false, Some(false));
        sidebar
    }

    #[test]
    fn the_reveal_slides_until_its_scheduled_end_and_not_with_reduced_motion() {
        let mut sidebar = collapsed();
        let now = Instant::now();
        assert!(!sidebar.animating(now));
        sidebar.show(false);
        assert!(sidebar.animating(now));
        let shown = sidebar.reveal_slide_to_schedule().expect("a slide to end");
        assert_eq!(sidebar.reveal_slide_to_schedule(), None, "scheduled once");
        assert!(sidebar.end_reveal_slide(shown));
        assert!(!sidebar.animating(now));
        assert!(!sidebar.end_reveal_slide(shown), "ended once");
        // Sliding back: a stale end from the slide out changes nothing.
        assert!(sidebar.dismiss(false).is_some());
        assert!(!sidebar.end_reveal_slide(shown));
        assert!(sidebar.animating(now));
        let hiding = sidebar.reveal_slide_to_schedule().expect("a slide to end");
        assert!(sidebar.finish_hiding(hiding).is_some());
        assert!(!sidebar.animating(now), "unmounting ends the slide");
        // Reduced motion unmounts at once: nothing slides.
        sidebar.show(false);
        assert_eq!(sidebar.dismiss(true), Some(SidebarEffect::Repaint));
        assert!(!sidebar.animating(now));
    }

    #[test]
    fn a_toggle_pins_the_pick_slides_once_and_ends_any_reveal() {
        let mut sidebar = collapsed();
        sidebar.show(false);
        let now = Instant::now();
        assert_eq!(sidebar.toggle(now), SidebarEffect::EndSlide);
        assert!(sidebar.visible());
        assert_eq!(sidebar.choice(), Some(true));
        assert_eq!(sidebar.generation(), 1);
        assert_eq!(sidebar.reveal(), Reveal::Hidden);
        assert!(sidebar.sliding(now));
        assert!(!sidebar.sliding(now + SLIDE));
        // Expanded, the pointer never reveals.
        assert_eq!(sidebar.hover(false, true), None);
        sidebar.choose(None);
        sidebar.settle(WIDE);
        assert!(sidebar.visible());
    }

    #[test]
    fn a_narrow_window_collapses_it_and_widening_brings_it_back() {
        let mut sidebar = Sidebar::<u8>::default();
        sidebar.settle(WIDE);
        assert!(sidebar.visible());
        assert!(sidebar.settle(NARROW_WINDOW - 1.));
        assert!(!sidebar.visible());
        assert!(!sidebar.settle(NARROW));
        // Collapsed by itself: no slide, and the user's pick is untouched.
        assert_eq!(sidebar.generation(), 0);
        assert_eq!(sidebar.choice(), None);
        sidebar.settle(NARROW_WINDOW);
        assert!(sidebar.visible());
        // A reveal open while narrow ends when the window widens.
        sidebar.settle(NARROW);
        sidebar.show(false);
        sidebar.settle(WIDE);
        assert!(sidebar.visible());
        assert_eq!(sidebar.reveal(), Reveal::Hidden);
        // An explicit expand earlier in the session does not stop it.
        let now = Instant::now();
        sidebar.toggle(now);
        sidebar.toggle(now);
        assert_eq!(sidebar.choice(), Some(true));
        sidebar.settle(NARROW);
        assert!(!sidebar.visible());
        sidebar.settle(WIDE);
        assert!(sidebar.visible());
    }

    #[test]
    fn collapsed_by_the_user_it_stays_collapsed_when_the_window_widens() {
        let mut sidebar = Sidebar::<u8>::default();
        sidebar.settle(WIDE);
        sidebar.toggle(Instant::now());
        sidebar.settle(NARROW);
        sidebar.settle(WIDE);
        assert!(!sidebar.visible());
        assert_eq!(sidebar.choice(), Some(false));
        // Collapsed by the user while it was already narrow: the same.
        let mut sidebar = Sidebar::<u8>::default();
        sidebar.settle(NARROW);
        sidebar.toggle(Instant::now());
        sidebar.settle(NARROW);
        assert!(sidebar.visible());
        sidebar.toggle(Instant::now());
        sidebar.settle(WIDE);
        assert!(!sidebar.visible());
    }

    #[test]
    fn expanded_in_a_narrow_window_it_stays_until_the_window_widens() {
        let mut sidebar = Sidebar::<u8>::default();
        sidebar.settle(NARROW);
        assert!(!sidebar.visible());
        assert_eq!(sidebar.toggle(Instant::now()), SidebarEffect::EndSlide);
        sidebar.settle(NARROW);
        sidebar.settle(NARROW - 100.);
        assert!(sidebar.visible());
        // Widening ends the hold; narrowing again collapses it by itself.
        sidebar.settle(WIDE);
        assert!(sidebar.visible());
        sidebar.settle(NARROW);
        assert!(!sidebar.visible());
    }

    #[test]
    fn dragging_the_edge_resizes_within_its_range_and_saves_once() {
        let mut sidebar = Sidebar::<u8>::default();
        assert_eq!(sidebar.width(), DEFAULT_WIDTH);
        // Without a drag, a move does nothing.
        assert!(!sidebar.resize_to(300.));
        assert_eq!(sidebar.end_resize(), None);
        sidebar.begin_resize();
        assert!(sidebar.resizing());
        assert!(sidebar.resize_to(280.4));
        assert!(!sidebar.resize_to(280.4));
        assert!(sidebar.resize_to(1000.));
        assert_eq!(sidebar.width(), MAX_WIDTH);
        assert!(sidebar.resize_to(10.));
        assert_eq!(sidebar.width(), MIN_WIDTH);
        assert!(sidebar.resize_to(280.4));
        assert_eq!(sidebar.end_resize(), Some(280));
        assert!(!sidebar.resizing());
        assert_eq!(sidebar.width(), 280.);
        // A drag that ends where it began saves nothing.
        sidebar.begin_resize();
        assert!(sidebar.resize_to(200.));
        assert!(sidebar.resize_to(280.));
        assert_eq!(sidebar.end_resize(), None);
        sidebar.begin_resize();
        assert_eq!(sidebar.end_resize(), None);
    }

    #[test]
    fn a_saved_width_is_clamped_to_the_range() {
        let mut sidebar = Sidebar::<u8>::default();
        sidebar.restore_width(Some(300));
        assert_eq!(sidebar.width(), 300.);
        sidebar.restore_width(Some(40));
        assert_eq!(sidebar.width(), MIN_WIDTH);
        sidebar.restore_width(Some(u16::MAX));
        assert_eq!(sidebar.width(), MAX_WIDTH);
        sidebar.restore_width(None);
        assert_eq!(sidebar.width(), DEFAULT_WIDTH);
    }

    #[test]
    fn hover_opens_after_intent_and_only_the_latest_timer_acts() {
        let mut sidebar = collapsed();
        let Some(SidebarEffect::Schedule { open: true, ticket }) = sidebar.hover(false, true)
        else {
            panic!("hover schedules an open");
        };
        // Leaving and coming back before the timer runs schedules again.
        assert_eq!(sidebar.hover(false, false), None);
        let Some(SidebarEffect::Schedule {
            open: true,
            ticket: later,
        }) = sidebar.hover(false, true)
        else {
            panic!("hover schedules an open");
        };
        assert!(!sidebar.fire(true, ticket));
        assert!(sidebar.fire(true, later));
        sidebar.show(false);
        assert_eq!(sidebar.reveal(), Reveal::Shown);
        // The pointer moves from the rail onto the reveal: it stays.
        assert_eq!(sidebar.hover(true, true), None);
        assert_eq!(sidebar.hover(false, false), None);
        assert!(sidebar.holds(|| false));
        let Some(SidebarEffect::Schedule {
            open: false,
            ticket,
        }) = sidebar.hover(true, false)
        else {
            panic!("leaving schedules a close");
        };
        assert!(!sidebar.holds(|| true));
        assert!(sidebar.fire(false, ticket));
        let generation = sidebar.reveal_generation();
        assert_eq!(
            sidebar.dismiss(false),
            Some(SidebarEffect::FinishHiding(generation + 1))
        );
        assert_eq!(sidebar.reveal(), Reveal::Hiding);
        // Hovering again while it slides back shows it at once.
        assert_eq!(sidebar.hover(false, true), Some(SidebarEffect::Repaint));
        assert_eq!(sidebar.reveal(), Reveal::Shown);
        assert_eq!(sidebar.finish_hiding(generation + 1), None);
        assert!(sidebar.hover(false, false).is_some());
        assert_eq!(sidebar.dismiss(true), Some(SidebarEffect::Repaint));
        assert_eq!(sidebar.reveal(), Reveal::Hidden);
        assert_eq!(sidebar.dismiss(true), None);
    }

    #[test]
    fn a_keyboard_reveal_returns_to_its_opener_and_escape_holds_it_shut() {
        let mut sidebar = collapsed();
        assert!(sidebar.keyboard_may_open(|_| false));
        sidebar.open_for_keyboard(7);
        assert_eq!(sidebar.opener(), Some(&7));
        assert!(sidebar.keyboard_reveal());
        assert!(sidebar.holds(|| true));
        assert!(!sidebar.holds(|| false));
        assert!(!sidebar.keyboard_may_open(|_| true));
        // Escape: focus goes back to the opener, which must not reveal again
        // until focus has left it.
        sidebar.escape_to(7);
        assert_eq!(sidebar.dismiss(true), Some(SidebarEffect::Repaint));
        assert_eq!(sidebar.opener(), None);
        assert!(!sidebar.keyboard_may_open(|focus| *focus == 7));
        assert!(sidebar.keyboard_may_open(|_| false));
    }

    #[test]
    fn files_dragged_over_the_list_are_refused_when_none_opens() {
        let mut sidebar = collapsed();
        let repaint = Some(SidebarEffect::Repaint);
        assert_eq!(sidebar.drag_over_list(true, false), repaint);
        assert_eq!(sidebar.drag_over_list(true, false), None);
        assert!(!sidebar.drop_refused());
        assert_eq!(sidebar.drag_over_list(true, true), repaint);
        assert!(sidebar.drop_refused());
        // Over the rail, the drag reveals the list like hover.
        assert!(matches!(
            sidebar.drag_over_rail(true),
            Some(SidebarEffect::Schedule { open: true, .. })
        ));
        assert_eq!(sidebar.drag_over_rail(true), None);
        assert!(sidebar.dragging());
        // Ending it cancels the pending open.
        assert_eq!(sidebar.clear_drag(), None);
        assert!(!sidebar.dragging());
        assert_eq!(sidebar.clear_drag(), None);
    }
}
