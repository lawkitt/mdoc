//! The workspace's notices (ADR 0048): one session message and the last run's
//! crash report (ADR 0047). They wait behind the header's bell with the
//! locked-cases banner, which the case state owns (ADR 0043).

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

    /// Open the bell for the locked-cases banner, which the case state
    /// holds: a locked case is needed now (ADR 0043).
    pub(super) fn show_locked_cases(&self) -> NoticesEffect {
        NoticesEffect::OpenBell
    }

    pub(super) fn dismiss(&mut self) {
        self.message = None;
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

    /// The bell's count with the locked-cases banner. A new notice opens the
    /// popover, except locked cases, which open it only when a locked case is
    /// needed (ADR 0043); no notices close it.
    pub(super) fn count(
        &mut self,
        case_banner: bool,
        case_needed: bool,
    ) -> (usize, Option<NoticesEffect>) {
        let own = usize::from(self.crash.is_some()) + usize::from(self.message.is_some());
        let shown = own + usize::from(case_banner);
        let opening = own + usize::from(case_needed);
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
        assert_eq!(notices.show_locked_cases(), NoticesEffect::OpenBell);
    }

    #[test]
    fn locked_cases_count_but_open_the_bell_only_when_needed() {
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
}
