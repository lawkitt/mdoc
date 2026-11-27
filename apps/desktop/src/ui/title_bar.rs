//! The title bar: its drag area, the drawn caption buttons, and the
//! header's notice bell.

use crate::ui::{Icon, header_menu_icon, style::Theme};
use gpui::{AnyElement, Window, div, prelude::*, px};

/// The header's bell while workspace notices wait: an amber dot
/// with the count; a click opens or closes the notices popover.
pub fn notice_bell(title: &crate::ui::style::TitleBar, theme: Theme) -> Option<AnyElement> {
    let count = title.notices.get();
    if count == 0 {
        return None;
    }
    let open = title.notices_open.get();
    let amber = crate::ui::style::markdown_style(theme).alert_warning;
    let bell = title.notices_bell.clone();
    Some(
        div()
            .relative()
            .child(
                gpui::canvas(move |bounds, _, _| bell.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .child(
                header_menu_icon(
                    "header-notices",
                    if count == 1 {
                        "1 notice".to_string()
                    } else {
                        format!("{count} notices")
                    },
                    Icon::Bell,
                    theme,
                    open,
                )
                .when(cfg!(test), |v| v.debug_selector(|| "header-notices".into()))
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(crate::workspace::ToggleNotices), cx)
                }),
            )
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .right(px(2.))
                    .min_w(px(14.))
                    .h(px(14.))
                    .px(px(3.))
                    .rounded_full()
                    .bg(amber)
                    .text_color(theme.badge_fg())
                    .text_size(px(10.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(count.to_string()),
            )
            .into_any_element(),
    )
}

/// Make `area` move the window like a title bar: a press that
/// moves drags the window, a double click zooms. Controls inside stop the
/// press first ([`no_drag`]), so clicking them never moves the window.
/// Windows hit-tests the area as the native caption, which already moves
/// the window and toggles maximize on a double click.
pub fn title_drag(area: gpui::Div, armed: std::rc::Rc<std::cell::Cell<bool>>) -> gpui::Div {
    let area = area.window_control_area(gpui::WindowControlArea::Drag);
    if cfg!(target_os = "windows") {
        return area;
    }
    let (down, up, out) = (armed.clone(), armed.clone(), armed.clone());
    area.on_mouse_down(gpui::MouseButton::Left, move |event, window, _| {
        if event.click_count == 2 {
            down.set(false);
            if cfg!(target_os = "macos") {
                window.titlebar_double_click();
            } else {
                window.zoom_window();
            }
        } else {
            down.set(true);
        }
    })
    .on_mouse_up(gpui::MouseButton::Left, move |_, _, _| up.set(false))
    .on_mouse_down_out(move |_, _, _| out.set(false))
    .on_mouse_move(move |event, window, _| {
        if armed.get() && event.pressed_button == Some(gpui::MouseButton::Left) {
            armed.set(false);
            window.start_window_move();
        }
    })
}

/// Keep a press on `control` from reaching an enclosing [`title_drag`].
pub fn no_drag<E: InteractiveElement>(control: E) -> E {
    control.on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// Drawn window buttons for Windows and client-decorated Linux;
/// macOS keeps its traffic lights. Windows hit-tests them natively and acts
/// on release only if the press reached it, so they must not stop it there.
/// Place them with [`title_bar`].
fn caption_buttons(window: &Window, theme: Theme) -> Option<AnyElement> {
    if cfg!(target_os = "macos") || cfg!(test) {
        return None;
    }
    if cfg!(target_os = "linux") && matches!(window.window_decorations(), gpui::Decorations::Server)
    {
        return None;
    }
    let p = theme.pdf_style();
    let button = |id: &'static str, glyph: &'static str, area: gpui::WindowControlArea| {
        div()
            .id(id)
            .w(px(40.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(13.))
            .text_color(p.header_muted)
            .hover(move |s| s.bg(p.placeholder_bg))
            .window_control_area(area)
            .child(glyph)
            .when(!cfg!(target_os = "windows"), |b| {
                b.on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            })
            .on_click(move |_, window, cx| {
                if cfg!(target_os = "linux") {
                    match area {
                        gpui::WindowControlArea::Min => window.minimize_window(),
                        gpui::WindowControlArea::Max => window.zoom_window(),
                        gpui::WindowControlArea::Close => {
                            window.dispatch_action(Box::new(crate::workspace::Quit), cx)
                        }
                        gpui::WindowControlArea::Drag => {}
                    }
                }
            })
    };
    Some(
        div()
            .flex()
            .h(px(crate::ui::style::HEADER_HEIGHT))
            .flex_shrink_0()
            .child(button("caption-min", "—", gpui::WindowControlArea::Min))
            .child(button("caption-max", "☐", gpui::WindowControlArea::Max))
            .child(button("caption-close", "✕", gpui::WindowControlArea::Close))
            .into_any_element(),
    )
}

/// A header row: `bar` (a [`title_drag`] area) with the caption buttons
/// beside it, not inside it. Windows asks GPUI which control area is under
/// the pointer, and GPUI answers with the first one painted, so an enclosing
/// drag area would turn the buttons into plain caption. `rule` repeats the
/// bar's bottom border under the buttons.
pub fn title_bar(
    bar: gpui::Div,
    rule: Option<gpui::Hsla>,
    window: &Window,
    theme: Theme,
) -> AnyElement {
    let Some(buttons) = caption_buttons(window, theme) else {
        return bar.into_any_element();
    };
    div()
        .flex()
        .items_end()
        .flex_shrink_0()
        .child(bar.flex_1().min_w_0())
        .child(
            div()
                .flex_shrink_0()
                .when_some(rule, |v, color| v.border_b_1().border_color(color))
                .child(buttons),
        )
        .into_any_element()
}
