//! The window's own words: its notices on restoring tabs and on comparing
//! models, the comparison window's titles, the clipboard actions' names, a dev
//! profile's window title and what a crash report says the window showed.
use std::fmt::Display;

pub(crate) const OPEN_BEFORE_COMPARING: &str = "Open a document before comparing models.";
pub(crate) const OCR_COMPARISON_NEEDS_PDF: &str =
    "OCR comparison requires an attached original PDF.";
pub(crate) const CONVERT_BEFORE_COMPARING: &str =
    "Convert the document to Markdown before comparing detection.";
pub(crate) const COMPARE_OCR_TITLE: &str = "Compare OCR models — mdoc";
pub(crate) const COMPARE_PSEUDONYMIZATION_TITLE: &str = "Compare pseudonymization models — mdoc";
pub(crate) const PASTE_AI_REPLY: &str = "Paste AI reply";
pub(crate) const PSEUDONYMIZE_CLIPBOARD: &str = "Pseudonymize clipboard";
pub(crate) const RESTORE_CLIPBOARD: &str = "Restore clipboard";

pub(crate) fn dev_profile_title(title: impl Display) -> String {
    format!("{title} — dev profile")
}

/// What a crash report says the window showed (no text, no names): the
/// document view's state, or Home when there is none; the tab count; the
/// sidebar; the theme; and whether the palette is open.
pub(crate) fn crash_state(
    view: Option<String>,
    tabs: usize,
    sidebar_expanded: bool,
    theme: impl std::fmt::Debug,
    palette_open: bool,
) -> String {
    format!(
        "{}, {tabs} tabs, sidebar {}, {theme:?} theme{}",
        view.as_deref().unwrap_or("Home"),
        if sidebar_expanded {
            "expanded"
        } else {
            "collapsed"
        },
        if palette_open { ", palette open" } else { "" },
    )
}

pub(crate) fn could_not_restore_tabs(error: impl Display) -> String {
    format!("Could not restore tabs: {error}")
}
