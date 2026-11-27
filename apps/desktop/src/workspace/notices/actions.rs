//! The notices' actions: carrying out a notices effect, closing the
//! popover, and checking for and restarting into an update.

use crate::workspace::{Workspace, notices};
use gpui::{Context, Window};

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
