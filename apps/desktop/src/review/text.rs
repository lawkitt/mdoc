//! The words the pseudonymization review shows: the toolbar's scan, the
//! Replacements panel and its setup card, the replacement popup, the field
//! chooser, the view switcher and Side by side, the Inspector's tabs and
//! Share, the AI reply, the Project's names, refusals and the clipboard's
//! notices, each a constant or a function, with their counts and plurals
//! worded here.
use super::mapping::PanelFilter;
use super::refusal::Step;
use crate::shelf::Readiness;
use mdoc_store::settings;
use std::fmt::Display;

// ---- The toolbar's scan -------------------------------------------------------

/// The running scan's stages, as the toolbar shows them in turn.
pub(super) const SCAN_STAGES: [&str; 4] = [
    "Reading…",
    "Analyzing…",
    "Finding names…",
    "Linking mentions…",
];

/// A scan of several windows: the one being read.
pub(super) fn finding_names_of(done: usize, windows: usize) -> String {
    format!("Finding names… {} of {windows}", done)
}

/// What a screen reader says for the scan status, which cancels it.
pub(super) fn cancel_pseudonymization(stage: impl Display) -> String {
    format!("{} Cancel pseudonymization", stage)
}

/// An edit cancelled the running scan.
pub(super) const SCAN_CANCELLED: &str = "Document changed; scan cancelled. Rescan when ready.";
/// The model's setup waits for another model job.
pub(super) const ANOTHER_MODEL_JOB: &str = "Another model job is running. Retry when it finishes.";

// ---- The selection pill ---------------------------------------------------------

pub(super) const REPLACE: &str = "Replace";
pub(super) const REPLACE_WITH_ALIAS: &str = "Replace with alias";

// ---- The crash report's description of the tab ----------------------------------

pub(super) const SOURCE_ONLY: &str = "source only";
pub(super) const PDF: &str = "PDF";
pub(super) const CONVERTED: &str = "converted";
pub(super) const MARKDOWN: &str = "Markdown";
pub(super) const SOURCE_SHOWN: &str = "source shown";
pub(super) const REVIEW_TAB: &str = "Review tab";
pub(super) const SHARE_TAB: &str = "Share tab";
pub(super) const NOTES_TAB: &str = "Notes tab";
pub(super) const POPUP_OPEN: &str = "popup open";
pub(super) const BEFORE_AFTER_VIEW: &str = "before/after view";
pub(super) const FIND_OPEN: &str = "find open";

/// The tab's kind, size and zoom.
pub(super) fn crash_kind(kind: &str, bytes: usize, zoom: i32) -> String {
    format!("{kind}, {} bytes, zoom {}%", bytes, zoom)
}

/// The review's counts.
pub(super) fn crash_counts(proposed: usize, applied: usize) -> String {
    format!("{} proposed, {} applied", proposed, applied)
}

// ---- The field chooser ------------------------------------------------------------

pub(super) fn choose_a_field(count: usize) -> String {
    format!("Choose a field · {count}")
}
pub(super) const CLOSE_FIELD_CHOOSER: &str = "Close field chooser";
pub(super) const FIELD_NO_LONGER_AVAILABLE: &str = "Field no longer available";

// ---- The view switcher and Side by side ---------------------------------------------

pub(super) const VIEW: &str = "View";
pub(super) const EDIT: &str = "Edit";
pub(super) const ORIGINAL: &str = "Original";
pub(super) const PSEUDONYMIZED: &str = "Pseudonymized";
pub(super) const SIDE_BY_SIDE: &str = "Side by side";
pub(super) const REPLY: &str = "Reply";
pub(super) const ZOOM_OUT: &str = "Zoom out";
pub(super) const RESET_ZOOM: &str = "Reset zoom";
pub(super) const ZOOM_IN: &str = "Zoom in";
pub(super) const SWITCH_COLUMN: &str = "Switch column";
pub(super) const CLOSE_REPLY: &str = "Close reply (Esc)";
pub(super) const CLOSE_SIDE_BY_SIDE: &str = "Close side by side (Esc)";

