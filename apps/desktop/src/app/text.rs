//! The words the app's own parts say: the crash report and its issue, the
//! updater's reasons and failures, the downloadable models' names and the
//! packaged self-test's verdicts, each a constant or a function. Also the
//! signature requirement `codesign` reads, which is no screen word.
use super::updater::Ineligible;
use mdoc_import::ocr;
use mdoc_store::crash::safe_message;
use mdoc_store::settings::Model;
use std::fmt::Display;

// ---- The crash report ------------------------------------------------------

/// A panic with no message of its own.
pub(crate) const NO_MESSAGE: &str = "(no message)";
/// Where a report's backtrace starts: what places the crash goes before it.
pub(crate) const BACKTRACE: &str = "\n\nBacktrace:";
pub(super) fn compose(message: &str, location: &str, thread: &str, backtrace: &str) -> String {
    format!(
        "mdoc {} — {} {}\nPanic in thread '{thread}' at {location}:\n{}\n\nBacktrace:\n{backtrace}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        safe_message(message),
    )
}

/// A report's line naming the first frame in mdoc's own code.
pub(crate) fn first_frame(frame: impl Display) -> String {
    format!("First mdoc frame: {frame}")
}
/// A report's line when the program changed since launch.
pub(crate) const REBUILT: &str = "The program file changed after launch (rebuilt or updated): \
             backtrace symbols may be wrong; the panic location above is still right.";
/// A report's line naming what the window showed.
pub(crate) fn window_state(state: impl Display) -> String {
    format!("Window: {state}")
}
/// A frame as a report names it: its function and its source line.
pub(crate) fn frame(function: impl Display, relative: impl Display) -> String {
    format!("{function} at {relative}")
}
/// Where a report too long for an issue URL was cut.
pub(crate) const CUT: &str = "\n… (cut; use Copy for the full report)";
/// The issue's body around the report.
pub(crate) fn issue_body(body: impl Display) -> String {
    format!(
        "<!-- Check that nothing here is confidential before submitting. -->\n\n```\n{body}\n```\n"
    )
}
/// The issue's title.
pub(crate) const CRASH_REPORT: &str = "Crash report";

// ---- Updates ---------------------------------------------------------------

impl Ineligible {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Unsupported => "On this system, install updates from the release page.",
            Self::NotABundle => "This development build can't update itself.",
            Self::Translocated => {
                "Move mdoc to the Applications folder and reopen it to install updates here."
            }
            Self::DiskImage => {
                "mdoc is running from its disk image. Drag it to Applications and reopen it to install updates here."
            }
            Self::NotWritable => {
                "mdoc can't replace itself in its current folder. Install the update from the release page."
            }
            Self::NotSigned => {
                "This build isn't signed by the mdoc developer, so it can't update itself."
            }
        }
    }
}

/// Settings › Updates while the copy's eligibility is being checked.
pub(crate) const CHECKING_ELIGIBILITY: &str = "Checking whether mdoc can update itself…";
/// A release with no disk image or no checksums.
pub(crate) const NOT_INSTALLABLE: &str = "This release can't be installed from inside mdoc.";
/// The log line when an update failed.
pub(crate) fn update_failed_log(version: impl Display, message: impl Display) -> String {
    format!("mdoc: update to {version} failed: {message}")
}
/// The log line when an update failed while mdoc quit.
pub(crate) fn update_failed_at_quit_log(version: impl Display, message: impl Display) -> String {
    format!("mdoc: update to {version} failed at quit: {message}")
}
/// `--mdoc-self-update`'s lines.
pub(crate) const NOT_SEMVER: &str = "The app version isn't SemVer.";
pub(crate) fn latest_version(current: impl Display) -> String {
    format!("mdoc {current} is the latest version.")
}
pub(crate) const CANCELLED: &str = "Cancelled.";
pub(crate) fn updated(current: impl Display, version: impl Display) -> String {
    format!("Updated mdoc {current} to {}.", version)
}
pub(crate) const NO_CACHE_FOLDER: &str = "No cache folder for the update.";
pub(crate) fn not_in_checksums(name: impl Display) -> String {
    format!("The release's SHA256SUMS doesn't list {name}.")
}

// Fetching the disk image.

pub(crate) const LARGER_THAN_SAID: &str = "The update is larger than the release says.";
pub(crate) const CHECKSUM_MISMATCH: &str =
    "The downloaded update doesn't match its published checksum.";
