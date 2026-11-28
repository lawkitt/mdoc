//! The notices' actions: carrying out a notices effect, closing the
//! popover, and checking for and restarting into an update.

use crate::app::updater::BannerKind;
use crate::workspace::{Workspace, notices};
use gpui::{App, Context, Window};

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

    /// The crash bar's "See what would be sent": show the report.
    pub(in crate::workspace) fn preview_crash_report(&mut self, cx: &mut Context<Self>) {
        self.notices.preview_crash();
        cx.notify();
    }

    /// The crash bar's Send: open the issue form with the report, then
    /// discard it.
    pub(in crate::workspace) fn send_crash_report(&mut self, report: &str, cx: &mut Context<Self>) {
        cx.open_url(&crate::app::crash::issue_url(report));
        self.finish_crash_report(cx);
    }

    /// The crash bar's Copy: put the report on the clipboard, then discard it.
    pub(in crate::workspace) fn copy_crash_report(
        &mut self,
        report: String,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(report));
        self.finish_crash_report(cx);
    }

    /// Send, Copy and Dismiss end the crash report the same way.
    pub(in crate::workspace) fn finish_crash_report(&mut self, cx: &mut Context<Self>) {
        if let Some(effect) = self.notices.finish_crash() {
            self.carry_out_notices(effect);
        }
        cx.notify();
    }

    /// A click on the update card: restart into a staged update, or show
    /// the updates in Settings.
    pub(in crate::workspace) fn open_update_card(
        &mut self,
        kind: BannerKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match kind {
            BannerKind::Ready => self.restart_to_update(window, cx),
            _ => {
                window.activate_window();
                self.settings
                    .update(cx, |panel, cx| panel.show_updates(window, cx));
            }
        }
    }

    /// The update card's ×.
    pub(in crate::workspace) fn dismiss_update_card(cx: &mut App) {
        crate::app::updater::dismiss_banner(cx);
    }

    /// Carry out what a notices transition asks for.
    pub(in crate::workspace) fn carry_out_notices(&mut self, effect: notices::NoticesEffect) {
        match effect {
            notices::NoticesEffect::DiscardCrashReport => crate::app::crash::discard(),
            notices::NoticesEffect::OpenBell => self.title.notices_open.set(true),
            notices::NoticesEffect::CloseBell => self.title.notices_open.set(false),
        }
    }

    /// Close the notices popover; focus returns to the document, or to the
    /// window on Home.
    pub(in crate::workspace) fn close_notices(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.title.notices_open.set(false);
        if self.showing_overview(cx) {
            window.focus(&self.focus, cx);
        } else {
            self.focus_active(window, cx);
        }
        cx.notify();
    }
}
