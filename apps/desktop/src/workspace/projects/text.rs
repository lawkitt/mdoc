//! Projects' words: each string a user reads on the Project page, in the
//! Open Project list, New Project, the folder sheet, moves, delete and export
//! is a constant or a function here, with its counts and plurals worded only
//! here.

// ---- Export ---------------------------------------------------------------

/// An export that could not read its Project.
pub(crate) fn export_failed(error: impl std::fmt::Display) -> String {
    format!("Could not export the Project: {error}")
}
/// The folder picker's button.
pub(crate) fn export_here(name: &str) -> String {
    format!("Export \u{201c}{name}\u{201d} Here")
}
/// The folder picker did not open.
pub(crate) fn chooser_failed(name: &str, error: impl std::fmt::Display) -> String {
    format!(
        "Could not export \u{201c}{name}\u{201d}: the folder chooser \
         didn't open. {error}"
    )
}
/// A title in curly quotes, as a notice lists it.
pub(crate) fn quoted(title: impl std::fmt::Display) -> String {
    format!("\u{201c}{title}\u{201d}")
}
/// What an export lists when the Project's names wait to be saved.
pub(crate) const ITS_NAMES: &str = "its names";

/// Why an export did not start: these could not be saved first.
pub(crate) fn unsaved_notice(name: &str, unsaved: &[String]) -> String {
    format!(
        "Could not export \u{201c}{name}\u{201d}: {} could not be saved, so the export \
         would miss the latest changes. Nothing was exported.",
        unsaved.join(", ")
    )
}

/// What an export wrote, and what it could not.
pub(crate) fn exported_notice(
    name: &str,
    exported: Result<mdoc_vault::Exported, mdoc_vault::VaultError>,
) -> String {
    let exported = match exported {
        Ok(exported) => exported,
        Err(error) => return format!("Could not export \u{201c}{name}\u{201d}: {error}"),
    };
    let folder = dunce::simplified(&exported.folder).display().to_string();
    let written = match exported.documents {
        1 => "1 document".to_string(),
        n => format!("{n} documents"),
    };
    let mut notice = format!("Exported {written} of \u{201c}{name}\u{201d} to {folder}.");
    if !exported.failed.is_empty() {
        let failed: Vec<String> = (exported.failed.iter())
            .map(|(title, error)| format!("\u{201c}{title}\u{201d} ({error})"))
            .collect();
        notice.push_str(&format!(" Not written in full: {}.", failed.join(", ")));
    }
    notice
}

// ---- The folder sheet -----------------------------------------------------

/// Said when a tick would go over the cap.
pub(crate) const CAP_NOTE: &str = "Up to 20 files at a time";
/// A dropped folder with no name of its own.
pub(crate) const FOLDER: &str = "Folder";
fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
/// "2 subfolders".
pub(crate) fn subfolders(n: usize) -> String {
    plural(n, "subfolder", "subfolders")
}
/// "1 hidden file".
pub(crate) fn hidden_files(n: usize) -> String {
    plural(n, "hidden file", "hidden files")
}
/// "2 subfolders, 1 hidden file skipped".
pub(crate) fn skipped(parts: &[String]) -> String {
    parts.join(", ") + " skipped"
}
/// A folder with nothing to open.
pub(crate) fn no_supported_files(folder: &str) -> String {
    format!("“{}” has no supported files", folder)
}
/// Its subfolders, skipped.
pub(crate) fn subfolders_inside_skipped(n: usize, folder: &str) -> String {
    format!("{} inside “{}” skipped", subfolders(n), folder)
}
/// Its hidden files, skipped.
pub(crate) fn hidden_files_skipped(n: usize) -> String {
    format!("{} skipped", hidden_files(n))
}
/// The notes of an empty folder's notice, as one sentence.
pub(crate) fn notes(notes: &[String]) -> String {
    notes.join("; ") + "."
}
/// A row whose file already has a tab, Unfiled.
pub(crate) const ALREADY_OPEN: &str = "Already open";
/// A row whose file is open in a Project.
pub(crate) fn in_project(name: &str) -> String {
    format!("In “{name}”")
}
/// The sheet's primary button with a Project to add to.
pub(crate) fn add_to(name: &str) -> String {
    format!("Add to “{}”", name)
}
/// The sheet's primary button otherwise.
pub(crate) const OPEN_AS_NEW_PROJECT: &str = "Open as New Project";
/// A dropped folder's Project could not be made.
pub(crate) fn could_not_become_project(folder: &str, error: impl std::fmt::Display) -> String {
    format!("“{}” could not become a Project: {error}", folder)
}
/// A row, as a screen reader reads it with its status.
pub(crate) fn row_label(name: &str, status: &str) -> String {
    format!("{}, {status}", name)
}
/// How many rows are ticked.
pub(crate) fn selected(ticked: usize, total: usize) -> String {
    format!("{ticked} of {total} selected")
}
/// The count with the cap's note.
pub(crate) fn with_cap_note(count: &str, note: &str) -> String {
    format!("{count}. {note}.")
}
/// The sheet, as a screen reader names it.
pub(crate) fn open_folder(folder: &str) -> String {
    format!("Open folder “{}”", folder)
}
/// The sheet's title: the folder's name in quotes.
pub(crate) fn quoted_name(folder: &str) -> String {
    format!("“{}”", folder)
}
/// The sheet's list, as a screen reader names it.
pub(crate) const FILES: &str = "Files";
/// The sheet's button that opens the ticked files as they are.
pub(crate) const OPEN_FILES: &str = "Open files";

