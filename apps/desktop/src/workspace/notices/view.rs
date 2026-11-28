//! The notices' view: the bell's popover with the session notice, the
//! crash report's bar and the update card.

use super::text;
use crate::ui::{self, style};
use crate::workspace::Workspace;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Window, div, px};

impl Workspace {
    /// "mdoc had a problem last time it ran", with the exact report before
    /// anything leaves the Mac. Send and Copy also discard it.
    pub(in crate::workspace) fn crash_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (report, open) = self
            .notices
            .crash()
            .map(|(report, open)| (report.to_owned(), open))?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let accent = style::markdown_style(theme).alert_warning;
        let action = |id: &'static str, label: &'static str| {
            ui::control(id, label, theme, true)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .text_size(px(12.))
        };
        let send_report = report.clone();
        let copy_report = report.clone();
        Some(
            div()
                .id("crash-bar")
                .when(cfg!(test), |v| v.debug_selector(|| "crash-bar".into()))
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_1()
                .text_size(px(12.))
                .bg(gpui::Hsla { a: 0.07, ..accent })
                .border_b_0()
                .border_color(palette.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(palette.header_fg)
                                .child(text::HAD_A_PROBLEM),
                        )
                        .when(!open, |v| {
                            v.child(
                                action("crash-preview", text::SEE_WHAT_WOULD_BE_SENT).on_click(
                                    cx.listener(|this, _, _, cx| this.preview_crash_report(cx)),
                                ),
                            )
                        })
                        .when(open, |v| {
                            v.child(
                                action("crash-send", text::SEND)
                                    .tooltip(style::tooltip(text::SEND_TIP.into(), theme))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.send_crash_report(&send_report, cx)
                                    })),
                            )
                            .child(
                                action("crash-copy", text::COPY).on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.copy_crash_report(copy_report.clone(), cx)
                                    },
                                )),
                            )
                        })
                        .child(
                            action("crash-dismiss", crate::ui::text::DISMISS).on_click(
                                cx.listener(|this, _, _, cx| this.finish_crash_report(cx)),
                            ),
                        ),
                )
                .when(open, |v| {
                    v.child(
                        div()
                            .id("crash-report")
                            .when(cfg!(test), |v| v.debug_selector(|| "crash-report".into()))
                            .max_h(px(220.))
                            .overflow_y_scroll()
                            .p_2()
                            .rounded_md()
                            .bg(palette.bg)
                            .border_1()
                            .border_color(palette.border)
                            .font_family("Menlo")
                            .text_size(px(11.))
                            .text_color(palette.header_muted)
                            .child(report),
                    )
                })
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn notice_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.notices.message().map(str::to_owned).map(|notice| {
            let palette = self.theme.get().pdf_style();
            let accent = style::markdown_style(self.theme.get()).alert_warning;
            div()
                .flex()
                .flex_col()
                .text_size(px(12.))
                .bg(gpui::Hsla { a: 0.07, ..accent })
                .border_b_0()
                .border_color(palette.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_1()
                        .child(div().text_color(accent).child("ⓘ"))
                        .child(div().flex_1().min_w_0().child(notice))
                        .child(
                            crate::ui::focus_ring("dismiss-session-notice", self.theme.get())
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "dismiss-session-notice".into())
                                })
                                .flex_shrink_0()
                                .role(gpui::Role::Button)
                                .aria_label(text::DISMISS_NOTICE)
                                .key_context("UiControl")
                                .tab_index(0)
                                .aria_label(text::DISMISS_NOTICE)
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|v| v.bg(palette.placeholder_bg))
                                .child("×")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.notices.dismiss();
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element()
        })
    }

    /// The notices popover under the title row's right end:
    /// locked Projects, a crash report and session messages, with their actions.
    pub(in crate::workspace) fn notices_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.title.notices_open.get() {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let cards: Vec<AnyElement> = [
            self.project_key_bar(cx),
            self.recovery_nudge_bar(cx),
            self.crash_bar(cx),
            self.notice_bar(cx),
        ]
        .into_iter()
        .flatten()
        .map(|bar| {
            div()
                .rounded_md()
                .overflow_hidden()
                .border_1()
                .border_color(p.border)
                .child(bar)
                .into_any_element()
        })
        .collect();
        if cards.is_empty() {
            return None;
        }
        let right = window.viewport_size().width - px(style::CARD_MARGIN + 8.);
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(gpui::point(
                        right,
                        px(style::CARD_MARGIN + style::HEADER_HEIGHT),
                    ))
                    .anchor(gpui::Anchor::TopRight)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        ui::panel("notices-popover", theme)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "notices-popover".into())
                            })
                            .track_focus(&self.notices_focus)
                            .key_context("UiPanel UiMenu")
                            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                                this.close_notices(window, cx);
                                cx.stop_propagation();
                            }))
                            .w(px(400.))
                            .p_2()
                            .rounded_lg()
                            .shadow_lg()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .on_mouse_down_out(cx.listener(
                                |this, event: &gpui::MouseDownEvent, _, cx| {
                                    // A press on the bell is its toggle's to handle.
                                    if this.title.notices_bell.get().contains(&event.position) {
                                        return;
                                    }
                                    this.title.notices_open.set(false);
                                    cx.notify();
                                },
                            ))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .child(
                                        div()
                                            .flex_1()
                                            .px_1()
                                            .text_size(px(11.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(p.header_muted)
                                            .child(text::NOTICES),
                                    )
                                    .child(
                                        ui::control("close-notices", "×", theme, true)
                                            .when(cfg!(test), |v| {
                                                v.debug_selector(|| "close-notices".into())
                                            })
                                            .aria_label(text::CLOSE_NOTICES)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.close_notices(window, cx);
                                            })),
                                    ),
                            )
                            .children(cards),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// A quiet card while an update is on its way.
    pub(in crate::workspace) fn update_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::app::updater::BannerKind;
        let state = crate::app::updater::state(cx);
        let banner = state.banner()?;
        if banner.kind == BannerKind::Updated {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let accent = theme.search_accent();
        let version = banner.version.clone();
        let (title, detail) = match banner.kind {
            BannerKind::Available => (
                text::UPDATE_AVAILABLE.to_string(),
                text::available_detail(&version),
            ),
            BannerKind::Downloading => (
                text::DOWNLOADING_UPDATE.to_string(),
                text::downloading_detail(&version, state.percent().unwrap_or(0)),
            ),
            BannerKind::Ready => (
                text::INSTALLED_NEXT_LAUNCH.to_string(),
                text::ready_detail(&version),
            ),
            BannerKind::Failed => (
                text::UPDATE_FAILED.to_string(),
                text::failed_detail(&version),
            ),
            BannerKind::Updated => unreachable!(),
        };
        let ready = banner.kind == BannerKind::Ready;
        Some(
            crate::ui::focus_ring("update-banner", theme)
                .when(cfg!(test), |v| v.debug_selector(|| "update-banner".into()))
                .role(gpui::Role::Button)
                .aria_label(text::card_label(&title, &detail))
                .key_context("UiControl")
                .tab_index(0)
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py(px(6.))
                .mb_1()
                .rounded_lg()
                .border_1()
                .border_color(p.border)
                .bg(p.bg)
                .cursor_pointer()
                .hover(move |s| s.border_color(p.header_muted))
                .child(
                    gpui::svg()
                        .data(ui::Icon::Download.data())
                        .size(px(14.))
                        .flex_shrink_0()
                        .text_color(p.header_fg),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(12.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(title),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(11.))
                                .text_color(p.header_muted)
                                .child(detail),
                        ),
                )
                .child(
                    div()
                        .size(px(8.))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(if ready {
                            accent
                        } else {
                            style::markdown_style(theme).alert_warning
                        }),
                )
                .child(
                    div()
                        .id("dismiss-update-banner")
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "dismiss-update-banner".into())
                        })
                        .aria_label(crate::ui::text::DISMISS)
                        .px_1()
                        .rounded_sm()
                        .text_color(p.header_muted)
                        .hover(move |s| s.bg(p.placeholder_bg))
                        .child("×")
                        .on_click(|_, _, cx| {
                            cx.stop_propagation();
                            Self::dismiss_update_card(cx);
                        }),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_update_card(banner.kind, window, cx)
                }))
                .into_any_element(),
        )
    }
}
