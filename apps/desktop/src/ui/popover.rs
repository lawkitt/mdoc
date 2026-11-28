//! The anchored popover menu: where it floats over the window, and the
//! panel's keys and dismissal. Each menu keeps its own rows, size, offset,
//! and what closing it does with focus.
use gpui::{
    Anchor, Context, Deferred, FocusHandle, MouseDownEvent, Pixels, Point, Stateful, Window,
    prelude::*,
};

/// Where a menu floats: hung from a corner, at a point, or both, kept
/// `margin` inside the window (or at its edge when `None`).
pub struct Placement {
    pub corner: Option<Anchor>,
    pub position: Option<Point<Pixels>>,
    pub margin: Option<Pixels>,
}

/// The menu floating over the window on layer `layer`: header menus are 1,
/// page menus 2, and menus over the mention bar's card 3.
pub fn popover(placement: Placement, layer: usize, menu: impl IntoElement) -> Deferred {
    let mut anchored = gpui::anchored();
    if let Some(corner) = placement.corner {
        anchored = anchored.anchor(corner);
    }
    if let Some(position) = placement.position {
        anchored = anchored.position(position);
    }
    let anchored = match placement.margin {
        Some(margin) => anchored.snap_to_window_with_margin(margin),
        None => anchored.snap_to_window(),
    };
    gpui::deferred(anchored.child(menu)).with_priority(layer)
}

/// A menu panel's keys and dismissal.
pub trait MenuPanel: Sized {
    /// Holds focus in `group` under `context`: Tab and Shift+Tab cycle its
    /// stops, and Esc runs `close`, which decides where focus goes.
    fn menu_focus<V: 'static>(
        self,
        group: &FocusHandle,
        context: &str,
        cx: &mut Context<V>,
        close: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    ) -> Self;

    /// A press outside the panel runs `outside`, which decides whether it
    /// closes the menu (a press on its own button is that button's toggle).
    fn menu_dismiss<V: 'static>(
        self,
        cx: &mut Context<V>,
        outside: impl Fn(&mut V, &MouseDownEvent, &mut Window, &mut Context<V>) + 'static,
    ) -> Self;
}

impl MenuPanel for Stateful<gpui::Div> {
    fn menu_focus<V: 'static>(
        self,
        group: &FocusHandle,
        context: &str,
        cx: &mut Context<V>,
        close: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        let (next, previous) = (group.clone(), group.clone());
        self.track_focus(group)
            .key_context(context)
            .on_action(cx.listener(move |_, _: &super::NextControl, window, cx| {
                super::cycle(window, cx, Some(&next), false);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(move |_, _: &super::PreviousControl, window, cx| {
                    super::cycle(window, cx, Some(&previous), true);
                    cx.stop_propagation();
                }),
            )
            .on_action(
                cx.listener(move |this, _: &super::CloseMenu, window, cx| close(this, window, cx)),
            )
    }

    fn menu_dismiss<V: 'static>(
        self,
        cx: &mut Context<V>,
        outside: impl Fn(&mut V, &MouseDownEvent, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        self.on_mouse_down_out(
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                outside(this, event, window, cx)
            }),
        )
    }
}
