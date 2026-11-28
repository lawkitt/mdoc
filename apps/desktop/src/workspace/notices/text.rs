//! The notices' words: each string a user reads in the notices, the crash
//! report's bar and the update card is a constant or a function here.

/// The crash report's bar.
pub(crate) const HAD_A_PROBLEM: &str = "mdoc had a problem last time it ran.";
pub(crate) const SEE_WHAT_WOULD_BE_SENT: &str = "See what would be sent";
pub(crate) const SEND: &str = "Send…";
/// The Send button's tooltip.
pub(crate) const SEND_TIP: &str =
    "Opens a new GitHub issue with this report for you to check and submit.";
pub(crate) const COPY: &str = "Copy";

/// The session notice's dismiss button, as a screen reader names it.
pub(crate) const DISMISS_NOTICE: &str = "Dismiss notice";

/// The notices popover's heading.
pub(crate) const NOTICES: &str = "NOTICES";
/// Its close button, as a screen reader names it.
pub(crate) const CLOSE_NOTICES: &str = "Close notices";

/// The update card's title and detail, by the update's state.
pub(crate) const UPDATE_AVAILABLE: &str = "Update available";
pub(crate) fn available_detail(version: impl std::fmt::Display) -> String {
    format!("v{version}")
}
pub(crate) const DOWNLOADING_UPDATE: &str = "Downloading update";
pub(crate) fn downloading_detail(version: impl std::fmt::Display, percent: u8) -> String {
    format!("v{version} · {percent}%")
}
pub(crate) const INSTALLED_NEXT_LAUNCH: &str = "Installed, next launch";
pub(crate) fn ready_detail(version: impl std::fmt::Display) -> String {
    format!("v{version} · relaunch")
}
pub(crate) const UPDATE_FAILED: &str = "Update failed";
pub(crate) fn failed_detail(version: impl std::fmt::Display) -> String {
    format!("v{version} · details")
}
/// The update card, as a screen reader reads it.
pub(crate) fn card_label(title: &str, detail: &str) -> String {
    format!("{title}. {detail}")
}