// ---- The Replacements panel and its setup card --------------------------------------

/// The pseudonymization setup card's words. A profile whose model is here
/// but whose shared runtime is an earlier release's is asked for an update,
/// not a first setup.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct SetupCopy {
    pub(super) title: &'static str,
    pub(super) text: &'static str,
    /// The primary button while setup is needed and nothing failed.
    pub(super) action: &'static str,
    pub(super) note: String,
}

impl SetupCopy {
    pub(super) fn new(
        model: Option<settings::Model>,
        pending: mdoc_import::model_download::Pending,
    ) -> Self {
        let update = pending.runtime_update();
        let note = match model {
            Some(m) if update => format!(
                "{} · {} MB runtime update",
                crate::app::models::short_name(m),
                mdoc_import::model_download::megabytes(pending.total())
            ),
            Some(m) if pending.total() > 0 => format!(
                "{} · {}, once",
                crate::app::models::short_name(m),
                pending.download_label()
            ),
            Some(m) => format!(
                "{} · runs on this computer",
                crate::app::models::short_name(m)
            ),
            None => String::new(),
        };
        if update {
            Self {
                title: "Update pseudonymization",
                text: "The model is already on this computer; this version of mdoc needs a newer shared runtime to run it. Review the whole document before sharing.",
                action: "Update & scan",
                note,
            }
        } else {
            Self {
                title: "Set up pseudonymization",
                text: "Detects names, organizations and identifiers on this computer. Review the whole document before sharing.",
                action: "Download & scan",
                note,
            }
        }
    }
}

pub(super) const PSEUDONYMIZE: &str = "Pseudonymize";
pub(super) const RETRY: &str = "Retry";
pub(super) const COLLAPSE_ALL_IDENTITIES: &str = "Collapse all identities";
pub(super) const EXPAND_ALL_IDENTITIES: &str = "Expand all identities";

impl PanelFilter {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Proposed => "To replace",
            Self::Applied => "Applied",
            Self::Kept => "Kept",
        }
    }
}

/// The filter button's name.
pub(super) fn show_filter(filter: &str) -> String {
    format!("Show: {}", filter)
}

/// The search field's and the alias fields' placeholders.
pub(super) const FIND_ALIAS_OR_ORIGINAL: &str = "Find an alias or original";
pub(super) const ALIAS: &str = "Alias";
pub(super) const FIND_AN_IDENTITY: &str = "Find an identity";
pub(super) const NEW_ALIAS: &str = "New alias";

/// The latest step's notice and its way back.
pub(super) fn added(original: impl Display) -> String {
    format!("Added “{}”", original)
}
pub(super) fn kept(kept: usize) -> String {
    format!("Kept {kept}")
}
pub(super) fn merged_into(wording: impl Display) -> String {
    format!("Merged into {wording}")
}
pub(super) fn applied(n: usize) -> String {
    format!("Applied {n}")
}
pub(super) const UNDO: &str = "Undo";

pub(super) const DROP_FOR_A_NEW_ALIAS: &str = "Drop to give it a new alias";
pub(super) const NO_MATCHING_REPLACEMENTS: &str = "No matching replacements.";
pub(super) const NOTHING_FOUND: &str = "Nothing found. Select text to add it by hand.";
pub(super) const UPDATE_ALIASES: &str = "Update aliases";
pub(super) const RESCAN: &str = "↻ Rescan";
pub(super) const REVIEW_FOR_MISSED_IDENTIFIERS: &str =
    "Review for missed identifiers before sharing.";

/// The Replacements footer's summary. A running scan has no counts to give
/// yet, so it says so instead of "Nothing to replace yet" (#309). What is
/// left counts `names`, the Review tab's unit, and its `pending` places
/// when they differ (#1314).
pub(super) fn replacement_summary(
    scanning: bool,
    names: usize,
    pending: usize,
    applied: usize,
    kept: usize,
) -> String {
    if scanning {
        return "Scanning…".into();
    }
    let mut parts = Vec::new();
    if pending > 0 {
        parts.push(format!("{} to replace", names_in_places(names, pending)));
    }
    if applied > 0 {
        parts.push(format!("{applied} applied"));
    }
    if kept > 0 {
        parts.push(format!("{kept} kept"));
    }
    if parts.is_empty() {
        "Nothing to replace yet".to_string()
    } else {
        parts.join(" · ")
    }
}

