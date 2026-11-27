//! The words a document tab shows: its header and ⋯ menu, the empty page, the
//! find bar, the conversion and password cards, the notices, the Original
//! pane, the comment rail, notes, the What you did popover, spelling and the
//! Model comparison window, each a constant or a function, with their counts
//! and plurals worded here.
use super::comment_rail::Thread;
use crate::ui;
use mdoc_import::comments::Comment;
use mdoc_import::docx_preview::PreviewError;
use std::fmt::Display;

// ---- The document's own page ------------------------------------------------

/// The Replace row's field, before anything is typed.
pub(crate) const REPLACE_WITH: &str = "Replace with";
/// A locked document's password field, before anything is typed.
pub(crate) const PASSWORD: &str = "Password";
/// Ghost text at the caret of an empty document.
pub(crate) const TYPE_HERE: &str = "Type here…";
pub(crate) const REPLACE: &str = "Replace";
pub(crate) const MATCH_CASE: &str = "Match case";
pub(crate) const WHOLE_WORD: &str = "Whole word";
pub(crate) const PREVIOUS_MATCH: &str = "Previous match";
pub(crate) const NEXT_MATCH: &str = "Next match";
pub(crate) const CLOSE_MARKDOWN_SEARCH: &str = "Close Markdown search";
/// The Replace row's button that replaces every match.
pub(crate) const REPLACE_ALL_BUTTON: &str = "Replace All";
/// That button's name for a screen reader and its tooltip.
pub(crate) const REPLACE_ALL: &str = "Replace all";
/// The find bar's field, and the text field's name when it has no other.
pub(crate) const SEARCH_MARKDOWN: &str = "Search Markdown";
/// The notices' titles.
pub(crate) const REVIEW_CONVERSION: &str = "Review conversion";
pub(crate) const TEXT_RECOGNITION_REQUIRED: &str = "Text recognition required";
pub(crate) const PREVIEW: &str = "Preview";
pub(crate) const OCR_SETUP: &str = "OCR setup";
pub(crate) const CLIPBOARD: &str = "Clipboard";
pub(crate) const NEEDS_ATTENTION: &str = "Needs attention";
pub(crate) const NOT_COPIED: &str = "Not copied";
/// What the strip above the text says while work runs on the document.
pub(crate) const SETTING_UP_TEXT_RECOGNITION: &str = "Setting up text recognition…";
pub(crate) const RECOGNIZING_TEXT: &str = "Recognizing text…";
pub(crate) const CONVERTING_TO_MARKDOWN: &str = "Converting to Markdown…";
pub(crate) const LOADING_MARKDOWN: &str = "Loading Markdown…";
pub(crate) const MARKDOWN_UNAVAILABLE: &str = "Markdown unavailable";
pub(crate) const RETRY: &str = "Retry";
/// The Original pane while a locked PDF waits for its password.
pub(crate) const PDF_PASSWORD_PROTECTED: &str =
    "This PDF is password-protected. Enter its password to see it here.";

/// The line under the toolbar of a document whose text came from OCR.
pub(crate) fn recognized_by(
    model: impl Display,
    dpi: impl Display,
    minimum_confidence: impl Display,
) -> String {
    format!(
        "Text recognized by {} · {} DPI · minimum confidence {}",
        model, dpi, minimum_confidence
    )
}

// ---- The header and its ⋯ menu ------------------------------------------------

/// The header's Apply, counting names as the Review tab does (#1314).
pub(crate) fn apply_label(names: usize) -> String {
    format!("Apply {names}")
}

pub(crate) const TRY_AGAIN: &str = "Try again";
pub(crate) const CONVERT_TO_MARKDOWN: &str = "Convert to Markdown";
pub(crate) const PSEUDONYMIZE: &str = "Pseudonymize";
/// Pseudonymize's tooltip.
pub(crate) const FIND_NAMES_TO_REPLACE: &str = "Find names to replace";
pub(crate) const SHARE: &str = "Share";

/// The header's line for what the conversion left out, as a screen reader
/// reads it.
pub(crate) fn left_out(line: impl Display) -> String {
    format!("Left out of the Markdown: {line}")
}