// ---- New Project ----------------------------------------------------------

/// The sheet's title, also its name for a screen reader.
pub(crate) const NEW_PROJECT: &str = "New Project";
/// The line under the title.
pub(crate) const NEW_PROJECT_NOTE: &str = "Documents you add from its page land in it.";
/// The name field's placeholder and label.
pub(crate) const PROJECT_NAME: &str = "Project name";
pub(crate) const NAME: &str = "Name";
/// The sheet's button.
pub(crate) const CREATE: &str = "Create";
/// A Project that could not be made.
pub(crate) fn could_not_create(name: &str, error: impl std::fmt::Display) -> String {
    format!("Could not create “{name}”: {error}")
}
/// A taken name, numbered: "Lease 2".
pub(crate) fn numbered(name: &str, n: usize) -> String {
    format!("{name} {n}")
}

// ---- The Project page -----------------------------------------------------

/// A swatch's name, as the page reads it out.
pub(crate) const RED: &str = "Red";
pub(crate) const ORANGE: &str = "Orange";
pub(crate) const AMBER: &str = "Amber";
pub(crate) const GREEN: &str = "Green";
pub(crate) const TEAL: &str = "Teal";
pub(crate) const BLUE: &str = "Blue";
pub(crate) const VIOLET: &str = "Violet";
pub(crate) const PINK: &str = "Pink";
/// The segments.
pub(crate) const DOCUMENTS: &str = "Documents";
pub(crate) const REPLIES: &str = "Replies";
pub(crate) const NAMES: &str = "Names";
pub(crate) const AI_ACTIVITY: &str = "AI activity";
/// The segment row, as a screen reader names it.
pub(crate) const PROJECT_VIEW: &str = "Project view";
/// The header's title bar label.
pub(crate) const PROJECT: &str = "Project";
/// The ⋯ menu.
pub(crate) const RENAME: &str = "Rename";
pub(crate) const COLOUR: &str = "Colour";
pub(crate) const CATEGORIES_TO_DETECT: &str = "Categories to detect";
pub(crate) const EXPORT_PROJECT: &str = "Export Project…";
pub(crate) const DELETE_PROJECT: &str = "Delete Project…";
/// The header's controls, as a screen reader names them.
pub(crate) fn rename(name: &str) -> String {
    format!("Rename {}", name)
}
pub(crate) const PROJECT_COLOUR: &str = "Project colour";
pub(crate) const ADD_DOCUMENT: &str = "Add document";
pub(crate) fn actions_for(name: &str) -> String {
    format!("Actions for Project {}", name)
}
/// Add document's file picker button.
pub(crate) fn add_to_curly(name: &str) -> String {
    format!("Add to \u{201c}{name}\u{201d}")
}
/// The file picker did not open.
pub(crate) fn chooser_did_not_show(error: impl std::fmt::Display) -> String {
    format!("Could not show the file chooser: {error}")
}
/// The Documents segment.
pub(crate) const NO_DOCUMENTS: &str =
    "No documents yet. Add a document, or drop files on this page.";
/// A member row's detail: whether it needs review, and when it changed.
pub(crate) fn member_detail(needs_review: bool, updated: &str) -> String {
    format!(
        "{}{}",
        if !needs_review {
            ""
        } else {
            "Needs review · "
        },
        format_args!("updated {}", updated)
    )
}
pub(crate) const MOVE_TO_UNFILED: &str = "Move to Unfiled";
pub(crate) fn move_to_unfiled(title: &str) -> String {
    format!("Move “{}” to Unfiled", title)
}
/// The Replies segment.
pub(crate) const NO_REPLIES: &str = "No saved replies. An AI reply pasted into one of this Project's documents is \
                 kept here.";
