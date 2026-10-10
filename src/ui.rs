//! Shared chrome controls. GPUI activates focused click controls with Enter/Space.
use crate::style::Theme;
use gpui::{
    Animation, AnimationExt, AnyElement, App, FocusHandle, KeyBinding, Window, actions, div,
    prelude::*, px,
};
use std::time::Duration;

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

pub fn action_control(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
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
fn outlined(v: gpui::Stateful<gpui::Div>, theme: Theme) -> gpui::Stateful<gpui::Div> {
    let p = theme.pdf_style();
    v.rounded_lg()
        .bg(p.bg)
        .border_1()
        .border_color(p.border)
        .text_color(p.header_fg)
        .font_weight(gpui::FontWeight::MEDIUM)
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

/// A card button without the shared hover/focus styles, which callers set.
fn card_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
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
) -> gpui::Stateful<gpui::Div> {
    let p = theme.pdf_style();
    outlined(card_button(id, label, enabled).px_3(), theme)
        .focus_visible(move |s| s.border_color(p.header_fg))
        .when(enabled, |v| v.hover(move |s| s.bg(p.placeholder_bg)))
}

/// A tertiary text action, such as "Choose model…".
pub fn link_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let p = theme.pdf_style();
    card_button(id, label, enabled)
        .text_color(theme.search_accent())
        .focus_visible(move |s| s.bg(p.placeholder_bg))
        .when(enabled, |v| v.hover(|s| s.underline()))
}

/// A centred card for a pane that has no content yet (ADR 0026). Callers add
/// the title, one sentence, actions and a footnote, in that order.
pub fn state_card(id: impl Into<gpui::ElementId>, theme: Theme) -> gpui::Stateful<gpui::Div> {
    let p = theme.pdf_style();
    div()
        .id(id)
        .w_full()
        .max_w(px(440.))
        .flex()
        .flex_col()
        .gap_2()
        .p_5()
        .rounded_lg()
        .border_1()
        .border_color(p.border)
        .bg(theme.sidebar_bg())
        .text_size(px(13.))
        .text_color(p.header_fg)
        .child(
            gpui::svg()
                .data(include_bytes!("../resources/ui/document.svg"))
                .size(px(24.))
                .mb_1()
                .text_color(p.header_muted),
        )
}
pub fn card_title(text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .text_size(px(16.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(text.into())
}
pub fn card_text(text: impl Into<gpui::SharedString>, theme: Theme) -> gpui::Div {
    div()
        .text_size(px(13.))
        .text_color(theme.pdf_style().header_muted)
        .child(text.into())
}
pub fn card_note(text: impl Into<gpui::SharedString>, theme: Theme) -> gpui::Div {
    div()
        .pt_1()
        .text_size(px(11.))
        .text_color(theme.pdf_style().header_muted)
        .child(text.into())
}
/// A secondary explanation under a card's actions, e.g. language scope.
pub fn card_hint(text: impl Into<gpui::SharedString>, theme: Theme) -> gpui::Div {
    let p = theme.pdf_style();
    div()
        .flex()
        .gap_1()
        .text_size(px(12.))
        .text_color(p.header_muted)
        .child(div().flex_shrink_0().child("ⓘ"))
        .child(div().min_w_0().child(text.into()))
}
pub fn card_error(text: impl Into<gpui::SharedString>, theme: Theme) -> gpui::Div {
    div()
        .text_size(px(12.))
        .text_color(crate::style::markdown_style(theme).alert_caution)
        .child(text.into())
}
pub fn card_actions() -> gpui::Div {
    div().flex().flex_wrap().items_center().gap_2().pt_2()
}

/// Download progress: total bundle bytes while downloading, otherwise the
/// current phase (Verifying, Checking runtime) without a percentage.
pub fn setup_progress(
    id: impl Into<gpui::ElementId>,
    state: &crate::model_download::State,
    cancelling: bool,
    theme: Theme,
) -> gpui::Div {
    let (received, total) = state.overall();
    let label = if cancelling {
        "Cancelling…".to_owned()
    } else if state.downloading() {
        format!(
            "Downloading {}%",
            (received * 100).checked_div(total).unwrap_or(0)
        )
    } else if state.phase.is_empty() {
        "Preparing…".to_owned()
    } else {
        format!("{}…", state.phase.trim_end_matches('…'))
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(activity(id, label, theme).flex_1())
                .when(state.downloading(), |v| {
                    v.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(theme.pdf_style().header_muted)
                            .child(format!(
                                "{} / {} MB",
                                received / 1_000_000,
                                crate::model_download::megabytes(total)
                            )),
                    )
                }),
        )
        .when(state.downloading(), |v| {
            v.child(progress_bar(received, total, theme))
        })
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
    control_base(id, label.clone(), theme, enabled).when(!label.is_empty(), |v| v.child(label))
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
    Undo,
    Redo,
    Rescan,
    Expand,
    Collapse,
    Filter,
    Sidebar,
    Copy,
    Check,
}

impl Icon {
    pub fn data(self) -> &'static [u8] {
        match self {
            Self::Open => include_bytes!("../resources/ui/open.svg"),
            Self::Save => include_bytes!("../resources/ui/save.svg"),
            Self::SaveAs => include_bytes!("../resources/ui/save-as.svg"),
            Self::Sun => include_bytes!("../resources/ui/sun.svg"),
            Self::Moon => include_bytes!("../resources/ui/moon.svg"),
            Self::Settings => include_bytes!("../resources/ui/settings.svg"),
            Self::Anonymous => include_bytes!("../resources/ui/anonymous.svg"),
            Self::Undo => include_bytes!("../resources/ui/undo.svg"),
            Self::Redo => include_bytes!("../resources/ui/redo.svg"),
            Self::Rescan => include_bytes!("../resources/ui/rescan.svg"),
            Self::Expand => include_bytes!("../resources/ui/expand.svg"),
            Self::Collapse => include_bytes!("../resources/ui/collapse.svg"),
            Self::Filter => include_bytes!("../resources/ui/filter.svg"),
            Self::Sidebar => include_bytes!("../resources/ui/sidebar.svg"),
            Self::Copy => include_bytes!("../resources/ui/copy.svg"),
            Self::Check => include_bytes!("../resources/ui/check.svg"),
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
    icon_button(id, label, icon, theme, enabled)
        .when(cfg!(test), |v| v.debug_selector(move || id.into()))
}
/// A 32 px icon control whose label is its tooltip and accessible name.
pub fn icon_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    icon: Icon,
    theme: Theme,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    control_base(id, label.clone(), theme, enabled)
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
        .tooltip(crate::style::tooltip(label.to_string(), theme))
}

/// An outlined icon-and-text control that floats over content; `label` is its
/// tooltip and accessible name, `text` what it shows.
pub fn floating_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    icon: Icon,
    text: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let p = theme.pdf_style();
    let label = label.into();
    outlined(control_base(id, label.clone(), theme, enabled), theme)
        .h(px(32.))
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
        .tooltip(crate::style::tooltip(label.to_string(), theme))
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

#[cfg(test)]
mod tests {
    #[test]
    fn page_lists_compress_to_ranges() {
        assert_eq!(super::page_ranges(&[3, 4, 5, 6, 7, 12], 8), "3–7, 12");
        assert_eq!(super::page_ranges(&[1], 8), "1");
        assert_eq!(super::page_ranges(&[1, 3, 5, 7], 2), "1, 3, …");
        assert_eq!(super::page_ranges(&[], 8), "");
    }
}
