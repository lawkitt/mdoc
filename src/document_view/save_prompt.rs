//! Save As that says why it opens. GPUI's save dialog takes no message, so
//! the vendored platform crates take one on the side, for the next dialog
//! only: the panel's message line on macOS, the dialog's title on Windows
//! (`vendor/gpui-pre-macos/MDOC-PATCH.md`, `vendor/gpui-pre-windows/MDOC-PATCH.md`).

/// Why Save As opens on the way to a new case: a case lists its documents by
/// path, so a fresh conversion is saved first (#75).
pub(crate) const CASE_REASON: &str = "Save the document first so the case can find it";

#[cfg(test)]
thread_local! {
    static LAST_REASON: std::cell::Cell<Option<Option<&'static str>>> =
        const { std::cell::Cell::new(None) };
}

/// Runs `prompt` (one `prompt_for_new_path` call) with `reason` on the
/// dialog it opens. The platform takes the reason when the dialog is asked
/// for; it is cleared afterwards in case none was (the test platform).
pub(crate) fn with_reason<R>(reason: Option<&'static str>, prompt: impl FnOnce() -> R) -> R {
    #[cfg(test)]
    LAST_REASON.set(Some(reason));
    set(reason);
    let result = prompt();
    set(None);
    result
}

/// The reason the last Save As in this test opened with, taken.
#[cfg(test)]
pub(crate) fn take_last_reason() -> Option<Option<&'static str>> {
    LAST_REASON.take()
}

#[cfg(target_os = "macos")]
fn set(reason: Option<&str>) {
    gpui_macos::set_next_save_prompt_message(reason.map(str::to_owned));
}

#[cfg(windows)]
fn set(reason: Option<&str>) {
    gpui_windows::set_next_save_prompt_message(reason.map(str::to_owned));
}

#[cfg(not(any(target_os = "macos", windows)))]
fn set(_reason: Option<&str>) {}
