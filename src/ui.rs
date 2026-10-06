//! Shared chrome controls. GPUI activates focused click controls with Enter/Space.
use crate::style::Theme;
use gpui::{App, FocusHandle, KeyBinding, Window, actions, div, prelude::*, px};

actions!(ui, [NextControl, PreviousControl, CloseMenu]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", NextControl, Some("UiControl || UiPanel")),
        KeyBinding::new("shift-tab", PreviousControl, Some("UiControl || UiPanel")),
        KeyBinding::new("escape", CloseMenu, Some("UiMenu")),
    ]);
}

pub fn cycle(window: &mut Window, cx: &mut App, group: Option<&FocusHandle>, backwards: bool) {
    for _ in 0..256 {
        if backwards {
            window.focus_prev(cx);
        } else {
            window.focus_next(cx);
        }
        if group.is_none_or(|group| group.contains_focused(window, cx)) {
            break;
        }
    }
}

fn control_base(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let p = theme.pdf_style();
    let label = label.into();
    div()
        .id(id)
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
        .focus_visible(move |s| s.bg(p.placeholder_bg).text_color(theme.search_accent()))
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
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    control_base(id, label.clone(), theme, enabled).child(label)
}

#[derive(Clone, Copy)]
pub enum Icon {
    Open,
    Save,
    SaveAs,
    Sun,
    Moon,
    Settings,
    Anonymous,
    ChevronDown,
}

impl Icon {
    fn data(self) -> &'static [u8] {
        match self {
            Self::Open => include_bytes!("../resources/ui/open.svg"),
            Self::Save => include_bytes!("../resources/ui/save.svg"),
            Self::SaveAs => include_bytes!("../resources/ui/save-as.svg"),
            Self::Sun => include_bytes!("../resources/ui/sun.svg"),
            Self::Moon => include_bytes!("../resources/ui/moon.svg"),
            Self::Settings => include_bytes!("../resources/ui/settings.svg"),
            Self::Anonymous => include_bytes!("../resources/ui/anonymous.svg"),
            Self::ChevronDown => include_bytes!("../resources/ui/chevron-down.svg"),
        }
    }
}

pub fn icon_control(
    id: &'static str,
    label: &'static str,
    icon: Icon,
    theme: Theme,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    control_base(id, label, theme, enabled)
        .p_0()
        .size(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .child(
            gpui::svg()
                .data(icon.data())
                .size(px(18.))
                .text_color(theme.pdf_style().header_fg),
        )
        .tooltip(crate::style::tooltip(label.into(), theme))
        .when(cfg!(test), |v| v.debug_selector(move || id.into()))
}

/// Reveal a focused control inside a scrolling panel without moving document state.
pub fn reveal_focus(
    control: gpui::Stateful<gpui::Div>,
    focus: FocusHandle,
    scroll: gpui::ScrollHandle,
) -> gpui::Stateful<gpui::Div> {
    control.relative().child(
        gpui::canvas(
            move |bounds, window, cx| {
                if !focus.contains_focused(window, cx) {
                    return;
                }
                let viewport = scroll.bounds();
                if viewport.size.height <= px(0.) {
                    return;
                }
                let delta = if bounds.top() < viewport.top() {
                    viewport.top() - bounds.top()
                } else if bounds.bottom() > viewport.bottom() {
                    viewport.bottom() - bounds.bottom()
                } else {
                    px(0.)
                };
                if f32::from(delta).abs() > 0.5 {
                    let scroll = scroll.clone();
                    let focus = focus.clone();
                    let current = scroll.offset();
                    let max = scroll.max_offset().y.max(px(0.));
                    let offset = gpui::point(current.x, (current.y + delta).clamp(-max, px(0.)));
                    if offset == current {
                        return;
                    }
                    window.defer(cx, move |window, cx| {
                        if focus.contains_focused(window, cx) {
                            scroll.set_offset(offset);
                            window.refresh();
                        }
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0(),
    )
}
