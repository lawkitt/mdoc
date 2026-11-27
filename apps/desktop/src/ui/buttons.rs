//! Buttons: text, icon, split and floating controls, and the key hint
//! inside one.

use crate::ui::{Control, Icon, focus_ring, style::Theme};
use gpui::{div, prelude::*, px};

pub fn action_control(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
    primary: bool,
) -> Control {
    let p = theme.pdf_style();
    control(id, label, theme, enabled)
        .px_3()
        .py_1()
        .border_1()
        .border_color(p.border)
        .when(primary, |v| outlined(v, theme))
}

/// The prominent-action look shared by primary and floating buttons: an
/// outlined button on the window background, in medium weight.
fn outlined(v: Control, theme: Theme) -> Control {
    let p = theme.pdf_style();
    v.rounded_lg()
        .bg(p.bg)
        .border_1()
        .border_color(p.border)
        .text_color(p.header_fg)
        .font_weight(gpui::FontWeight::MEDIUM)
}

/// A card button without the shared hover/focus styles, which callers set.
fn card_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    enabled: bool,
    theme: Theme,
) -> Control {
    let label = label.into();
    focus_ring(id, theme)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .key_context("UiControl")
        .tab_index(0)
        .tab_stop(enabled)
        .flex_shrink_0()
        .px_2()
        .py_1()
        .rounded_md()
        .text_size(px(13.))
        .child(label)
        .when(enabled, |v| v.cursor_pointer())
        .when(!enabled, |v| v.opacity(0.45))
}

/// The one prominent action of a card or dialog.
pub fn primary_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> Control {
    let p = theme.pdf_style();
    outlined(card_button(id, label, enabled, theme).px_3(), theme)
        .when(enabled, |v| v.hover(move |s| s.bg(p.placeholder_bg)))
}

/// The header's one filled button: ink on paper inverted, so it is
/// the darkest thing in light mode and the brightest in dark.
pub fn filled_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> Control {
    let p = theme.pdf_style();
    let label = label.into();
    focus_ring(id, theme)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .key_context("UiControl")
        .tab_index(0)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .h(px(28.))
        .px_3()
        .rounded_md()
        .bg(p.header_fg)
        .text_color(p.bg)
        .text_size(px(13.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .child(label)
        .when(enabled, |v| v.cursor_pointer().hover(|s| s.opacity(0.88)))
        .when(!enabled, |v| v.opacity(0.45))
}

/// A keyboard hint inside a button or menu row, such as "⌘↩".
pub fn key_hint(text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .px(px(4.))
        .rounded_sm()
        .border_1()
        .border_color(gpui::hsla(0., 0., 0.5, 0.45))
        .text_size(px(11.))
        .opacity(0.8)
        .child(text.into())
}

/// A 28 px header icon control, quieter than [`icon_button`].
pub fn header_icon(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    icon: Icon,
    theme: Theme,
    on: bool,
) -> Control {
    let label = label.into();
    header_icon_untipped(id, label.clone(), icon, theme, on)
        .tooltip(crate::ui::style::tooltip(label.to_string(), theme))
}

/// A header icon that opens a menu or popover: its tooltip shows only while
/// that is closed, so it never sits on top of the menu it opened.
pub fn header_menu_icon(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    icon: Icon,
    theme: Theme,
    open: bool,
) -> Control {
    let label = label.into();
    let button = header_icon_untipped(id, label.clone(), icon, theme, open);
    if open {
        button
    } else {
        button.tooltip(crate::ui::style::tooltip(label.to_string(), theme))
    }
}

/// A header icon whose caller sets its own tooltip.
pub fn header_icon_untipped(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    icon: Icon,
    theme: Theme,
    on: bool,
) -> Control {
    let p = theme.pdf_style();
    control_base(id, label.into(), theme, true)
        .p_0()
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .when(on, |v| v.bg(p.placeholder_bg))
        .child(
            gpui::svg()
                .data(icon.data())
                .size(px(16.))
                .text_color(if on { p.header_fg } else { p.header_muted }),
        )
}

/// A split button: `main` repeats the last choice, `more` opens the menu
/// of every choice. Both are 28 px high, in one outline.
pub fn split_button(
    main: Control,
    more_id: impl Into<gpui::ElementId>,
    more_label: impl Into<gpui::SharedString>,
    open: bool,
    theme: Theme,
) -> (gpui::Div, Control) {
    let p = theme.pdf_style();
    let more = header_menu_icon(more_id, more_label, Icon::ChevronDown, theme, open)
        .w(px(22.))
        .h(px(26.));
    let frame = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(28.))
        .rounded_md()
        .border_1()
        .border_color(p.border)
        .child(
            main.h(px(26.))
                .flex()
                .items_center()
                .font_weight(gpui::FontWeight::MEDIUM),
        )
        .child(div().w(px(1.)).h(px(16.)).bg(p.border));
    (frame, more)
}

/// A tertiary text action, such as "Choose model…".
pub fn link_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> Control {
    card_button(id, label, enabled, theme)
        .text_color(theme.search_accent())
        .when(enabled, |v| v.hover(|s| s.underline()))
}

fn control_base(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> Control {
    let p = theme.pdf_style();
    let label = label.into();
    focus_ring(id, theme)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .key_context("UiControl")
        .tab_index(0)
        .tab_stop(enabled)
        .flex_shrink_0()
        .px_2()
        .py_1()
        .rounded_md()
        .text_size(px(13.))
        .text_color(p.header_fg)
        .when(enabled, |v| {
            v.cursor_pointer().hover(move |s| s.bg(p.placeholder_bg))
        })
        .when(!enabled, |v| v.opacity(0.45))
}

pub fn control(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> Control {
    let label = label.into();
    control_base(id, label.clone(), theme, enabled).when(!label.is_empty(), |v| v.child(label))
}

/// A close button matching the Original viewer's.
pub fn close_control(id: impl Into<gpui::ElementId>, theme: Theme) -> Control {
    gpui_pdf::close_control(id, &theme.pdf_style())
        .key_context("UiControl")
        .tab_stop(true)
}

/// The height of a [`floating_button`]; labels floating beside one center on it.
pub const FLOATING_BUTTON_HEIGHT: f32 = 32.;

/// An outlined icon-and-text control that floats over content; `label` is its
/// tooltip and accessible name, `text` what it shows.
pub fn floating_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    icon: Icon,
    text: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> Control {
    let p = theme.pdf_style();
    let label = label.into();
    outlined(control_base(id, label.clone(), theme, enabled), theme)
        .h(px(FLOATING_BUTTON_HEIGHT))
        .px_3()
        .py_0()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .child(
            gpui::svg()
                .data(icon.data())
                .size(px(16.))
                .text_color(p.header_fg),
        )
        .child(text.into())
        .tooltip(crate::ui::style::tooltip(label.to_string(), theme))
}
