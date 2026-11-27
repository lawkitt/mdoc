//! Popover menu rows and their divider.

use crate::ui::{Control, Icon, focus_ring, style::Theme};
use gpui::{div, prelude::*, px};

/// One row of a popover menu: label, optional check and shortcut.
pub fn menu_row(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    shortcut: Option<&'static str>,
    checked: Option<bool>,
    theme: Theme,
    enabled: bool,
) -> Control {
    menu_row_because(id, label, shortcut, checked, theme, enabled, None)
}

/// A menu row that, when disabled, says why under its label. Screen readers
/// hear the same reason as the row's description.
pub fn menu_row_because(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    shortcut: Option<&'static str>,
    checked: Option<bool>,
    theme: Theme,
    enabled: bool,
    reason: Option<gpui::SharedString>,
) -> Control {
    let p = theme.pdf_style();
    let label = label.into();
    let reason = reason.filter(|_| !enabled);
    // Disabled rows fade their label and shortcut; the reason stays legible.
    let fade = if enabled { 1. } else { 0.45 };
    focus_ring(id, theme)
        .role(gpui::Role::MenuItem)
        .aria_label(label.clone())
        .when_some(reason.clone(), |v, reason| {
            v.aria_description(super::text::unavailable(reason))
        })
        .key_context("UiControl")
        .tab_index(0)
        .flex()
        .items_center()
        .gap_2()
        .min_w(px(220.))
        .px_2()
        .min_h(px(28.))
        .when(reason.is_some(), |v| v.py_1())
        .rounded_md()
        .text_size(px(13.))
        .when(enabled, |v| {
            v.cursor_pointer().hover(move |s| s.bg(p.placeholder_bg))
        })
        .child(
            div()
                .w(px(14.))
                .flex_shrink_0()
                .opacity(fade)
                .when(checked == Some(true), |v| {
                    v.child(
                        gpui::svg()
                            .data(Icon::Check.data())
                            .size(px(14.))
                            .text_color(p.header_fg),
                    )
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().truncate().opacity(fade).child(label))
                .when_some(reason, |v, reason| {
                    v.child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .child(reason),
                    )
                }),
        )
        .when_some(shortcut, |v, s| {
            v.child(
                div()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .opacity(fade)
                    .child(s),
            )
        })
}

/// A thin divider between menu groups.
pub fn menu_divider(theme: Theme) -> gpui::Div {
    div().my_1().h(px(1.)).mx_2().bg(theme.pdf_style().border)
}