pub(crate) const HIDE_REPLACEMENTS: &str = "Hide replacements";
pub(crate) const SHOW_REPLACEMENTS: &str = "Show replacements";
pub(crate) const HIDE_THE_ORIGINAL_FILE: &str = "Hide the original file";
pub(crate) const SHOW_THE_ORIGINAL_FILE: &str = "Show the original file";
pub(crate) const MORE_ACTIONS: &str = "More actions";
/// Why a menu row is off.
pub(crate) const CONVERT_FIRST: &str = "Convert the document to Markdown first.";
pub(crate) const AVAILABLE_ONCE_LOADED: &str = "Available once the document has loaded.";
pub(crate) const PSEUDONYMIZE_FIRST: &str = "Pseudonymize the document first.";
pub(crate) const APPLY_REPLACEMENTS_FIRST: &str = "Apply replacements first.";
pub(crate) const NO_NAMES_REPLACED: &str = "No names in this document are replaced.";
pub(crate) const ONLY_VAULT_DOCUMENTS_MOVE: &str = "Only a document in the Vault moves.";
/// The ⋯ menu's rows, until one command table words them.
pub(crate) const SAVE: &str = "Save";
pub(crate) const EXPORT_MARKDOWN: &str = "Export Markdown…";
pub(crate) const COPY_MARKDOWN: &str = "Copy Markdown";
pub(crate) const COPY_FOR_AI: &str = "Copy for AI";
pub(crate) const PASTE_AI_REPLY: &str = "Paste AI reply";
pub(crate) const NOTES: &str = "Notes";
pub(crate) const SPELLING: &str = "Spelling";
pub(crate) const RESCAN: &str = "Rescan";
pub(crate) const MOVE_TO_PROJECT: &str = "Move to Project…";
/// The Markdown and Source switch.
pub(crate) const VIEW: &str = "View";
pub(crate) const MARKDOWN: &str = "Markdown";
pub(crate) const SOURCE: &str = "Source";

/// The Spelling row, with how many words are flagged.
pub(crate) fn spelling_count(misspelled: impl Display) -> String {
    format!("Spelling · {misspelled}")
}

// ---- The empty page -------------------------------------------------------------

pub(crate) const DROP_TO_OPEN: &str = "Drop to open";
pub(crate) const NO_SUPPORTED_FILES: &str = "No supported files";
pub(crate) const OPEN_FILES: &str = "Open files…";
pub(crate) const SELECT_SEVERAL: &str = "Select several to open each in its own tab";
pub(crate) const OR_JUST_START_TYPING: &str = "or just start typing";
/// The computer, as the empty page's privacy line names it.
pub(crate) const THIS_MAC: &str = "this Mac";
pub(crate) const THIS_COMPUTER: &str = "this computer";

/// The privacy line's explanation, on hover.
pub(crate) fn runs_on(machine: impl Display) -> String {
    format!(
        "Detection, conversion and Projects run on {machine}. mdoc goes online only \
         to check for updates and to download models you choose."
    )
}

/// The privacy line with its explanation, as a screen reader reads it.
pub(crate) fn stay_on_detail(machine: impl Display, detail: impl Display) -> String {
    format!("Documents stay on {machine}. {detail}")
}

/// The empty page's privacy line.
pub(crate) fn stay_on(machine: impl Display) -> String {
    format!("Documents stay on {machine}")
}

// ---- The conversion card -----------------------------------------------------

pub(crate) const WAITING_TO_CONVERT: &str = "Waiting to convert…";
pub(crate) const CONVERSION_CANCELLED: &str = "Conversion cancelled";
pub(crate) const CONVERSION_NOT_COMPLETED: &str = "Conversion could not be completed";
pub(crate) const CONVERT_TO_BEGIN: &str = "Convert this document to begin editing";
pub(crate) const RECOGNITION_NEEDS_ATTENTION: &str = "Text recognition needs your attention";

/// Why a PDF whose pages all lack text needs recognition, for `count` pages.
pub(crate) fn no_usable_text_all(count: usize) -> String {
    format!(
        "{} usable text. Recognition reads {} on this computer; the original stays in the preview.",
        if count == 1 {
            "The page has no"
        } else {
            "No page has"
        },
        if count == 1 { "it" } else { "them" },
    )
}