pub(crate) fn refused_download(url: impl Display) -> String {
    format!("Refused to download the update from {url}.")
}
pub(crate) const REDIRECT_WITHOUT_LOCATION: &str =
    "The update server sent a redirect without a location.";
pub(crate) const TOO_MANY_REDIRECTS: &str = "The update download redirected too many times.";

// Installing on macOS: only the macOS install steps use these.

#[cfg(target_os = "macos")]
pub(crate) const NO_APP_IN_IMAGE: &str = "The downloaded disk image does not contain mdoc.app.";
#[cfg(target_os = "macos")]
pub(crate) fn not_signed(e: impl Display) -> String {
    format!("The update is not signed by the mdoc developer: {e}")
}
#[cfg(target_os = "macos")]
pub(crate) fn gatekeeper_rejected(e: impl Display) -> String {
    format!("macOS Gatekeeper rejected the update: {e}")
}
#[cfg(target_os = "macos")]
pub(crate) fn wrong_version(found: impl Display, version: impl Display) -> String {
    format!("The update contains mdoc {found}, not the {version} the release promised.")
}
#[cfg(target_os = "macos")]
pub(crate) const NO_PARENT_FOLDER: &str = "The installed app has no parent folder.";
#[cfg(target_os = "macos")]
pub(crate) const NO_NAME: &str = "The installed app has no name.";
#[cfg(target_os = "macos")]
pub(crate) fn not_restored(backup: impl Display, e: impl Display) -> String {
    format!("Couldn't restore the previous app from {}: {e}", backup)
}
#[cfg(target_os = "macos")]
pub(crate) fn not_moved_aside(e: impl Display) -> String {
    format!("Couldn't move the current app aside: {e}")
}
#[cfg(target_os = "macos")]
pub(crate) fn not_moved_into_place(e: impl Display) -> String {
    format!("Couldn't move the new app into place: {e}")
}
#[cfg(target_os = "macos")]
pub(crate) fn not_moved_nor_restored(
    e: impl Display,
    restore: impl Display,
    backup: impl Display,
) -> String {
    format!(
        "Couldn't move the new app into place ({e}) or restore the previous one ({restore}). The previous app is at {}.",
        backup
    )
}
#[cfg(target_os = "macos")]
pub(crate) const NO_READABLE_VERSION: &str = "The update has no readable version.";
#[cfg(target_os = "macos")]
pub(crate) fn not_copied(e: impl Display) -> String {
    format!("Couldn't copy the update: {e}")
}
#[cfg(target_os = "macos")]
pub(crate) fn image_not_opened(e: impl Display) -> String {
    format!("Couldn't open the downloaded disk image: {e}")
}

// Not words: `codesign`'s argument for the designated requirement. Never
// translated.

/// `-R` with the requirement every mdoc bundle must satisfy.
#[cfg(target_os = "macos")]
pub(crate) fn requirement_argument(requirement: impl Display) -> String {
    format!("-R{requirement}")
}

// ---- The downloadable models -----------------------------------------------

/// A model's status where automatic inference doesn't run.
pub(crate) const UNSUPPORTED: &str =
    "Automatic inference supports Apple Silicon macOS and Windows x64.";
pub fn setup_label(model: Model, remove: bool) -> &'static str {
    match (model, remove) {
        (Model::Ocr(_), false) => "installing text recognition",
        (Model::Ocr(_), true) => "removing text recognition",
        (Model::Pii(_), false) => "installing pseudonymization model",
        (Model::Pii(_), true) => "removing pseudonymization model",
    }
}

/// How setup prompts name the bundle, e.g. "English OCR".
pub fn short_name(model: Model) -> String {
    match model {
        Model::Ocr(m) => format!("{} OCR", ocr::title(m)),
        Model::Pii(m) => format!("{} model", m.title()),
    }
}

/// A model's attribution line.
pub(crate) fn attribution(source: impl Display, license: impl Display) -> String {
    format!("Source: {source}\nLicense: {license}")
}

// ---- The packaged self-test ------------------------------------------------

pub(crate) const SELF_TEST_UNSUPPORTED: &str =
    "The self-test needs Apple Silicon macOS or Windows x64.";
pub(crate) const OCR_DID_NOT_RUN: &str = "OCR did not run.";
pub(crate) const CONVERTED_WITHOUT_OCR: &str = "The document converted without OCR.";
pub(crate) fn text_differs(markdown: impl Display) -> String {
    format!("Recognized text differs:\n{}", markdown)
}
