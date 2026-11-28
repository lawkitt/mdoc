//! The notices' state: one session message, the last run's crash report
//! and the recovery-key nudge, with the bell's count and the effects each
//! change asks of the window. No GPUI.

/// What the workspace view carries out for a notices transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum NoticesEffect {
    /// Remove the crash report file; it was sent, copied or dismissed.
    DiscardCrashReport,
    /// A new notice arrived: open the bell's popover.
    OpenBell,
    /// Nothing is left: close the bell's popover.
    CloseBell,
}

#[derive(Default)]
pub(in crate::workspace) struct Notices {
    message: Option<String>,
    /// The masked report and whether its preview is open.
    crash: Option<(String, bool)>,
    /// Notices that open the bell, as of the last count.
    opening: usize,
    /// The first-Project nudge to save the recovery key shows.
    recovery_nudge: bool,
}

impl Notices {
    pub(in crate::workspace) fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The crash report and whether its preview is open.
    pub(in crate::workspace) fn crash(&self) -> Option<(&str, bool)> {
        self.crash
            .as_ref()
            .map(|(report, open)| (report.as_str(), *open))
    }

    /// Show `message`, replacing any earlier one.
    pub(in crate::workspace) fn post(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    /// Show `message` and open the bell now, even when the user closed it
    /// over an earlier notice.
    pub(in crate::workspace) fn post_now(&mut self, message: impl Into<String>) -> NoticesEffect {
        self.post(message);
        NoticesEffect::OpenBell
    }

    /// Open the bell for the Vault's banner, which the Vault state holds:
    /// the locked Vault is needed now.
    pub(in crate::workspace) fn show_locked_projects(&self) -> NoticesEffect {
        NoticesEffect::OpenBell
    }

    /// Whether the first-Project nudge shows, which the window decides.
    pub(in crate::workspace) fn set_recovery_nudge(&mut self, shows: bool) {
        self.recovery_nudge = shows;
    }

    pub(in crate::workspace) fn dismiss(&mut self) {
        self.message = None;
    }

    /// Dismiss `message` if it is still the one shown: it is no longer
    /// true. A later message stays.
    pub(in crate::workspace) fn withdraw(&mut self, message: &str) {
        if self.message.as_deref() == Some(message) {
            self.message = None;
        }
    }

    /// The last run's report, read at launch.
    pub(in crate::workspace) fn load_crash(&mut self, report: Option<String>) {
        self.crash = report.map(|report| (report, false));
    }

    /// "See what would be sent": the report's text opens below the bar.
    pub(in crate::workspace) fn preview_crash(&mut self) {
        if let Some((_, open)) = &mut self.crash {
            *open = true;
        }
    }

    /// Send, Copy and Dismiss all end the report.
    pub(in crate::workspace) fn finish_crash(&mut self) -> Option<NoticesEffect> {
        self.crash.take().map(|_| NoticesEffect::DiscardCrashReport)
    }

    /// The bell's count with the Vault's banner. A new notice opens the
    /// popover, and the banner opens it only when `needed`; no notices close
    /// it.
    pub(in crate::workspace) fn count(
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