/// Why some pages need recognition: `count` of them, at `pages`.
pub(crate) fn no_usable_text(count: usize, pages: impl Display) -> String {
    format!(
        "{count} {} no usable text ({}). Recognition reads {} on this computer; the original stays in the preview.",
        if count == 1 { "page has" } else { "pages have" },
        pages,
        if count == 1 { "it" } else { "them" },
    )
}

pub(crate) const LOCAL_OCR_UNAVAILABLE: &str = "Local OCR is unavailable on this platform.";
pub(crate) const USE_NATIVE_TEXT_ONLY: &str = "Use native text only";
pub(crate) const RUN_OCR: &str = "Run OCR";
/// The model's name on the card when it has none of its own.
pub(crate) const OCR: &str = "OCR";
pub(crate) const DOWNLOAD_AND_RECOGNIZE: &str = "Download & recognize";

/// The recognition hint when the engine cannot read some languages.
pub(crate) fn cannot_read(
    reason: impl Display,
    engine: impl Display,
    languages: impl Display,
) -> String {
    format!("{}. {} OCR cannot read {}.", reason, engine, languages)
}

pub(crate) const RECOGNITION_LANGUAGES: &str = "Recognition languages";
pub(crate) const RECOGNIZE: &str = "Recognize";

/// A language choice on the card, as the tests find it.
pub(crate) fn recognize_selector(label: impl Display) -> String {
    format!("recognize {label}")
}

pub(crate) const FOR_THIS_DOCUMENT_ONLY: &str =
    "For this document only. Default: Settings › Recognition.";
pub(crate) const RECOGNITION_CANCELLED: &str = "Recognition cancelled.";

/// The card's note on a model that is installed.
pub(crate) fn runs_here(name: impl Display) -> String {
    format!("{name} · runs on this computer")
}

/// The card's note on a model still to download, `size` once.
pub(crate) fn downloads_once(name: impl Display, size: impl Display) -> String {
    format!("{name} · {}, once · then runs on this computer", size)
}

/// The card's title by how many pages need recognition.
pub(crate) const THIS_PAGE_NEEDS_RECOGNITION: &str = "This page needs text recognition";
pub(crate) const ALL_PAGES_NEED_RECOGNITION: &str = "All pages need text recognition";
pub(crate) const SOME_PAGES_NEED_RECOGNITION: &str = "Some pages need text recognition";

// ---- The Original pane's status and the notices -------------------------------

pub(crate) const PREPARING_PREVIEW: &str = "Preparing preview…";
pub(crate) const WAITING_TO_PREPARE_PREVIEW: &str = "Waiting to prepare preview…";
pub(crate) const NO_PREVIEW_TITLE: &str = "No preview";
pub(crate) const SHOW: &str = "Show";
pub(crate) const DISMISS_NOTICE: &str = "Dismiss notice";

// ---- Converting and unlocking ---------------------------------------------------

/// The render's scanned file, on the OCR consent card.
#[cfg(all(
    debug_assertions,
    feature = "headless-renders",
    any(target_os = "macos", target_os = "windows")
))]
pub(crate) const SCANNED_AGREEMENT: &str = "Scanned agreement.pdf";
pub(crate) const ANOTHER_MODEL_JOB: &str = "Another model job is running. Retry when it finishes.";
pub(crate) const OCR_MODEL_NOT_READY: &str =
    "Selected OCR model is not ready. Open Settings to download or repair it.";
pub(crate) const PASSWORD_TO_CONVERT: &str = "Enter its password to convert it. mdoc unlocks it in memory while this tab is open and never saves the password or an unlocked copy.";
pub(crate) const PASSWORD_PENDING: &str = "Enter the document’s password to show the Original.";
pub(crate) const PASSWORD_TO_SHOW: &str = "Enter its password to show the Original. mdoc unlocks it in memory while this tab is open and never saves the password or an unlocked copy.";
pub(crate) const PASSWORD_PROTECTED: &str = "This document is password-protected";
pub(crate) const UNLOCK: &str = "Unlock";

// ---- Files in and out -------------------------------------------------------------

