//! The workspace's notices: one session message and the last run's
//! crash report. They wait behind the header's bell with the Vault's
//! banner, which the Vault state owns.
use crate::ui::{self, style};
use crate::workspace::{Workspace, notices};
use gpui::actions;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Window, div, px};

/// What the workspace view carries out for a notices transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NoticesEffect {
    /// Remove the crash report file; it was sent, copied or dismissed.
    DiscardCrashReport,
    /// A new notice arrived: open the bell's popover.
    OpenBell,
    /// Nothing is left: close the bell's popover.
    CloseBell,
}

#[derive(Default)]
pub(super) struct Notices {
    message: Option<String>,
    /// The masked report and whether its preview is open.
    crash: Option<(String, bool)>,
    /// Notices that open the bell, as of the last count.
    opening: usize,
    /// The first-Project nudge to save the recovery key shows.
    recovery_nudge: bool,
}

impl Notices {
    pub(super) fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The crash report and whether its preview is open.
    pub(super) fn crash(&self) -> Option<(&str, bool)> {
        self.crash
            .as_ref()
            .map(|(report, open)| (report.as_str(), *open))
    }

    /// Show `message`, replacing any earlier one.
    pub(super) fn post(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    /// Show `message` and open the bell now, even when the user closed it
    /// over an earlier notice.
    pub(super) fn post_now(&mut self, message: impl Into<String>) -> NoticesEffect {
        self.post(message);
        NoticesEffect::OpenBell
    }

    /// Open the bell for the Vault's banner, which the Vault state holds:
    /// the locked Vault is needed now.
    pub(super) fn show_locked_projects(&self) -> NoticesEffect {
        NoticesEffect::OpenBell
    }

    /// Whether the first-Project nudge shows, which the window decides.
    pub(super) fn set_recovery_nudge(&mut self, shows: bool) {
        self.recovery_nudge = shows;
    }

    pub(super) fn dismiss(&mut self) {
        self.message = None;
    }

    /// Dismiss `message` if it is still the one shown: it is no longer
    /// true. A later message stays.
    pub(super) fn withdraw(&mut self, message: &str) {
        if self.message.as_deref() == Some(message) {
            self.message = None;
        }
    }

    /// The last run's report, read at launch.
    pub(super) fn load_crash(&mut self, report: Option<String>) {
        self.crash = report.map(|report| (report, false));
    }

    /// "See what would be sent": the report's text opens below the bar.
    pub(super) fn preview_crash(&mut self) {
        if let Some((_, open)) = &mut self.crash {
            *open = true;
        }
    }

    /// Send, Copy and Dismiss all end the report.
    pub(super) fn finish_crash(&mut self) -> Option<NoticesEffect> {
        self.crash.take().map(|_| NoticesEffect::DiscardCrashReport)
    }

    /// The bell's count with the Vault's banner. A new notice opens the
    /// popover, and the banner opens it only when `needed`; no notices close
    /// it.
    pub(super) fn count(
        &mut self,
        project_banner: bool,
        project_needed: bool,
    ) -> (usize, Option<NoticesEffect>) {
        let own = usize::from(self.crash.is_some()) + usize::from(self.message.is_some());
        // The recovery-key nudge only counts on the bell: opened by itself,
        // the popover covered the new Project's header, on every launch
        // until answered (#1286).
        let shown = own + usize::from(self.recovery_nudge) + usize::from(project_banner);
        let opening = own + usize::from(project_needed);
        let effect = if opening > self.opening {
            Some(NoticesEffect::OpenBell)
        } else if shown == 0 {
            Some(NoticesEffect::CloseBell)
        } else {
            None
        };
        self.opening = opening;
        (shown, effect)
    }
}

impl Workspace {
    /// The launch-time update check, unless the user turned it off.
    pub(crate) fn check_for_updates_at_launch(&self, cx: &mut Context<Self>) {
        let updates = self
            .preferences
            .borrow()
            .snapshot()
            .unwrap_or_default()
            .updates;
        if updates.check_automatically {
            crate::app::updater::spawn_check(updates.include_prereleases, true, cx);
        }
    }

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
        let done = |this: &mut Self, cx: &mut Context<Self>| {
            if let Some(effect) = this.notices.finish_crash() {
                this.carry_out_notices(effect);
            }
            cx.notify();
        };
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
                                .child("mdoc had a problem last time it ran."),
                        )
                        .when(!open, |v| {
                            v.child(action("crash-preview", "See what would be sent").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.notices.preview_crash();
                                    cx.notify();
                                }),
                            ))
                        })
                        .when(open, |v| {
                            v.child(
                                action("crash-send", "Send…")
                                    .tooltip(style::tooltip(
                                        "Opens a new GitHub issue with this report for you to check and submit."
                                            .into(),
                                        theme,
                                    ))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.open_url(&crate::app::crash::issue_url(&send_report));
                                        done(this, cx);
                                    })),
                            )
                            .child(action("crash-copy", "Copy").on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        copy_report.clone(),
                                    ));
                                    done(this, cx);
                                },
                            )))
                        })
                        .child(
                            action("crash-dismiss", "Dismiss")
                                .on_click(cx.listener(move |this, _, _, cx| done(this, cx))),
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

    /// Restart now: the ordinary quit — unsaved-change prompts,
    /// session save — swaps the staged update in, then mdoc relaunches.
    pub(crate) fn restart_to_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::app::updater::request_restart(cx);
        self.settings
            .update(cx, |panel, cx| panel.close(window, cx));
        self.quit(window, cx);
        // Quitting can't start now (a prompt is open): don't relaunch later.
        if self.quit_flow.may_quit() {
            crate::app::updater::cancel_restart(cx);
        }
    }

    /// Carry out what a notices transition asks for.
    pub(in crate::workspace) fn carry_out_notices(&mut self, effect: notices::NoticesEffect) {
        match effect {
            notices::NoticesEffect::DiscardCrashReport => crate::app::crash::discard(),
            notices::NoticesEffect::OpenBell => self.title.notices_open.set(true),
            notices::NoticesEffect::CloseBell => self.title.notices_open.set(false),
        }
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
                                .aria_label("Dismiss notice")
                                .key_context("UiControl")
                                .tab_index(0)
                                .aria_label("Dismiss notice")
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
}