/// "2 names in 3 places", or "2 names" when each appears once (#1314).
pub(super) fn names_in_places(names: usize, places: usize) -> String {
    let unit = if names == 1 { "name" } else { "names" };
    if places == names {
        format!("{names} {unit}")
    } else {
        format!("{names} {unit} in {places} places")
    }
}

// ---- The panel's rows -------------------------------------------------------------------

/// A category's heading, as a screen reader reads it.
pub(super) fn category_heading(category: impl Display, entities: usize, mentions: usize) -> String {
    format!(
        "{}, {entities} {}, {mentions} {}",
        category,
        if entities == 1 { "name" } else { "names" },
        if mentions == 1 { "mention" } else { "mentions" },
    )
}

/// An identity's row, as a screen reader reads it.
pub(super) fn identity_row(
    original: impl Display,
    alias: impl Display,
    mentions: usize,
    applied: usize,
    kept: usize,
    expanded: bool,
) -> String {
    format!(
        "{} to {}, {} mentions, {} applied, {} kept, {}",
        original,
        alias,
        mentions,
        applied,
        kept,
        if expanded { "expanded" } else { "collapsed" }
    )
}

pub(super) fn link_to(alias: impl Display) -> String {
    format!("Link to {}", alias)
}
/// The tag of an identity whose every mention is kept.
pub(super) const KEPT_TAG: &str = "kept";
pub(super) const HIDE_MENTIONS: &str = "Hide mentions";
pub(super) const SHOW_MENTIONS: &str = "Show mentions";
pub(super) fn replace_with(original: impl Display, alias: impl Display) -> String {
    format!("Replace {} with {}", original, alias)
}
pub(super) fn original_kept(word: impl Display) -> String {
    format!("{word}: original kept")
}
pub(super) fn replace_word(word: impl Display) -> String {
    format!("Replace {word}")
}
/// A mention row's suffix when it is applied.
pub(super) const COMMA_APPLIED: &str = ", applied";
pub(super) const NOT_A_NAME: &str = "Not a name";

// ---- Merge hints and the checklist -------------------------------------------------------

pub(super) fn looks_like(wording: impl Display) -> String {
    format!("Looks like {wording}")
}
pub(super) const MERGE: &str = "Merge";
pub(super) fn merge_with(wording: impl Display) -> String {
    format!("Merge with {wording}")
}
pub(super) const COULD_NOT_PROPOSE_AGAIN: &str =
    "Couldn't propose it again; select the text to add it.";

// ---- The replacement popup ------------------------------------------------------------------

pub(super) const CANCEL_ADDITION: &str = "Cancel addition";
pub(super) fn tick_to_propose(in_scope: impl Display) -> String {
    format!("Replace · tick to propose {in_scope} again")
}
pub(super) fn untick_to_keep(in_scope: impl Display) -> String {
    format!("Replace · untick to keep {in_scope} originals")
}
pub(super) const TICK_TO_REPLACE_AGAIN: &str = "Tick to replace again (Space)";
/// The tick's tooltip, with the scope's shortcut.
pub(super) fn untick_to_keep_original(keys: impl Display) -> String {
    format!("Untick to keep the original (Space, {})", keys)
}
pub(super) fn kept_as_original(alias: impl Display) -> String {
    format!("Kept as the original; tick Replace to use {alias}")
}
pub(super) const CORRECT_CATEGORY: &str = "Correct category for selected scope";
pub(super) const APPLIED: &str = "Applied";
pub(super) const PREVIOUS_MENTION: &str = "Previous mention";
pub(super) fn previous_mention_of(alias: impl Display) -> String {
    format!("Previous mention of {alias} (←)")
}
/// The position's noun, as in "Mention 4 of 6".
pub(super) const MENTION: &str = "Mention";
pub(super) const NEXT_MENTION: &str = "Next mention";
pub(super) fn next_mention_of(alias: impl Display) -> String {
    format!("Next mention of {alias} (→)")
}
pub(super) const CLOSE_REPLACEMENT_POPUP: &str = "Close replacement popup";
pub(super) const HIDDEN_SOURCE_VALUE: &str =
    "Hidden source value · containing passage shown in document";
