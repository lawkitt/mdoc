//! Carry out destination history without reopening closed tabs or reading case keys.
use super::{
    Workspace,
    case_page::Place,
    navigation::{Arrival, Stop},
};
use crate::{style, ui};
use gpui::{
    AnyElement, App, Context, MouseButton, MouseDownEvent, Window, actions, div, prelude::*, px,
};

actions!(mdoc_navigation, [Back, Forward]);

impl Workspace {
    /// The place on screen, as a stop.
    pub(super) fn place(&self, cx: &App) -> Stop {
        if let Some(page) = &self.cases.page {
            Stop::Case(page.id().into())
        } else if self.showing_home(cx) {
            Stop::Home
        } else {
            Stop::Tab(self.tabs.active())
        }
    }

    pub(super) fn note_destination(&mut self, arrival: Arrival, cx: &App) {
        let stop = self.place(cx);
        self.navigation.arrive(stop, arrival);
    }

    /// The one liveness rule for the buttons, their tooltips and the
    /// actions: Home, an open tab that isn't Home's placeholder, a case
    /// that still exists.
    fn live_stops<'a>(&'a self, cx: &'a App) -> impl Fn(&Stop) -> bool + 'a {
        let cases: Vec<String> = self
            .case_coordinator
            .summaries()
            .into_iter()
            .map(|case| case.id)
            .collect();
        move |stop| match stop {
            Stop::Home => true,
            Stop::Tab(id) => self
                .tabs
                .get(*id)
                .is_some_and(|tab| !self.is_placeholder(tab, cx)),
            Stop::Case(id) => cases.contains(id),
        }
    }

    pub(super) fn navigation_target(&self, backwards: bool, cx: &App) -> Option<&Stop> {
        self.navigation
            .target(backwards, &self.place(cx), self.live_stops(cx))
    }

    pub(super) fn navigation_label(&self, backwards: bool, cx: &App) -> String {
        let direction = if backwards { "Back" } else { "Forward" };
        let destination = match self.navigation_target(backwards, cx) {
            Some(Stop::Home) => Some("Home".into()),
            Some(Stop::Tab(id)) => self.tabs.get(*id).map(|tab| super::tab_label(tab, cx).name),
            Some(Stop::Case(id)) => self
                .case_coordinator
                .summaries()
                .into_iter()
                .find(|case| &case.id == id)
                .map(|case| format!("Case “{}”", case.name)),
            None => None,
        };
        match destination {
            Some(destination) => format!("{direction} to {destination}"),
            None => direction.into(),
        }
    }

    pub(super) fn navigate(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Navigation must preserve the restrictions of stacked modal surfaces.
        if self.settings.read(cx).open
            || self.cases.list_open
            || self.cases.explainer
            || self.folder_sheets.open()
            || self.shell.palette.is_some()
            || window.has_active_prompt()
            || self.quit_flow.quitting()
        {
            return;
        }
        let here = self.place(cx);
        let Some(index) = self
            .navigation
            .target_index(backwards, &here, self.live_stops(cx))
        else {
            return;
        };
        let stop = self.navigation.go(index);
        self.close_shortcuts(window, cx);
        // A menu belongs to the page it was opened over.
        self.tab_menu = None;
        self.cases.header_menu = None;
        self.menu_return = None;
        // The cursor is already on this stop, so the destination's own
        // visit is a no-op and preserves the forward branch.
        match stop {
            Stop::Home => self.show_home(window, cx),
            Stop::Tab(id) => self.activate(id, window, cx),
            Stop::Case(id) => self.show_case_page(&id, Place::Header, window, cx),
        }
    }

    pub(super) fn navigation_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let mut buttons = div().flex().items_center().gap_1();
        for (backwards, id, glyph, focus) in [
            (true, "workspace-back", "‹", &self.back_focus),
            (false, "workspace-forward", "›", &self.forward_focus),
        ] {
            let enabled = self.navigation_target(backwards, cx).is_some();
            let label = self.navigation_label(backwards, cx);
            buttons = buttons.child(
                ui::control(id, glyph, theme, enabled)
                    .track_focus(&focus.clone().tab_stop(enabled))
                    .aria_label(label.clone())
                    .when(!enabled, |v| {
                        v.a11y_synthetic_children(|tree| tree.parent_node().set_disabled())
                    })
                    .tooltip(style::tooltip(label, theme))
                    .p_0()
                    .size(px(28.))
                    .text_size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(cfg!(test), |v| v.debug_selector(move || id.into()))
                    .when(enabled, |v| {
                        v.on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate(backwards, window, cx);
                        }))
                    }),
            );
        }
        ui::no_drag(buttons).into_any_element()
    }

    pub(super) fn navigation_mouse(&self, cx: &mut Context<Self>) -> AnyElement {
        let workspace = cx.entity().downgrade();
        // A window capture listener also reaches the editor and occluding
        // popovers. A physical press navigates once, never again on mouse-up.
        gpui::canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let workspace = workspace.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase.capture()
                        && let MouseButton::Navigate(direction) = event.button
                    {
                        let _ = workspace.update(cx, |this, cx| {
                            this.navigate(direction == gpui::NavigationDirection::Back, window, cx);
                            cx.stop_propagation();
                        });
                    }
                });
            },
        )
        .absolute()
        .size_0()
        .into_any_element()
    }
}