actions!(mdoc_shell, [CloseNotices]);

impl Workspace {
    /// The notices popover under the title row's right end:
    /// locked Projects, a crash report and session messages, with their actions.
    pub(super) fn notices_view(
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
                                            .child("NOTICES"),
                                    )
                                    .child(
                                        ui::control("close-notices", "×", theme, true)
                                            .when(cfg!(test), |v| {
                                                v.debug_selector(|| "close-notices".into())
                                            })
                                            .aria_label("Close notices")
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
    /// Close the notices popover; focus returns to the document, or to the
    /// window on Home.
    pub(super) fn close_notices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.title.notices_open.set(false);
        if self.showing_overview(cx) {
            window.focus(&self.focus, cx);
        } else {
            self.focus_active(window, cx);
        }
        cx.notify();
    }
    /// A quiet card while an update is on its way.
    pub(super) fn update_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
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
            BannerKind::Available => ("Update available".to_string(), format!("v{version}")),
            BannerKind::Downloading => (
                "Downloading update".to_string(),
                format!("v{version} · {}%", state.percent().unwrap_or(0)),
            ),
            BannerKind::Ready => (
                "Installed, next launch".to_string(),
                format!("v{version} · relaunch"),
            ),
            BannerKind::Failed => ("Update failed".to_string(), format!("v{version} · details")),
            BannerKind::Updated => unreachable!(),
        };
        let ready = banner.kind == BannerKind::Ready;
        Some(
            crate::ui::focus_ring("update-banner", theme)
                .when(cfg!(test), |v| v.debug_selector(|| "update-banner".into()))
                .role(gpui::Role::Button)
                .aria_label(format!("{title}. {detail}"))
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
                        .aria_label("Dismiss")
                        .px_1()
                        .rounded_sm()
                        .text_color(p.header_muted)
                        .hover(move |s| s.bg(p.placeholder_bg))
                        .child("×")
                        .on_click(|_, _, cx| {
                            cx.stop_propagation();
                            crate::app::updater::dismiss_banner(cx);
                        }),
                )
                .on_click(cx.listener(move |this, _, window, cx| match banner.kind {
                    BannerKind::Ready => this.restart_to_update(window, cx),
                    _ => {
                        window.activate_window();
                        this.settings
                            .update(cx, |panel, cx| panel.show_updates(window, cx));
                    }
                }))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_notice_opens_the_bell_once_and_none_closes_it() {
        let mut notices = Notices::default();
        assert_eq!(
            notices.count(false, false),
            (0, Some(NoticesEffect::CloseBell))
        );
        notices.post("Could not save session: disk full");
        assert_eq!(
            notices.count(false, false),
            (1, Some(NoticesEffect::OpenBell))
        );
        // Unchanged: the user may have closed the popover; it stays closed.
        assert_eq!(notices.count(false, false), (1, None));
        // A replacement message is not a new notice.
        notices.post("The clipboard has no text to paste.");
        assert_eq!(
            notices.message(),
            Some("The clipboard has no text to paste.")
        );
        assert_eq!(notices.count(false, false), (1, None));
        notices.dismiss();
        assert_eq!(
            notices.count(false, false),
            (0, Some(NoticesEffect::CloseBell))
        );
    }

    #[test]
    fn a_withdrawn_notice_goes_only_while_it_is_the_one_shown() {
        let mut notices = Notices::default();
        notices.post("Documents can't be added yet.");
        notices.withdraw("Something else.");
        assert_eq!(notices.message(), Some("Documents can't be added yet."));
        notices.withdraw("Documents can't be added yet.");
        assert_eq!(notices.message(), None);
        notices.post("Exported 3 documents.");
        notices.withdraw("Documents can't be added yet.");
        assert_eq!(notices.message(), Some("Exported 3 documents."));
    }

    #[test]
    fn an_urgent_notice_reopens_the_bell_at_an_unchanged_count() {
        let mut notices = Notices::default();
        notices.post("Could not restore tabs");
        assert_eq!(
            notices.count(false, false),
            (1, Some(NoticesEffect::OpenBell))
        );
        // The user closed the popover; the count alone would keep it closed.
        assert_eq!(
            notices.post_now("Copy for AI works from a pseudonymized document."),
            NoticesEffect::OpenBell
        );
        assert_eq!(notices.count(false, false), (1, None));
        assert_eq!(notices.show_locked_projects(), NoticesEffect::OpenBell);
    }

    #[test]
    fn locked_projects_count_but_open_the_bell_only_when_needed() {
        let mut notices = Notices::default();
        assert_eq!(notices.count(true, false), (1, None));
        assert_eq!(
            notices.count(true, true),
            (1, Some(NoticesEffect::OpenBell))
        );
        notices.post("Saved elsewhere");
        assert_eq!(
            notices.count(true, true),
            (2, Some(NoticesEffect::OpenBell))
        );
    }

    #[test]
    fn a_crash_report_previews_then_ends_with_one_discard() {
        let mut notices = Notices::default();
        assert_eq!(notices.finish_crash(), None);
        notices.load_crash(Some("mdoc 0.2.0 — macos aarch64".into()));
        assert_eq!(notices.crash(), Some(("mdoc 0.2.0 — macos aarch64", false)));
        notices.preview_crash();
        assert_eq!(notices.crash(), Some(("mdoc 0.2.0 — macos aarch64", true)));
        assert_eq!(
            notices.count(false, false),
            (1, Some(NoticesEffect::OpenBell))
        );
        assert_eq!(
            notices.finish_crash(),
            Some(NoticesEffect::DiscardCrashReport)
        );
        assert_eq!(notices.crash(), None);
        assert_eq!(notices.finish_crash(), None);
        notices.load_crash(None);
        assert_eq!(notices.crash(), None);
    }

    #[test]
    fn the_recovery_nudge_counts_until_it_goes_and_never_opens_the_bell() {
        let mut notices = Notices::default();
        notices.set_recovery_nudge(true);
        // Counted on the bell's badge, the popover left as it is (#1286).
        assert_eq!(notices.count(false, false), (1, None));
        assert_eq!(notices.count(false, false), (1, None));
        notices.post("Exported 3 documents.");
        assert_eq!(
            notices.count(false, false),
            (2, Some(NoticesEffect::OpenBell))
        );
        notices.dismiss();
        notices.set_recovery_nudge(false);
        assert_eq!(
            notices.count(false, false),
            (0, Some(NoticesEffect::CloseBell))
        );
    }
}