pub(super) const BELONGS_TO_MENU: &str = "Belongs to…";
/// The owner row's value when the identity belongs to nobody.
pub(super) const NOBODY_OWNER: &str = "nobody";
pub(super) const BELONGS_TO: &str = "Belongs to";
pub(super) const RENAME_ALIAS: &str = "Rename alias";
pub(super) fn rename_to(draft: impl Display) -> String {
    format!("Rename to \"{draft}\"")
}
pub(super) fn mentions(selected_count: usize) -> String {
    format!("{selected_count} mentions")
}
pub(super) const SAME_AS: &str = "Same as…";
pub(super) fn use_alias(
    alias: impl Display,
    original: impl Display,
    category: impl Display,
) -> String {
    format!("Use {alias}, {original}, {}", category)
}
pub(super) const NOBODY: &str = "Nobody";
pub(super) const THIS_ONE: &str = "This one";
pub(super) const ONLY_THIS_MENTION: &str = "Only this mention";
pub(super) const ALL: &str = "All";
pub(super) fn all_mentions_of(mentions: usize, alias: impl Display) -> String {
    format!("All {mentions} mentions of {alias}, in every spelling")
}
pub(super) fn all_mentions(count: usize) -> String {
    format!("All {count} mentions")
}

// ---- The popup's and the panel's refusals -----------------------------------------------------

pub(super) const CHOOSE_EXISTING_ENTITY: &str =
    "Choose the existing entity below to use its alias.";
pub(super) const REVIEW_THE_REPLACEMENT_AGAIN: &str =
    "Document changed. Review the replacement again.";
pub(super) const REVIEW_THE_MAPPING_AGAIN: &str = "Document changed. Review the mapping again.";
pub(super) const ENTITY_NO_LONGER_AVAILABLE: &str = "Entity is no longer available.";
pub(super) const CANDIDATE_CHANGED: &str = "Candidate changed. Review it again.";
pub(super) const VARIANT_NO_LONGER_AVAILABLE: &str = "Variant is no longer available.";
pub(super) const IDENTITY_NO_LONGER_AVAILABLE: &str = "Identity is no longer available.";
pub(super) const REPLACEMENT_NO_LONGER_AVAILABLE: &str = "Replacement is no longer available.";
pub(super) const ORIGINAL_VARIANT_NO_LONGER_AVAILABLE: &str =
    "Original variant is no longer available.";
pub(super) const REPLACEMENT_CHANGED: &str = "Replacement changed. Review it again.";
pub(super) const CANDIDATES_OVERLAP: &str = "Candidates overlap. Review them again.";

// ---- The Inspector's tabs and Share ------------------------------------------------------------

/// The Review tab's label: the names in the list (#1314).
pub(super) fn review_tab_label(names: usize) -> String {
    format!("Review · {names}")
}

pub(super) const SHARE: &str = "Share";
pub(super) const NOTES: &str = "Notes";
pub(super) const SEND_TO_AN_AI: &str = "Send to an AI";
pub(super) const ADDS_A_SHORT_NOTE: &str =
    "Adds a short note asking the AI to keep every alias exactly as written.";
pub(super) const PROJECT: &str = "Project";
pub(super) fn open_project(name: impl Display) -> String {
    format!("Open Project {}", name)
}
pub(super) const KEPT_WITH_THE_DOCUMENT: &str =
    "Kept with the document, encrypted on this computer.";
