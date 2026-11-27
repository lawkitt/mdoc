//! The window's own words: its notices on restoring tabs and on comparing
//! models, the comparison window's titles and the clipboard actions' names.
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

pub(crate) fn could_not_restore_tabs(error: impl Display) -> String {
    format!("Could not restore tabs: {error}")
}
