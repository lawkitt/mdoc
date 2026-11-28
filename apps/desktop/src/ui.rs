//! Shared chrome controls, one file per kind: buttons, menus, cards, the
//! Copied feedback and the title bar, beside the style and the confirm
//! dialog. Here stay what every kind builds on: focus and keyboard reach,
//! status and spoken text, icons, the popover surface and page ranges. GPUI
//! activates focused click controls with Enter/Space.
mod buttons;
mod cards;
pub(crate) mod confirm_dialog;
mod copied;
mod menus;
pub(crate) mod style;
pub(crate) mod text;
mod title_bar;

use crate::ui::style::Theme;
use gpui::{
    Animation, AnimationExt, AnyElement, App, FocusHandle, KeyBinding, Window, actions, div,
    prelude::*, px,
};
use std::time::Duration;

pub use buttons::{
    FLOATING_BUTTON_HEIGHT, action_control, close_control, control, filled_button, floating_button,
    header_icon, header_icon_untipped, header_menu_icon, key_hint, link_button, primary_button,
    split_button,
};
pub use cards::{
    card_actions, card_error, card_hint, card_note, card_surface, card_text, card_title,
    model_attribution, setup_progress, state_card,
};
pub use copied::{copied_look, copy_label};
pub use gpui_pdf::FocusRing as Control;
pub use menus::{menu_divider, menu_row, menu_row_because};
pub use title_bar::{no_drag, notice_bell, title_bar, title_drag};

/// The one keyboard focus indicator.
pub fn focus_ring(id: impl Into<gpui::ElementId>, theme: Theme) -> Control {
    gpui_pdf::focus_ring(id, theme.search_accent())
}

/// A common surface for app-owned popovers and dialogs. Callers own sizing/focus.
pub fn panel(id: impl Into<gpui::ElementId>, theme: Theme) -> gpui::Stateful<gpui::Div> {
    let p = theme.pdf_style();
    div()
        .id(id)
        .occlude()
        .rounded_md()
        .shadow_md()
        .bg(p.bg)
        .border_1()
        .border_color(p.border)
        .text_color(p.header_fg)
        .text_size(px(13.))
}

/// Only mounted while busy. The first phase reserves space without flashing;
/// the repeating phase respects GPUI's reduced-motion preference.
pub fn spinner(id: impl Into<gpui::ElementId>, theme: Theme) -> AnyElement {
    gpui::svg()
        .data(include_bytes!("../resources/ui/spinner.svg"))
        .size(px(14.))
        .flex_shrink_0()
        .text_color(theme.search_accent())
        .with_animations(
            id,
            vec![
                Animation::new(Duration::from_millis(150)).with_max_fps(20.),
                Animation::new(Duration::from_millis(900))
                    .repeat()
                    .with_max_fps(30.),
            ],
            |icon, phase, delta| {
                icon.opacity(if phase == 0 { 0. } else { 1. })
                    .with_transformation(gpui::Transformation::rotate(gpui::radians(
                        if phase == 0 {
                            0.
                        } else {
                            delta * std::f32::consts::TAU
                        },
                    )))
            },
        )
        .into_any_element()
}

pub fn activity(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .min_w_0()
        .text_size(px(12.))
        .text_color(theme.pdf_style().header_muted)
        .role(gpui::Role::Status)
        .aria_label(label.clone())
        .child(spinner("activity-spinner", theme))
        .child(div().min_w_0().child(label))
}

/// A counter or status line for screen readers: a Label node whose
/// value is `spoken`, in a polite live region, so VoiceOver and Narrator read
/// it and announce each change. Keep `id` stable across states so a change is
/// an update of one node, not a new one.
pub fn live_text(
    id: impl Into<gpui::ElementId>,
    spoken: impl Into<gpui::SharedString>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::Role::Label)
        .aria_value(spoken)
        .a11y_synthetic_children(|tree| tree.parent_node().set_live(gpui::accesskit::Live::Polite))
}

/// What a screen reader says for a "4/6" position: "Mention 4 of 6".
pub fn position_announcement(noun: &str, position: usize, total: usize) -> String {
    text::position_of(noun, position, total)
}

/// Real byte counts only: the caller must not pass stale counts from a
/// verification/runtime phase.
pub fn progress_bar(received: u64, total: u64, theme: Theme) -> gpui::Div {
    let fraction = if total == 0 {
        0.
    } else {
        (received as f32 / total as f32).clamp(0., 1.)
    };
    div()
        .h(px(3.))
        .w_full()
        .rounded_full()
        .bg(theme.pdf_style().placeholder_bg)
        .child(
            div()
                .h_full()
                .w(gpui::relative(fraction))
                .rounded_full()
                .bg(theme.search_accent()),
        )
}