pub(super) const UNFILED_NAMES_KEPT: &str = "Unfiled. Its names are kept with it, encrypted on this computer; in a Project, its documents share them.";
pub(super) const MOVE_TO_PROJECT: &str = "Move to Project…";
pub(super) const MOVE_TO_NEW_PROJECT: &str = "Move to New Project";
/// Share's copy button when names are left: it counts names.
pub(super) fn replace_names_and_copy(names: usize) -> String {
    let unit = if names == 1 { "name" } else { "names" };
    format!("Replace {names} {unit} and copy")
}
pub(super) const COPY_FOR_AI: &str = "Copy for AI";

/// The Share status for `pending` proposals, `applied` replacements, `kept`
/// mentions and `left` known names still in the text as written. It counts
/// `names`, the Review tab's unit: those still to replace while any is,
/// then those replaced (#1314).
pub(super) fn share_status(
    names: usize,
    pending: usize,
    applied: usize,
    kept: usize,
    left: usize,
) -> String {
    let unit = if names == 1 { "name" } else { "names" };
    if pending > 0 {
        let them = if names == 1 { "it" } else { "them" };
        return format!(
            "{} still to replace — apply {them} first.",
            names_in_places(names, pending)
        );
    }
    if applied == 0 {
        return "Nothing is replaced yet.".to_string();
    }
    let applied = format!("{names} {unit}");
    let kept = if kept > 0 {
        format!(" · {kept} kept")
    } else {
        String::new()
    };
    match left {
        0 => format!("{applied} replaced{kept}. Names in the text are aliases now."),
        1 => format!("{applied} replaced{kept}. 1 name not replaced."),
        n => format!("{applied} replaced{kept}. {n} names not replaced."),
    }
}

/// The replacement tip: an applied identity's suffix, and how to edit it.
pub(super) const DOT_APPLIED: &str = " · applied";
pub(super) const KEPT_CLICK_TO_EDIT: &str = "Kept · click to edit";
pub(super) const CLICK_TO_EDIT: &str = "Click to edit";

// ---- The AI reply ------------------------------------------------------------------------------

pub(super) const AI_REPLY_HEADING: &str = "AI REPLY";
pub(super) const AI_REPLY: &str = "AI reply";
pub(super) const NOTHING_CAN_BE_RESTORED: &str =
    "No aliases are applied, so nothing can be restored.";
pub(super) const PASTE_THE_AIS_ANSWER: &str =
    "Paste the AI's answer: the real names come back here at once.";
pub(super) const COPY_REPLY: &str = "Copy reply";
pub(super) const OPEN_REPLY: &str = "Open reply";
pub(super) const NEXT_UNRECOGNIZED: &str = "Next unrecognized";
pub(super) const SAVED_REPLIES_HEADING: &str = "SAVED REPLIES";
pub(super) const PASTE_THE_AIS_REPLY: &str = "Paste the AI's reply here";

/// "N values restored · M unrecognized".
pub(super) fn reply_summary(restored: usize, unresolved: usize) -> String {
    format!(
        "{restored} {} restored · {unresolved} unrecognized",
        if restored == 1 { "value" } else { "values" }
    )
}

// ---- The Project's names ------------------------------------------------------------------------

pub(super) const DOCUMENT_CHANGED_TRY_AGAIN: &str = "The document changed. Try again.";
pub(super) fn could_not_follow(error: impl Display) -> String {
    format!("Could not follow a change to the Project: {error}")
}
/// The names section's toggle when the Project has no name to list.
pub(super) fn add_a_name_to(chevron: &str, project: impl Display) -> String {
    format!("{} Add a name to “{}”", chevron, project)
}
/// The names section's toggle: the Project's names this document lacks.
pub(super) fn in_project_not_here(chevron: &str, project: impl Display, absent: usize) -> String {
    format!(
        "{} In “{}”, not in this document ({})",
        chevron, project, absent
    )
}
pub(super) const ADD_A_NAME_TO_THE_PROJECT: &str = "Add a name to the Project…";

// ---- Refusals ---------------------------------------------------------------------------------