/// The window title while a tab shows.
pub(crate) fn window_title(name: impl Display) -> String {
    format!("{} — mdoc", name)
}

pub(crate) const CHOOSE_A_SOURCE: &str = "Choose a Markdown or supported source document.";
pub(crate) const OPEN_ONE_OR_MORE: &str = "Open one or more files";
pub(crate) const EXPORT_EXTENSION: &str =
    "Export Markdown with a .md, .markdown, .mdown, or .txt extension.";
pub(crate) const CHOOSE_YOUR_OWN_FOLDER: &str =
    "Choose a folder of your own: this one holds a copy mdoc removes.";
pub(crate) const PRESERVE_THE_SOURCE: &str =
    "Choose a different path to preserve the imported source.";

pub(crate) fn could_not_export(error: impl Display) -> String {
    format!("Could not export: {error}")
}

/// The system file dialog could not open. On Linux GPUI uses the
/// xdg-desktop-portal file chooser, which minimal systems and WSL lack.
pub(crate) fn chooser_unavailable(error: &impl std::fmt::Display) -> String {
    let hint = if cfg!(target_os = "linux") {
        " Install xdg-desktop-portal with a backend such as xdg-desktop-portal-gtk, \
         or open files from the command line (mdoc path/to/file) or by dropping them."
    } else {
        ""
    };
    format!("The file dialog could not open: {error}.{hint}")
}

// ---- The Original pane --------------------------------------------------------

/// DOCX previews are rendered locally and only approximate Word's layout;
/// the Markdown is converted from the file itself.
pub(crate) const DOCX_NOTE: &str = "Approximate preview: the layout may differ from Word. The Markdown is converted from the file itself.";
/// The preview's find field.
pub(crate) const FIND: &str = "Find…";
pub(crate) const FIND_IN_PREVIEW: &str = "Find in preview";

/// The preview pane's message when the source can't be read, for example
/// because it moved; Retry can succeed once it is back.
pub(crate) const UNREADABLE_SOURCE: &str =
    "mdoc could not read this file. Check that it is still there.";

/// The preview pane's message for a format it can't show.
pub(crate) const NO_PREVIEW: &str =
    "No preview for this format. mdoc previews PDF, DOCX and image files.";

/// The Original's name in its header.
pub(crate) fn source_name(name: impl Display) -> String {
    format!("Source · {name}")
}

/// The header's comments toggle, with how many there are.
pub(crate) fn comments_toggle(count: impl Display) -> String {
    format!("💬 {count}")
}

pub(crate) const HIDE_COMMENTS: &str = "Hide comments";
pub(crate) const SHOW_COMMENTS: &str = "Show comments";

/// The header's hidden text toggle, with how much there is.
pub(crate) fn hidden_text(hidden: impl Display) -> String {
    format!("Hidden text ({hidden})")
}

pub(crate) const HIDE_HIDDEN_TEXT: &str = "Hide hidden text";
pub(crate) const SHOW_HIDDEN_TEXT: &str = "Show hidden text";
pub(crate) const PDF_ENCRYPTION_UNSUPPORTED: &str =
    "This PDF uses encryption mdoc can’t open, so it can’t be previewed.";
pub(crate) const PDF_DAMAGED: &str = "This PDF is damaged or incomplete, so it can't be previewed.";

pub(crate) fn encryption_unsupported(scheme: impl Display) -> String {
    format!("This document uses {scheme} encryption, which mdoc can’t unlock.")
}

/// The one sentence the preview pane shows for a failed DOCX preview. The
/// error's `Display` text keeps the technical detail for tests and diagnostics.
pub(crate) fn docx_message(error: &PreviewError) -> &'static str {
    match error {
        PreviewError::InputTooLarge => {
            "This DOCX is too large to preview. Previews take files up to 50 MB."
        }
        PreviewError::MacrosNotSupported => "DOCX files with macros can't be previewed.",
        // A locked DOCX asks for its password; this is a file in Word's
        // older binary layout, or encrypted in a way mdoc can't open.
        PreviewError::Encrypted => {
            "This DOCX is encrypted or isn’t a Word package mdoc can read, so it can't be previewed."
        }
        PreviewError::TrackedChangesNotSupported => {
            "DOCX files with tracked changes can't be previewed. Accept or reject the changes in Word first."
        }
        PreviewError::NestedArchive => {
            "This DOCX contains other packed files, so it can't be previewed."
        }
        PreviewError::Package(_) => "This DOCX is damaged or incomplete, so it can't be previewed.",
        PreviewError::Render(_) => "The preview could not be drawn.",
        PreviewError::Rejected => "The preview can't draw this DOCX.",
        PreviewError::Timeout => "The preview took longer than 30 seconds and was stopped.",
        PreviewError::Cancelled => "The preview was cancelled.",
        PreviewError::Read(_) => "mdoc could not read this file. Check that it is still there.",
    }
}

