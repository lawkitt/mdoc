//! The Keychain explainer (ADR 0043): before mdoc reads the case key in a way
//! that may show the macOS password dialog, say why, and preview that dialog
//! with a short, captioned animation below the text so the real one is
//! expected, not suspicious, and the drawing is never taken for it.
use super::*;
use gpui::{Animation, AnimationExt, Hsla};
use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

/// One loop of the preview: type the password, then press Always Allow.
const LOOP: Duration = Duration::from_millis(4200);
/// The frame shown under Reduce Motion: password typed, Always Allow pressed.
const STILL: f32 = 0.8;
/// Preview size; positions below are relative to its top-left corner.
const WIDTH: f32 = 384.;
const HEIGHT: f32 = 168.;
/// Where the text column (and Always Allow) starts, after the padlock.
const TEXT_X: f32 = 80.;
const ALWAYS_X: f32 = TEXT_X;
const ALWAYS_W: f32 = 92.;
const BUTTON_Y: f32 = HEIGHT - 16. - 22.;
const DOTS: usize = 8;

static BADGE: LazyLock<Arc<gpui::Image>> = LazyLock::new(|| {
    Arc::new(gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        include_bytes!("../resources/ui/app-badge.png").to_vec(),
    ))
});

/// The macOS alert's own colours, light or dark like the system.
struct Alert {
    bg: Hsla,
    border: Hsla,
    text: Hsla,
    secondary: Hsla,
    field: Hsla,
    field_border: Hsla,
    button: Hsla,
    button_pressed: Hsla,
    button_text: Hsla,
    default_button: Hsla,
    ring: Hsla,
}
impl Alert {
    fn new(theme: Theme) -> Self {
        let blue = gpui::rgb(0x2f7ff5).into();
        match theme {
            Theme::Dark => Self {
                bg: gpui::rgb(0x3a3a3c).into(),
                border: gpui::rgb(0x5a5a5c).into(),
                text: gpui::rgb(0xf5f5f7).into(),
                secondary: gpui::rgb(0xd2d2d6).into(),
                field: gpui::rgb(0x58585b).into(),
                field_border: gpui::rgb(0x6c6c70).into(),
                button: gpui::rgb(0x707074).into(),
                button_pressed: gpui::rgb(0x8e8e93).into(),
                button_text: gpui::rgb(0xffffff).into(),
                default_button: blue,
                ring: Hsla { a: 0.75, ..blue },
            },
            Theme::Light => Self {
                bg: gpui::rgb(0xececec).into(),
                border: gpui::rgb(0xc4c4c6).into(),
                text: gpui::rgb(0x1d1d1f).into(),
                secondary: gpui::rgb(0x48484a).into(),
                field: gpui::rgb(0xffffff).into(),
                field_border: gpui::rgb(0xc0c0c4).into(),
                button: gpui::rgb(0xffffff).into(),
                button_pressed: gpui::rgb(0xd8d8dc).into(),
                button_text: gpui::rgb(0x1d1d1f).into(),
                default_button: blue,
                ring: Hsla { a: 0.6, ..blue },
            },
        }
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
/// Progress of `t` through the window `from..to`, clamped to 0..1.
fn phase(t: f32, from: f32, to: f32) -> f32 {
    ((t - from) / (to - from)).clamp(0., 1.)
}

/// One frame of the dialog at loop position `t` (0..1).
fn preview_frame(theme: Theme, t: f32) -> gpui::Div {
    let c = Alert::new(theme);
    let dots = (phase(t, 0.08, 0.4) * DOTS as f32).round() as usize;
    // The caret blinks while the field waits, and stays while typing.
    let caret = dots == DOTS || (t * 8.).fract() < 0.6 || (0.08..0.4).contains(&t);
    let travel = ease(phase(t, 0.48, 0.7));
    let pressed = (0.72..0.86).contains(&t);
    let pointer = if t < 0.44 {
        0.
    } else {
        phase(t, 0.44, 0.48) * (1. - phase(t, 0.94, 1.))
    };
    let (start_x, start_y) = (WIDTH - 90., 92.);
    let (end_x, end_y) = (ALWAYS_X + ALWAYS_W * 0.55, BUTTON_Y + 10.);
    let pointer_x = lerp(start_x, end_x, travel);
    let pointer_y = lerp(start_y, end_y, travel) + if pressed { 1. } else { 0. };
    let button = |label: &'static str, width: f32, bg: Hsla, color: Hsla| {
        div()
            .w(px(width))
            .h(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .bg(bg)
            .text_color(color)
            .text_size(px(12.))
            .child(label)
    };
    let lock = div()
        .relative()
        .flex_shrink_0()
        .size(px(52.))
        .child(
            gpui::svg()
                .data(include_bytes!("../resources/ui/lock-shackle.svg"))
                .absolute()
                .size(px(52.))
                .text_color(gpui::rgb(0xc7c9cc)),
        )
        .child(
            gpui::svg()
                .data(include_bytes!("../resources/ui/lock-body.svg"))
                .absolute()
                .size(px(52.))
                .text_color(gpui::rgb(0xe3a63c)),
        )
        .child(
            gpui::svg()
                .data(include_bytes!("../resources/ui/lock-ridges.svg"))
                .absolute()
                .size(px(52.))
                .text_color(gpui::rgba(0xb07a1e66)),
        )
        .child(
            gpui::img(BADGE.clone())
                .absolute()
                .right(px(-4.))
                .bottom(px(-3.))
                .size(px(26.)),
        );
    div()
        .relative()
        .w(px(WIDTH))
        .h(px(HEIGHT))
        .flex_shrink_0()
        .rounded(px(10.))
        .bg(c.bg)
        .border_1()
        .border_color(c.border)
        .text_color(c.text)
        .child(
            div()
                .absolute()
                .left(px(16.))
                .top(px(16.))
                .right(px(16.))
                .flex()
                .gap(px(12.))
                .child(lock)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(5.))
                        .child(
                            div()
                                .text_size(px(12.))
                                .line_height(px(15.))
                                .font_weight(gpui::FontWeight::BOLD)
                                .child(
                                    "mdoc wants to use your confidential information stored \
                                     in “io.github.lawkitt.mdoc” in your keychain.",
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.5)) // type-ramp: illustration
                                .text_color(c.secondary)
                                .child("To allow this, enter the “login” keychain password."),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .pt(px(2.))
                                .text_size(px(11.))
                                .child("Password:")
                                .child(
                                    div()
                                        .flex_1()
                                        .h(px(20.))
                                        .flex()
                                        .items_center()
                                        .px(px(5.))
                                        .rounded(px(3.))
                                        .bg(c.field)
                                        .border_1()
                                        .border_color(c.field_border)
                                        // The focus ring of the active field.
                                        .shadow(vec![gpui::BoxShadow {
                                            color: c.ring,
                                            offset: gpui::point(px(0.), px(0.)),
                                            blur_radius: px(0.),
                                            spread_radius: px(2.5),
                                            inset: false,
                                        }])
                                        .text_size(px(9.)) // type-ramp: illustration
                                        .child("●".repeat(dots))
                                        .when(caret, |v| {
                                            v.child(
                                                div()
                                                    .ml(px(1.))
                                                    .w(px(1.))
                                                    .h(px(12.))
                                                    .bg(c.default_button),
                                            )
                                        }),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .absolute()
                .left(px(16.))
                .top(px(BUTTON_Y))
                .size(px(20.))
                .rounded_full()
                .bg(c.button)
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(c.button_text)
                .child("?"),
        )
        .child(
            div()
                .absolute()
                .left(px(ALWAYS_X))
                .top(px(BUTTON_Y))
                .child(button(
                    "Always Allow",
                    ALWAYS_W,
                    if pressed { c.button_pressed } else { c.button },
                    c.button_text,
                )),
        )
        .child(
            div()
                .absolute()
                .right(px(16.))
                .top(px(BUTTON_Y))
                .flex()
                .gap(px(10.))
                .child(button("Deny", 72., c.button, c.button_text))
                .child(button("Allow", 72., c.default_button, gpui::white())),
        )
        .when(pointer > 0., |v| {
            let size = if pressed { 0.92 } else { 1. };
            v.child(
                div()
                    .absolute()
                    .left(px(pointer_x))
                    .top(px(pointer_y))
                    .opacity(pointer)
                    .child(
                        gpui::svg()
                            .data(include_bytes!("../resources/ui/pointer-outline.svg"))
                            .absolute()
                            .w(px(13. * size))
                            .h(px(19.5 * size))
                            .text_color(gpui::white()),
                    )
                    .child(
                        gpui::svg()
                            .data(include_bytes!("../resources/ui/pointer.svg"))
                            .absolute()
                            .w(px(13. * size))
                            .h(px(19.5 * size))
                            .text_color(gpui::black()),
                    ),
            )
        })
}

/// The animated preview; a still frame under Reduce Motion.
fn preview(theme: Theme, cx: &App) -> AnyElement {
    if cx.reduce_motion() {
        return preview_frame(theme, STILL).into_any_element();
    }
    div()
        .with_animation(
            "keychain-preview",
            Animation::new(LOOP).repeat().with_max_fps(30.),
            move |el, t| el.child(preview_frame(theme, t)),
        )
        .into_any_element()
}

impl Workspace {
    /// The explainer dialog, shown before a case action reads the key.
    pub(super) fn keychain_explainer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.cases.explainer {
            return None;
        }
        if std::mem::take(&mut self.explainer_focus_pending) {
            // Keep the first return point when Try again reopens it.
            if !self.explainer_group.contains_focused(window, cx) {
                self.explainer_return = window.focused(cx);
            }
            window.focus(&self.explainer_focus, cx);
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let line = |text: &'static str| {
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(palette.header_muted)
                        .child("•"),
                )
                .child(div().min_w_0().child(text))
        };
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_start()
                    .justify_center()
                    .pt(px(64.))
                    .bg(gpui::hsla(0., 0., 0., 0.25))
                    // A modal, like Settings: nothing behind it takes clicks.
                    .id("keychain-overlay")
                    .occlude()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(gpui::MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                    .child(
                        ui::panel("keychain-explainer", theme)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "keychain-explainer".into())
                            })
                            .role(gpui::Role::Dialog)
                            .aria_label("mdoc will ask for Keychain access")
                            .w(px(440.))
                            .p_5()
                            .flex()
                            .flex_col()
                            .gap_3()
                            // A modal: window shortcuts (close tab, open, new
                            // tab…) stop here, so the waiting action finds the
                            // state it was asked for. Quit still works.
                            .on_action(|_: &Close, _, _| {})
                            .on_action(|_: &New, _, _| {})
                            .on_action(|_: &Open, _, _| {})
                            .on_action(|_: &Import, _, _| {})
                            .on_action(|_: &OpenCase, _, _| {})
                            .on_action(|_: &Settings, _, _| {})
                            .on_action(|_: &crate::menus::About, _, _| {})
                            .on_action(|_: &NextTab, _, _| {})
                            .on_action(|_: &PreviousTab, _, _| {})
                            .on_action(|_: &ToggleSidebar, _, _| {})
                            .on_action(|_: &RetryDocument, _, _| {})
                            // Tab cycles only its own buttons.
                            .key_context("UiPanel")
                            .track_focus(&self.explainer_group)
                            .tab_group()
                            .tab_stop(false)
                            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                                ui::cycle(window, cx, Some(&this.explainer_group), false);
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                                ui::cycle(window, cx, Some(&this.explainer_group), true);
                                cx.stop_propagation();
                            }))
                            .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, window, cx| {
                                if e.keystroke.key == "escape" {
                                    this.cancel_unlock(window, cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .child(ui::card_title("mdoc will ask for Keychain access"))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .text_size(px(13.))
                                    .child(line(
                                        "Saved cases are encrypted, and the key lives in your \
                                         Mac’s Keychain.",
                                    ))
                                    .child(line(
                                        "macOS will ask for your Mac login password. mdoc never \
                                         sees it.",
                                    ))
                                    .child(line("Choose Always Allow so it isn’t asked again.")),
                            )
                            // Below the text and captioned, so the drawing
                            // reads as what comes next, never as the system
                            // dialog itself.
                            .child(
                                div()
                                    .id("keychain-preview-figure")
                                    .role(gpui::Role::Image)
                                    .aria_label("Preview of the macOS Keychain dialog")
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap_2()
                                    .pt_2()
                                    .pb_4()
                                    .rounded_md()
                                    .bg(palette.placeholder_bg)
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(palette.header_muted)
                                            .child("After Continue, macOS shows this:"),
                                    )
                                    .child(preview(theme, cx)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap_2()
                                    .pt_1()
                                    .child(
                                        ui::action_control(
                                            "keychain-not-now",
                                            "Not now",
                                            theme,
                                            true,
                                            false,
                                        )
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "keychain-not-now".into())
                                        })
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.cancel_unlock(window, cx)
                                            }),
                                        ),
                                    )
                                    .child(
                                        ui::primary_button(
                                            "keychain-continue",
                                            "Continue",
                                            theme,
                                            true,
                                        )
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "keychain-continue".into())
                                        })
                                        .track_focus(&self.explainer_focus)
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.continue_unlock(window, cx)
                                            }),
                                        ),
                                    ),
                            ),
                    ),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }

    /// Locked or declined cases: one line with Unlock / Try again.
    pub(super) fn case_key_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (text, action) = self.case_key_banner()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let accent = style::markdown_style(theme).alert_warning;
        Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .text_size(px(12.))
                .bg(gpui::Hsla { a: 0.07, ..accent })
                .border_b_0()
                .border_color(palette.border)
                .child(div().text_color(accent).child("ⓘ"))
                .child(div().flex_1().min_w_0().child(text))
                .child(
                    ui::control("unlock-cases", action, theme, true)
                        .when(cfg!(test), |v| v.debug_selector(|| "unlock-cases".into()))
                        .on_click(cx.listener(|this, _, window, cx| this.unlock_cases(window, cx))),
                )
                .into_any_element(),
        )
    }
}