impl Step {
    /// The button's label, the count shown before the click.
    pub(crate) fn label(self) -> String {
        match self {
            Self::ReplaceAndCopy(n) => format!("Replace {n} and copy"),
            Self::Review => "Review".into(),
            Self::FindNames => "Find names".into(),
        }
    }
}

pub(super) fn reason(readiness: Readiness, scanned: bool, failed: bool) -> String {
    match readiness {
        Readiness::LeftToReview(1) => "1 proposal is left to review.".into(),
        Readiness::LeftToReview(n) => format!("{n} proposals are left to review."),
        Readiness::NamesNotReplaced(1) => "1 known name is still written out in the text. \
             It may be a name another document added to the Project."
            .into(),
        Readiness::NamesNotReplaced(n) => format!(
            "{n} known names are still written out in the text. They may be names another \
             document added to the Project."
        ),
        Readiness::ChangedSinceReview => "The text changed since its review.".into(),
        Readiness::NotReviewed if scanned => "It hasn't been reviewed.".into(),
        Readiness::NotReviewed => "Its names haven't been found yet.".into(),
        Readiness::Checking if failed => {
            "Its last change could not be saved, so its names can't be checked.".into()
        }
        Readiness::Checking => "mdoc is still checking its names. Try again in a moment.".into(),
        Readiness::Ready => "Try again in a moment.".into(),
    }
}

pub(super) const EXCLUDED_FROM_AI: &str = "This document is excluded from AI.";
pub(super) const NO_NAMES_REPLACED: &str = "No names in this document are replaced.";

/// A selection's copy that replaces its proposals first.
pub(super) fn replace_and_copy_selection(proposals: usize) -> String {
    format!("Replace {} and Copy", proposals)
}

// ---- The clipboard ------------------------------------------------------------------------------

pub(super) const NO_NAMES_TO_REPLACE: &str =
    "This tab has no names to replace yet. Pseudonymize a document or open a Project first.";
pub(super) const NO_NAMES_TO_RESTORE: &str =
    "This tab has no names to restore yet. Pseudonymize a document or open a Project first.";
pub(super) const CLIPBOARD_HAS_NO_TEXT: &str = "The clipboard has no text.";
/// Where the names come from when the tab has no Project.
pub(super) const THIS_DOCUMENT: &str = "this document";

/// Pseudonymize Clipboard's notice: `count` names replaced from `source`.
pub(super) fn pseudonymized_clipboard(count: usize, source: &str) -> String {
    match count {
        0 => format!("No names from {source} were found in the clipboard. It is unchanged."),
        n => format!(
            "Replaced {n} {} from {source} in the clipboard. Names mdoc doesn't know yet \
             were not replaced; check before pasting.",
            if n == 1 { "name" } else { "names" }
        ),
    }
}

/// Restore Clipboard's notice: names restored and aliases left unrecognized.
pub(super) fn restored_clipboard(restored: usize, unrecognised: usize, source: &str) -> String {
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    match (restored, unrecognised) {
        (0, 0) => {
            format!("No aliases from {source} were found in the clipboard. It is unchanged.")
        }
        (n, 0) => format!(
            "Restored {} from {source} in the clipboard.",
            plural(n, "name", "names")
        ),
        (n, m) => format!(
            "Restored {} from {source} in the clipboard; {} unrecognized, left as is.",
            plural(n, "name", "names"),
            plural(m, "alias", "aliases")
        ),
    }
}

/// Paste AI Response's name, for the refusal below.
pub(super) const RESTORING_AN_AI_REPLY: &str = "Restoring an AI reply";
pub(super) const CLIPBOARD_HAS_NO_REPLY: &str =
    "The clipboard has no text. Copy the AI's reply first.";
/// Why `action` can't restore: the tab has no aliases.
pub(super) fn needs_aliases(action: &str) -> String {
    format!(
        "{action} needs a document whose names were replaced with aliases. \
         Apply replacements, copy the text for the AI, then paste its reply here."
    )
}
