//! Shared chrome controls. GPUI activates focused click controls with Enter/Space.
use crate::style::Theme;
use gpui::{
    Animation, AnimationExt, AnyElement, App, FocusHandle, KeyBinding, Window, actions, div,
    prelude::*, px,
};
use std::time::Duration;

pub use gpui_pdf::FocusRing as Control;

/// The one keyboard focus indicator (ADR 0074).
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

/// A counter or status line for screen readers (ADR 0064): a Label node whose
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
    format!("{noun} {position} of {total}")
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

/// The header's one filled button (ADR 0048): ink on paper inverted, so it is
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
        .tooltip(crate::style::tooltip(label.to_string(), theme))
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
/// hear the same reason as the row's description (ADR 0064).
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
            v.aria_description(format!("Unavailable. {reason}"))
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
/// Static model provenance next to the download size. It adds no Tab stop;
/// screen readers can read its named group and each prompt's existing actions.
pub fn model_attribution(
    id: impl Into<gpui::ElementId>,
    model: mdoc_store::settings::Model,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    let text = crate::models::attribution_label(model);
    div()
        .id(id)
        .role(gpui::Role::Group)
        .aria_label(format!("{}. {text}", model.name()))
        .min_w_0()
        .text_size(px(11.))
        .text_color(theme.pdf_style().header_muted)
        .child(text.clone())
        .when(cfg!(test), |v| {
            v.debug_selector(move || format!("{}: {text}", model.name()))
        })
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
    state: &mdoc_import::model_download::State,
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
                                mdoc_import::model_download::megabytes(total)
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
        SidebarCloseTab,
        SidebarMenu,
    ]
);

/// Tab inside a control group here; keyboard reach (ADR 0062), menus and
/// the sidebar's keys from the shortcuts table (ADR 0073).
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", NextControl, Some("UiControl || UiPanel")),
        KeyBinding::new("shift-tab", PreviousControl, Some("UiControl || UiPanel")),
    ]);
    cx.bind_keys(crate::shortcuts::bindings(crate::shortcuts::Owner::Ui));
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

/// A close button matching the Original viewer's (ADR 0044).
pub fn close_control(id: impl Into<gpui::ElementId>, theme: Theme) -> Control {
    gpui_pdf::close_control(id, &theme.pdf_style())
        .key_context("UiControl")
        .tab_stop(true)
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

/// The header's bell while workspace notices wait (ADR 0048): an amber dot
/// with the count; a click opens or closes the notices popover.
pub fn notice_bell(title: &crate::style::TitleBar, theme: Theme) -> Option<AnyElement> {
    let count = title.notices.get();
    if count == 0 {
        return None;
    }
    let open = title.notices_open.get();
    let amber = crate::style::markdown_style(theme).alert_warning;
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
                header_icon(
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

/// The content card's surface (ADR 0048): rounded, on the window ground.
pub fn card_surface<E: Styled>(surface: E, theme: Theme) -> E {
    surface
        .rounded_lg()
        .overflow_hidden()
        .bg(theme.pdf_style().bg)
        .border_1()
        .border_color(theme.card_border())
        .shadow_sm()
}

/// Make `area` move the window like a title bar (ADR 0048): a press that
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

/// Drawn window buttons for Windows and client-decorated Linux (ADR 0048);
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
            .h(px(crate::style::HEADER_HEIGHT))
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
        cx.simulate_event(gpui::ScrollWheelEvent {
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