/// "3–7, 12", truncated after `limit` runs.
pub fn page_ranges(pages: &[u32], limit: usize) -> String {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &page in pages {
        match runs.last_mut() {
            Some((_, end)) if page == *end + 1 => *end = page,
            _ => runs.push((page, page)),
        }
    }
    let mut text = runs
        .iter()
        .take(limit)
        .map(|&(a, b)| {
            if a == b {
                a.to_string()
            } else {
                format!("{a}–{b}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if runs.len() > limit {
        text.push_str(", …");
    }
    text
}

actions!(
    ui,
    [
        NextControl,
        PreviousControl,
        CloseMenu,
        NextRegion,
        PreviousRegion,
        FocusEditor,
        SidebarPrevious,
        SidebarNext,
        SidebarCollapse,
        SidebarExpand,
        SidebarExtendUp,
        SidebarExtendDown,
        SidebarDelete,
        SidebarMenu,
    ]
);

/// Tab inside a control group here; keyboard reach, menus and
/// the sidebar's keys from the shortcuts table.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", NextControl, Some("UiControl || UiPanel")),
        KeyBinding::new("shift-tab", PreviousControl, Some("UiControl || UiPanel")),
    ]);
    let bindings = crate::commands::shortcuts::bindings(crate::commands::shortcuts::Owner::Ui, cx);
    cx.bind_keys(bindings);
}

pub fn cycle(window: &mut Window, cx: &mut App, group: Option<&FocusHandle>, backwards: bool) {
    let start = window.focused(cx);
    let mut seen = Vec::new();
    // One full lap of the window's Tab stops at most; with no stop inside
    // `group`, focus goes back where it was. Stop at any repeat, not only
    // the first stop: a handle tracked by two elements leaves a duplicate
    // stop, and the walk from it never comes back to where it began.
    loop {
        if backwards {
            window.focus_prev(cx);
        } else {
            window.focus_next(cx);
        }
        if group.is_none_or(|group| group.contains_focused(window, cx)) {
            return;
        }
        match window.focused(cx) {
            Some(now) if !seen.contains(&now) => seen.push(now),
            _ => break,
        }
    }
    if let Some(start) = start {
        window.focus(&start, cx);
    }
}

#[derive(Clone, Copy)]
pub enum Icon {
    Open,
    Save,
    Sun,
    Moon,
    Settings,
    Anonymous,
    Filter,
    Sidebar,
    Copy,
    Check,
    Ocr,
    Close,
    Home,
    Search,
    More,
    PanelRight,
    Help,
    Source,
    ChevronRight,
    ChevronDown,
    ExpandAll,
    CollapseAll,
    Plus,
    Reply,
    ThemeSystem,
    Download,
    Bell,
    Clock,
}

impl Icon {
    pub fn data(self) -> &'static [u8] {
        match self {
            Self::Open => include_bytes!("../resources/ui/open.svg"),
            Self::Save => include_bytes!("../resources/ui/save.svg"),
            Self::Sun => include_bytes!("../resources/ui/sun.svg"),
            Self::Moon => include_bytes!("../resources/ui/moon.svg"),
            Self::Settings => include_bytes!("../resources/ui/settings.svg"),
            Self::Anonymous => include_bytes!("../resources/ui/anonymous.svg"),
            Self::Filter => include_bytes!("../resources/ui/filter.svg"),
            Self::Sidebar => include_bytes!("../resources/ui/sidebar.svg"),
            Self::Copy => include_bytes!("../resources/ui/copy.svg"),
            Self::Check => include_bytes!("../resources/ui/check.svg"),
            Self::Ocr => include_bytes!("../resources/ui/ocr.svg"),
            Self::Close => include_bytes!("../resources/ui/close.svg"),
            Self::Home => include_bytes!("../resources/ui/home.svg"),
            Self::Search => include_bytes!("../resources/ui/search.svg"),
            Self::More => include_bytes!("../resources/ui/more.svg"),
            Self::PanelRight => include_bytes!("../resources/ui/panel-right.svg"),
            Self::Help => include_bytes!("../resources/ui/help.svg"),
            Self::Source => include_bytes!("../resources/ui/source.svg"),
            Self::ChevronRight => include_bytes!("../resources/ui/chevron-right.svg"),
            Self::ChevronDown => include_bytes!("../resources/ui/chevron-down.svg"),
            Self::ExpandAll => include_bytes!("../resources/ui/expand-all.svg"),
            Self::CollapseAll => include_bytes!("../resources/ui/collapse-all.svg"),
            Self::Plus => include_bytes!("../resources/ui/plus.svg"),
            Self::Reply => include_bytes!("../resources/ui/reply.svg"),
            Self::ThemeSystem => include_bytes!("../resources/ui/theme-system.svg"),
            Self::Download => include_bytes!("../resources/ui/download.svg"),
            Self::Bell => include_bytes!("../resources/ui/bell.svg"),
            Self::Clock => include_bytes!("../resources/ui/clock.svg"),
        }
    }
}