pub(crate) const DELETE: &str = "Delete…";
pub(crate) fn delete_named(title: &str) -> String {
    format!("Delete “{}”", title)
}
/// The Names segment: the search field and the add field.
pub(crate) const SEARCH_NAMES: &str = "Search names";
pub(crate) const ADD_A_NAME: &str = "Add a name";
/// An identity's count in one open member.
pub(crate) fn open_count(title: &str, count: usize) -> String {
    format!("{title}: {count}")
}
/// What joins a row's parts.
pub(crate) const DOT: &str = " · ";
/// An identity's alias and its first original.
pub(crate) fn identity_line(alias: &str, original: &str) -> String {
    format!("{} · {}", alias, original)
}
/// An identity's variants, with its counts in the open members.
pub(crate) fn variants(n: usize, counts: &str) -> String {
    format!(
        "{} {}{}",
        n,
        if n == 1 { "variant" } else { "variants" },
        if counts.is_empty() {
            String::new()
        } else {
            format!(" · {counts}")
        }
    )
}
pub(crate) const FORGET_IDENTITY_MENU: &str = "Forget identity…";
/// Forgetting an identity: the question, its detail and its button.
pub(crate) fn forget_identity_question(alias: &str) -> String {
    format!("Forget identity “{alias}”?")
}
pub(crate) const FORGET_IDENTITY_DETAIL: &str = "Its originals are forgotten from this Project and its documents. Document text is untouched. This cannot be undone.";
pub(crate) const FORGET_IDENTITY: &str = "Forget Identity";
pub(crate) const NO_NAMES: &str =
    "No names yet. Names replaced in this Project's documents appear here.";
pub(crate) const NO_NAMES_MATCH: &str = "No names match.";
/// The categories to detect.
pub(crate) const CATEGORIES_NOTE: &str =
    "Scans skip categories that are off. Names you mark yourself are still replaced.";
/// A category's toggle, ticked or not.
pub(crate) fn category_toggle(on: bool, label: &str) -> String {
    format!("{} {}", if on { "✓" } else { "○" }, label)
}
/// The Project's lists.
pub(crate) const ALWAYS_AND_NEVER: &str = "Always and never replace";
pub(crate) const LISTS_NOTE: &str = "Scans in this Project's documents propose what you marked yourself and skip \
                 what you kept.";
pub(crate) const ALWAYS_REPLACE: &str = "Always replace";
pub(crate) const NEVER_REPLACE: &str = "Never replace";
pub(crate) const NO_ALWAYS: &str = "Nothing yet. Words you mark yourself appear here.";
pub(crate) const NO_NEVER: &str =
    "Nothing yet. Names you keep everywhere in a document appear here.";
pub(crate) const REMOVE: &str = "Remove";
pub(crate) fn remove_named(wording: &str) -> String {
    format!("Remove “{wording}”")
}
/// A document's file family, as its row shows it.
pub(crate) const PDF: &str = "PDF";
pub(crate) const DOC: &str = "DOC";
pub(crate) const XLS: &str = "XLS";
pub(crate) const PPT: &str = "PPT";
pub(crate) const BOOK: &str = "BOOK";
pub(crate) const IMG: &str = "IMG";
pub(crate) const SCAN: &str = "SCAN";
pub(crate) const MD: &str = "MD";
pub(crate) const TXT: &str = "TXT";
/// "4 documents · 2 replies · 11 names · created 3 Nov".
pub(crate) fn project_counts(
    documents: usize,
    replies: usize,
    names: usize,
    created: &str,
) -> String {
    let plural = |count: usize, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    format!(
        "{} · {} · {} · created {}",
        plural(documents, "document", "documents"),
        plural(replies, "reply", "replies"),
        plural(names, "name", "names"),
        created
    )
}
/// A date as the page writes it: "3 Nov".
pub(crate) fn day_month(day: impl std::fmt::Display, month: usize) -> String {
    format!(
        "{day} {}",
        [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"
        ][month]
    )
}

// ---- Loading, moves and delete --------------------------------------------