// ---- The comment rail -----------------------------------------------------------

/// A comment's date as the card shows it: "Sep 19, 2026".
pub(crate) fn short_date(date: &str) -> Option<String> {
    let mut parts = date.get(..10)?.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .get(month.parse::<usize>().ok()?.checked_sub(1)?)?;
    let day: u32 = day.parse().ok()?;
    year.parse::<u32>().ok()?;
    Some(format!("{month} {day}, {year}"))
}

pub(crate) fn author(comment: &Comment) -> &str {
    if comment.author.trim().is_empty() {
        "Unknown author"
    } else {
        comment.author.trim()
    }
}

pub(crate) fn replies_label(n: usize) -> Option<String> {
    match n {
        0 => None,
        1 => Some("1 reply".into()),
        n => Some(format!("{n} replies")),
    }
}

/// What a screen reader says for a card: "Comment by <author>, <date>, on
/// '<passage>': <text>. N replies."
pub(crate) fn card_label(thread: &Thread) -> String {
    let c = &thread.comment;
    let mut label = format!("Comment by {}", author(c));
    if let Some(date) = short_date(&c.date) {
        label.push_str(&format!(", {date}"));
    }
    let passage: String = c.anchor_text.chars().take(120).collect();
    if !passage.trim().is_empty() {
        label.push_str(&format!(", on '{}'", passage.trim()));
    }
    label.push_str(&format!(": {}", c.text.trim()));
    if !label.ends_with('.') {
        label.push('.');
    }
    if thread.comment.resolved {
        label.push_str(" Resolved.");
    }
    if let Some(replies) = replies_label(thread.replies.len()) {
        label.push_str(&format!(" {replies}."));
    }
    label
}

/// A resolved card's mark.
pub(crate) const RESOLVED: &str = "✓ resolved";
pub(crate) const EMPTY_COMMENT: &str = "Empty comment";

/// The row of comments anchored nowhere on the page, as a screen reader
/// names it.
pub(crate) fn not_on_the_page(count: impl Display) -> String {
    format!("Not on the page ({count})")
}

/// That row, with its open or closed mark.
pub(crate) fn not_on_the_page_row(mark: impl Display, count: impl Display) -> String {
    format!("{} Not on the page ({count})", mark)
}

/// A margin marker for the comments on one line: `card` is the one
/// comment's label, else there are `count`.
pub(crate) fn comments_on_line(count: usize, card: impl FnOnce() -> String) -> String {
    if count == 1 {
        format!("Comment on this line: {}", card())
    } else {
        format!("{} comments on this line", count)
    }
}

// ---- Find and Replace -------------------------------------------------------------

pub(crate) fn replace_all_status(replaced: usize, formatting: usize, alias: usize) -> String {
    let mut status = format!("Replaced {replaced}");
    let mut skipped = Vec::new();
    if formatting > 0 {
        skipped.push(format!("{formatting} (formatting)"));
    }
    if alias > 0 {
        skipped.push(format!("{alias} (alias)"));
    }
    if !skipped.is_empty() {
        status.push_str(" · skipped ");
        status.push_str(&skipped.join(" · "));
    }
    status
}

/// Why Replace left a match.
pub(crate) const SKIPPED_FORMATTING: &str = "Skipped: crosses formatting";
pub(crate) const SKIPPED_ALIAS: &str = "Skipped: alias, change it in Replacements";

