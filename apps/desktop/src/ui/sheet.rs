//! The modal sheet: a scrim over the window and the panel on it, which keeps
//! Tab inside. Each sheet chains its own size, keys, commands and footer on
//! what these return.
use super::style::Theme;
use gpui::{FocusHandle, div, prelude::*, px};

/// The scrim: the window dimmed, nothing behind it taking the pointer, and
/// the panel centred `top` below the window's top edge. The caller sizes it:
/// laid over the window's content from a deferred layer, or sized to the
/// window in a view of its own (the prompt's).
pub fn sheet_scrim(id: &'static str, theme: Theme, top: f32) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_start()
        .justify_center()
        .pt(px(top))
        .bg(theme.scrim())
        // A modal: nothing behind it takes the pointer.
        .occlude()
}

/// The panel: a dialog named `label`, under the key context `context`
/// beside `UiPanel`. Tab and Shift+Tab cycle the stops inside `group` and
/// never leave it.
pub fn sheet_panel(
    id: &'static str,
    theme: Theme,
    role: gpui::Role,
    label: impl Into<gpui::SharedString>,
    group: &FocusHandle,
    context: Option<&str>,
) -> gpui::Stateful<gpui::Div> {
    let (next, previous) = (group.clone(), group.clone());
    super::panel(id, theme)
        .when(cfg!(test), |v| v.debug_selector(move || id.into()))
        .role(role)
        .aria_label(label)
        .key_context(
            context
                .map_or_else(|| "UiPanel".to_string(), |c| format!("{c} UiPanel"))
                .as_str(),
        )
        .track_focus(group)
        .tab_group()
        .tab_stop(false)
        .on_action(move |_: &super::NextControl, window, cx| {
            super::cycle(window, cx, Some(&next), false);
            cx.stop_propagation();
        })
        .on_action(move |_: &super::PreviousControl, window, cx| {
            super::cycle(window, cx, Some(&previous), true);
            cx.stop_propagation();
        })
}