/// Projects that could not be read when the Vault opened.
pub(crate) fn projects_not_opened(failed: usize) -> String {
    if failed == 1 {
        "1 Project could not be opened. It is damaged or saved with another key.".into()
    } else {
        format!(
            "{failed} Projects could not be opened. They are damaged or saved with \
                     another key."
        )
    }
}
/// A member's names could not be folded into its Project.
pub(crate) fn names_not_kept(error: impl std::fmt::Display) -> String {
    format!("Could not keep the Project's names: {error}")
}
/// A move that could not start.
pub(crate) fn could_not_move_curly(title: &str, error: impl std::fmt::Display) -> String {
    format!("Could not move \u{201c}{title}\u{201d}: {error}")
}
/// A move whose Projects' names could not be kept first.
pub(crate) fn move_names_not_kept(title: &str, error: impl std::fmt::Display) -> String {
    format!(
        "Could not move \u{201c}{title}\u{201d}: its Project's names could not be kept. \
                 {error}"
    )
}
/// A move the Vault refused.
pub(crate) fn could_not_move(title: &str, error: impl std::fmt::Display) -> String {
    format!("Could not move “{title}”: {error}")
}
/// Where a document went, for the toast.
pub(crate) const UNFILED: &str = "Unfiled";
/// The toast after an Undo.
pub(crate) fn moved_back(title: &str, place: &str) -> String {
    format!("“{title}” moved back to {place}.")
}
/// The toast after a move, with the aliases it renumbered.
pub(crate) fn moved(title: &str, place: &str, renumbered: usize) -> String {
    if renumbered > 0 {
        format!(
            "“{title}” moved to {place}. {renumbered} {} renumbered to fit.",
            if renumbered == 1 { "alias" } else { "aliases" }
        )
    } else {
        format!("“{title}” moved to {place}.")
    }
}
/// The toast's undo button.
pub(crate) const UNDO: &str = "Undo";
/// Deleting a Project: the question.
pub(crate) fn delete_project_question(name: &str) -> String {
    format!("Delete the Project “{name}”?")
}
/// The question's detail for an empty Project, and its button.
pub(crate) const DELETE_EMPTY_DETAIL: &str = "Its names and its AI replies are deleted from this computer. This cannot \
                     be undone.";
pub(crate) const DELETE_BUTTON: &str = "Delete";
/// The question's detail with `count` documents.
pub(crate) fn delete_detail(count: usize) -> String {
    let documents = if count == 1 {
        "Its document".to_string()
    } else {
        format!("Its {count} documents")
    };
    format!(
        "{documents} can move to Unfiled, keeping the names their text uses, or be \
                     deleted with it. The Project's own AI replies are deleted either way. This \
                     cannot be undone."
    )
}
/// The question's buttons with documents.
pub(crate) const DELETE_DOCUMENTS: &str = "Delete Documents";
/// A delete whose member could not be saved first.
pub(crate) fn delete_member_not_saved(name: &str, error: impl std::fmt::Display) -> String {
    format!(
        "Could not delete the Project: \u{201c}{name}\u{201d} could not be \
                         saved first, so nothing was moved. {error}"
    )
}
/// A delete whose Project's names could not be saved first.
pub(crate) fn delete_names_not_saved(error: impl std::fmt::Display) -> String {
    format!(
        "Could not delete the Project: its names could not be saved first, so \
                     nothing was moved. {error}"
    )
}
/// A delete the Vault refused.
pub(crate) fn could_not_delete(error: impl std::fmt::Display) -> String {
    format!("Could not delete the Project: {error}")
}
/// A delete whose members changed after its question.
pub(crate) const DELETE_CHANGED: &str = "The Project's documents changed before it was deleted, so nothing was \
             deleted. Choose again for them as they are now.";

// ---- The Open Project list ------------------------------------------------

/// The list's title and close button.
pub(crate) const OPEN_PROJECT: &str = "Open Project";
pub(crate) const CLOSE_PROJECT_LIST: &str = "Close Project list";
/// A row, as a screen reader names it.
pub(crate) fn open_named(name: &str) -> String {
    format!("Open {}", name)
}
/// A row's detail: its names and documents, and whether it is open.
pub(crate) fn list_row_detail(names: usize, documents: usize, open: bool) -> String {
    format!(
        "{names} {} · {documents} {}{}",
        if names == 1 { "name" } else { "names" },
        if documents == 1 {
            "document"
        } else {
            "documents"
        },
        if open { " · open" } else { "" }
    )
}
/// The list with no Project.
pub(crate) const NO_PROJECTS: &str = "No Projects yet. Choose New Project, or right-click a document \
                             and choose Move to New Project.";
/// The list while the Vault waits for its key.
pub(crate) const LOCKED: &str = "Your documents are locked.";
pub(crate) const UNLOCK: &str = "Unlock…";