/// The find bar's counter: what it shows ("14 / 16") and what a screen reader
/// says ("Match 14 of 16").
pub(crate) fn find_counter(
    empty_query: bool,
    searching: bool,
    active: Option<usize>,
    matches: usize,
) -> (String, gpui::SharedString) {
    if empty_query {
        ("0 / 0".into(), "No search".into())
    } else if searching {
        ("Searching…".into(), "Searching…".into())
    } else if matches == 0 {
        ("0 matches".into(), "No matches".into())
    } else {
        let position = active.map_or(0, |index| index + 1);
        let spoken = if position == 0 {
            format!(
                "{matches} {}",
                if matches == 1 { "match" } else { "matches" }
            )
        } else {
            ui::position_announcement("Match", position, matches)
        };
        (format!("{position} / {matches}"), spoken.into())
    }
}

// ---- Spelling, notes, the What you did popover and Ask --------------------------

/// The editor's right-click rows for a flagged word.
pub(crate) const IGNORE_IN_THIS_DOCUMENT: &str = "Ignore in this document";
pub(crate) const ADD_TO_DICTIONARY: &str = "Add to dictionary";

/// What the tab says with no notes to edit yet.
pub(crate) const EMPTY_HINT: &str =
    "Notes about this document: deadlines, who to call, what to check. Only you see them.";
pub(crate) const NOTES_PRIVATE: &str =
    "Private to this computer. Never sent with Copy for AI or Share.";
pub(crate) const NOTES_UNAVAILABLE: &str = "Notes are kept once the document is saved in mdoc.";
pub(crate) const FROM_A_DELETED_REPLY: &str = "From a deleted AI reply";
pub(crate) const FROM_AI_REPLY: &str = "From AI reply";
pub(crate) const OPEN_THE_REPLY: &str = "From AI reply: open the reply";

/// The footer note: travel takes text edits with it.
pub(crate) const FOOTER: &str = "Text edits made between decisions are undone with them.";
/// A row whose undo point the editor's undo limit dropped.
pub(crate) const PAST_LIMIT: &str = "Older than the undo limit";
pub(crate) const WHAT_YOU_DID: &str = "What you did";
pub(crate) const UNDONE: &str = "Undone";
pub(crate) const UNDO_BACK_TO_HERE: &str = "Undo back to just after this";
pub(crate) const REDO_UP_TO_HERE: &str = "Redo up to here";
pub(crate) const NO_DECISIONS_YET: &str = "Your pseudonymization decisions appear here.";

pub(crate) const MORE_WAYS_TO_ASK: &str = "More ways to ask";

// ---- The Model comparison window ----------------------------------------------

/// A run's warnings when it has none.
pub(crate) const NONE: &str = "none";
/// The threshold or confidence field, before anything is typed.
pub(crate) const UNIT_INTERVAL: &str = "0–1";

/// A finished run's button: its model and how long it took.
pub(crate) fn result_label(name: impl Display, seconds: f64) -> String {
    format!("{} · {:.2}s", name, seconds)
}

pub(crate) const PDF_TOO_LARGE: &str = "PDF comparison supports up to 256 MiB.";
pub(crate) const NEEDS_AN_ORIGINAL_PDF: &str = "OCR comparison requires an original PDF.";

/// A run's OCR settings.
pub(crate) fn ocr_summary(
    model: impl Display,
    dpi: impl Display,
    minimum_confidence: impl Display,
) -> String {
    format!(
        "{} · {} DPI · minimum confidence {} · Force · CPU",
        model, dpi, minimum_confidence
    )
}

/// A run's pseudonymization settings.
pub(crate) fn pii_summary(model: impl Display, threshold: impl Display) -> String {
    format!("{} · threshold {} · CPU", model, threshold)
}

/// The report's renderer, in its OCR identity.
pub(crate) const RENDERER: &str = "PDFium native-v7988";
/// The report's confidence measure, in its pseudonymization identity.
pub(crate) const SPAN_CONFIDENCE: &str = "GLiNER2 span confidence";

/// A pseudonymization run's measurements.
pub(crate) fn candidates(count: impl Display) -> String {
    format!("{count} candidates · confidence and count are not accuracy scores")
}

