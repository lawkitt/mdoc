//! Saving's words: each string a user reads about autosave, the writers,
//! intake, the live views and the quit is a constant or a function here, with
//! its counts worded only here.

// ---- Autosave ---------------------------------------------------------------

/// Why a tab's write failed, after "Could not save …: ".
pub(crate) fn names_not_kept(error: impl std::fmt::Display) -> String {
    format!("its names could not be kept. {error}")
}
pub(crate) fn text_not_saved(error: impl std::fmt::Display) -> String {
    format!("its text could not be saved. {error}")
}
/// A new document's title before its first line has words.
pub(crate) const UNTITLED: &str = "Untitled";
/// A document's notes that could not be saved.
pub(crate) fn notes_not_saved(name: &str, error: impl std::fmt::Display) -> String {
    format!("Could not save the notes of “{name}”: {error}")
}
pub(crate) fn closed_notes_not_saved(error: impl std::fmt::Display) -> String {
    format!("Could not save a closed document's notes: {error}")
}
/// A Project's open members' names could not be folded before its write.
pub(crate) fn project_names_not_kept(error: impl std::fmt::Display) -> String {
    format!("Could not keep the Project's names: {error}")
}
/// A write with no Vault this run.
pub(crate) const DOCUMENTS_UNAVAILABLE: &str = "Documents are unavailable.";
/// A document the Vault made with an id mdoc can't read.
pub(crate) const INVALID_ID: &str = "Invalid id.";
/// A Project, a document or a closed document that could not be saved.
pub(crate) fn project_not_saved(name: &str, error: impl std::fmt::Display) -> String {
    format!("Could not save the Project “{name}”: {error}")
}
pub(crate) fn not_saved(name: &str, error: impl std::fmt::Display) -> String {
    format!("Could not save “{name}”: {error}")
}
pub(crate) fn closed_not_saved(error: impl std::fmt::Display) -> String {
    format!("Could not save a closed document: {error}")
}

// ---- Intake -----------------------------------------------------------------

/// Files imported before: the question and its detail.
pub(crate) fn repeats_question(repeats: &[String]) -> (String, String) {
    match repeats {
        [one] => (
            format!("“{one}” is already in mdoc"),
            "Open the document you imported before, or import the file again as a new \
                 document."
                .to_string(),
        ),
        many => (
            format!("{} files are already in mdoc", many.len()),
            format!(
                "{}\n\nOpen the documents you imported before, or import the files again \
                     as new documents. Other files open either way.",
                many.join("\n")
            ),
        ),
    }
}
/// The question's buttons.
pub(crate) const OPEN_EXISTING: &str = "Open Existing";
pub(crate) const IMPORT_AGAIN: &str = "Import Again";
/// Files that could not be imported, each with why.
pub(crate) fn could_not_import(failed: &[(String, String)]) -> String {
    let names: Vec<_> = failed
        .iter()
        .map(|(name, error)| format!("{name} ({error})"))
        .collect();
    format!("Could not import: {}", names.join(", "))
}
/// An import into a Project deleted meanwhile.
pub(crate) const PROJECT_GONE: &str = "Its Project no longer exists.";
/// Files of no supported kind.
pub(crate) fn unsupported_skipped(names: &[String]) -> String {
    format!("Unsupported files skipped: {}", names.join(", "))
}

// ---- The live views ---------------------------------------------------------

/// A new document's file name, numbered after the first.
pub(crate) fn untitled_file(number: usize) -> String {
    if number == 1 {
        "Untitled.md".to_string()
    } else {
        format!("Untitled {number}.md")
    }
}
/// A document, or its source, that could not be read from the Vault.
pub(crate) fn could_not_open(error: impl std::fmt::Display) -> String {
    format!("Could not open the document. {error}")
}
pub(crate) fn source_not_opened(error: impl std::fmt::Display) -> String {
    format!("Could not open the document's source. {error}")
}
/// Where the Source would be when the original can't be shown.
pub(crate) const ORIGINAL_MISSING: &str = "The original file is missing from the Vault, so it can't be shown. \
                         The Markdown is not affected.";
pub(crate) fn original_not_opened(error: impl std::fmt::Display) -> String {
    format!(
        "The original file could not be opened, so it can't be shown. The \
                         Markdown is not affected. {error}"
    )
}
/// A tab that waited for the Vault, when it can't be had.
pub(crate) fn document_cannot_open(why: impl std::fmt::Display) -> String {
    format!("This document can't open. {why}")
}

// ---- The quit and the session -----------------------------------------------

pub(crate) fn quit_not_saved(name: &str, error: impl std::fmt::Display) -> String {
    format!("Could not save \u{201c}{}\u{201d}: {error}", name)
}
pub(crate) fn quit_project_not_saved(name: &str, error: impl std::fmt::Display) -> String {
    format!("Could not save the Project \u{201c}{name}\u{201d}: {error}")
}
pub(crate) fn list_not_saved(error: impl std::fmt::Display) -> String {
    format!("Could not save the document list: {error}")
}
pub(crate) fn session_not_saved(error: impl std::fmt::Display) -> String {
    format!("Could not save session: {error}")
}
/// The quit's question when the session can't be saved.
pub(crate) const TAB_LIST_NOT_SAVED: &str = "Could not save the tab list";
pub(crate) const TAB_LIST_DETAIL: &str = "Your documents are saved. Cancel to retry, or quit using the previous \
                     session list.";
pub(crate) const QUIT_WITHOUT_SESSION: &str = "Quit without updating session";