/// Reveal a control inside a scrolling panel when it gains focus, without
/// moving document state. Revealing only on arrival leaves the wheel free to
/// move a focused control away.
pub fn reveal_focus<E: ParentElement + Styled>(
    mut control: E,
    focus: FocusHandle,
    scroll: gpui::ScrollHandle,
) -> E {
    // Absolute children use the padding box, inside the control's border.
    let border = control.style().border_widths.clone();
    control.relative().child(
        gpui::canvas(
            move |bounds, window, cx| {
                let focused = focus.contains_focused(window, cx);
                let viewport = scroll.bounds();
                let ready = viewport.size.height > px(0.);
                // Whether the control was focused when it last drew. The state
                // lives only while the control draws in consecutive frames, so a
                // control that was unmounted counts focus on return as new.
                let arrived = window.with_global_id(
                    gpui::ElementId::Name(format!("reveal-{focus:?}").into()),
                    |id, window| {
                        window.with_element_state(id, |was: Option<bool>, _| {
                            let was = was.unwrap_or(false);
                            (focused && !was && ready, focused && (was || ready))
                        })
                    },
                );
                if !arrived {
                    return;
                }
                // Reveal the outside outline too, rather than scrolling its
                // bottom stroke underneath the viewport's fixed footer.
                let border = [border.top, border.right, border.bottom, border.left]
                    .into_iter()
                    .flatten()
                    .map(|width| width.to_pixels(window.rem_size()))
                    .fold(px(0.), |a, b| a.max(b));
                let bounds = bounds.dilate(px(gpui_pdf::FOCUS_RING_OUTSET) + border);
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

#[cfg(test)]
mod tests {
    #[test]
    fn positions_are_spoken_in_words() {
        assert_eq!(
            super::position_announcement("Mention", 4, 6),
            "Mention 4 of 6"
        );
    }

    #[test]
    fn page_lists_compress_to_ranges() {
        assert_eq!(super::page_ranges(&[3, 4, 5, 6, 7, 12], 8), "3–7, 12");
        assert_eq!(super::page_ranges(&[1], 8), "1");
        assert_eq!(super::page_ranges(&[1, 3, 5, 7], 2), "1, 3, …");
        assert_eq!(super::page_ranges(&[], 8), "");
    }
}

#[cfg(test)]
mod reveal_tests {
    use gpui::prelude::FluentBuilder as _;
    use gpui::{
        Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
        StatefulInteractiveElement, Styled, TestAppContext, Window, div, px, size,
    };

    struct Panel {
        focus: FocusHandle,
        scroll: ScrollHandle,
        shown: bool,
    }

    impl Render for Panel {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let control = super::reveal_focus(
                div().h(px(20.)).track_focus(&self.focus),
                self.focus.clone(),
                self.scroll.clone(),
            );
            div().size_full().flex().flex_col().child(
                div()
                    .id("panel-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(div().h(px(2000.)))
                    .when(self.shown, |v| v.child(control)),
            )
        }
    }

    #[gpui::test]
    fn focus_is_revealed_on_arrival_only(cx: &mut TestAppContext) {
        let (panel, cx) = cx.add_window_view(|_, cx| Panel {
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            shown: true,
        });
        cx.simulate_resize(size(px(400.), px(300.)));
        let draw = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            cx.run_until_parked();
        };
        let offset =
            |cx: &mut gpui::VisualTestContext| cx.update(|_, cx| panel.read(cx).scroll.offset().y);
        draw(cx);
        cx.update(|window, cx| {
            let focus = panel.read(cx).focus.clone();
            window.focus(&focus, cx);
        });
        draw(cx);
        draw(cx);
        assert!(offset(cx) < px(-1500.), "focus revealed: {:?}", offset(cx));

        // The wheel may move the focused control away.
        cx.simulate_event(gpui::ScrollEvent {
            position: gpui::point(px(200.), px(150.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(800.))),
            ..Default::default()
        });
        draw(cx);
        draw(cx);
        assert!(offset(cx) > px(-1500.), "scrolled away: {:?}", offset(cx));

        // Unmounted and shown again, it counts as arriving.
        panel.update(cx, |panel, cx| {
            panel.shown = false;
            cx.notify();
        });
        draw(cx);
        cx.update(|_, cx| {
            panel
                .read(cx)
                .scroll
                .set_offset(gpui::point(px(0.), px(0.)))
        });
        panel.update(cx, |panel, cx| {
            panel.shown = true;
            cx.notify();
        });
        draw(cx);
        draw(cx);
        assert!(
            offset(cx) < px(-1500.),
            "revealed on return: {:?}",
            offset(cx)
        );
    }
}
