//! The Back and Forward buttons' view, and the mouse's back and forward
//! buttons; carrying out a stop is in `actions.rs`.
use crate::workspace::Workspace;
use crate::{ui, ui::style};
use gpui::{AnyElement, Context, MouseButton, MouseDownEvent, actions, div, prelude::*, px};

actions!(mdoc_navigation, [Back, Forward]);

impl Workspace {
    pub(in crate::workspace) fn navigation_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
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
                    .tooltip(style::tooltip(
                        if backwards {
                            crate::commands::shortcuts::tip(label, &Back)
                        } else {
                            crate::commands::shortcuts::tip(label, &Forward)
                        },
                        theme,
                    ))
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
    pub(in crate::workspace) fn navigation_mouse(&self, cx: &mut Context<Self>) -> AnyElement {
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