/// An OCR run's measurements for one page.
pub(crate) fn page_measurements(
    page: impl Display,
    render: impl Display,
    recognition: impl Display,
    preparation: impl Display,
    warnings: impl Display,
) -> String {
    format!(
        "Page {} · render {} ms · recognition {} ms · preparation {} ms · warnings {}",
        page, render, recognition, preparation, warnings
    )
}

/// One detected name in a pseudonymization run's results.
pub(crate) fn prediction(
    text: impl Display,
    category: impl Display,
    confidence: f64,
    start: impl Display,
    end: impl Display,
) -> String {
    format!(
        "{}  [{}]  {:.3}  bytes {}–{}",
        text, category, confidence, start, end
    )
}

pub(crate) const USE_PAGE_NUMBERS: &str = "Use page numbers such as 1, 3-5.";
pub(crate) const INVALID_PAGE_RANGE: &str =
    "Invalid page range; use positive, ascending page numbers.";
pub(crate) const TOO_MANY_PAGES: &str = "Page selection is too large.";
pub(crate) const COMPARISON_CANCELLED: &str = "Comparison cancelled.";
pub(crate) const NOT_ACCURACY_SCORES: &str =
    "Confidence and candidate count are not accuracy scores.";

/// One page of a run's recognition output.
pub(crate) fn page_text(page: impl Display, text: impl Display) -> String {
    format!("Page {}\n{}", page, text)
}

pub(crate) const CONFIGURATION_MISMATCH: &str = "Configuration does not match the captured input.";
pub(crate) const INVALID_REPORT_PATH: &str = "Invalid report path";
pub(crate) const ALL_PAGES: &str = "All pages";
pub(crate) const THRESHOLD_NOT_A_NUMBER: &str =
    "Threshold/confidence must be a number from 0 to 1.";
pub(crate) const THRESHOLD_OUT_OF_RANGE: &str =
    "Threshold/confidence must be finite and between 0 and 1.";

pub(crate) fn model_not_ready(model: impl Display) -> String {
    format!(
        "{} is not ready. Download/repair it in Settings first.",
        model
    )
}

pub(crate) const PARTIAL_RESULTS_DISCARDED: &str =
    "Comparison cancelled; partial results discarded.";
pub(crate) const PRESERVE_DOCUMENT_AND_SOURCE: &str =
    "Choose a different path to preserve the document and its source.";
pub(crate) const COMPARE_OCR_MODELS: &str = "Compare OCR models";
pub(crate) const COMPARE_PII_MODELS: &str = "Compare pseudonymization models";
pub(crate) const NEW_COMPARISON: &str = "New comparison";

pub(crate) fn captured_input(hash: impl Display, snapshot: impl Display) -> String {
    format!("Captured input SHA-256: {} · {}", hash, snapshot)
}

pub(crate) const PDF_SNAPSHOT: &str = "PDF snapshot";
pub(crate) const MARKDOWN_SNAPSHOT: &str = "Complete Markdown snapshot";

/// A run's row, numbered from 1.
pub(crate) fn run_number(number: impl Display) -> String {
    format!("Run {}", number)
}

pub(crate) const MINIMUM_CONFIDENCE: &str = "minimum confidence";
pub(crate) const THRESHOLD: &str = "threshold";

/// A resolution choice, `mark` before it when chosen.
pub(crate) fn dpi(mark: impl Display, dpi: impl Display) -> String {
    format!("{}{} DPI", mark, dpi)
}

pub(crate) const RUN: &str = "Run";
pub(crate) const PAGES: &str = "Pages";
pub(crate) const ALL_PAGES_FROZEN: &str = "All pages · frozen";
pub(crate) const RUN_BOTH: &str = "Run both";
pub(crate) const EXPORT_REPORT: &str = "Export report";
pub(crate) const SAVE_MARKDOWN: &str = "Save Markdown";
pub(crate) const CANCELLING_AFTER_STEP: &str = "Cancelling after the current step…";
pub(crate) const COMPARING_MODELS: &str = "Comparing models…";
pub(crate) const RECOGNITION_OUTPUT: &str = "Recognition output";
pub(crate) const PREPARED_MARKDOWN: &str = "Prepared Markdown";
pub(crate) const RESULTS_READ_ONLY: &str =
    "Results are read-only. Comparing never edits the document or changes defaults.";
