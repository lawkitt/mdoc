//! The words the shelf shows: its rows' states, ages and counts, the
//! Clipboard section, the footer, the first-close note and the tray's menu,
//! each a constant or a function, with their counts and plurals worded here.
use super::{ClipboardRestore, Readiness, ShelfDocument};
use mdoc_pii::Category;
use std::{fmt::Display, time::SystemTime};

// ---- A row ------------------------------------------------------------------

impl Readiness {
    /// The row's state, as the row and VoiceOver read it.
    pub(crate) fn label(self) -> String {
        match self {
            Self::Ready => "ready".into(),
            Self::LeftToReview(n) => format!("{n} left to review"),
            Self::Checking => "Checking names".into(),
            Self::ChangedSinceReview => "Changed since review".into(),
            Self::NotReviewed => "Not reviewed".into(),
            Self::NamesNotReplaced(1) => "1 name not replaced".into(),
            Self::NamesNotReplaced(n) => format!("{n} names not replaced"),
        }
    }
}

impl ShelfDocument {
    /// The row's state as it and VoiceOver read it: never scanned reads
    /// apart from a review left unfinished, as its step differs, and one
    /// moved since it was saved Ready says so (#1292).
    pub(crate) fn state(&self) -> String {
        match self.readiness {
            Readiness::NotReviewed if !self.scanned => "Names not looked for yet".into(),
            Readiness::NotReviewed if self.moved => "Moved · open it to share".into(),
            readiness => readiness.label(),
        }
    }
}

/// "just now", "5 min ago", "3 h ago", "yesterday", then a date.
pub(crate) fn ago(edited: SystemTime, now: SystemTime) -> String {
    let seconds = now
        .duration_since(edited)
        .map_or(0, |elapsed| elapsed.as_secs());
    if seconds < 60 {
        return "just now".into();
    }
    if seconds < 3600 {
        return format!("{} min ago", seconds / 60);
    }
    let edited = chrono::DateTime::<chrono::Local>::from(edited);
    let now = chrono::DateTime::<chrono::Local>::from(now);
    let (day, today) = (edited.date_naive(), now.date_naive());
    if day == today {
        format!("{} h ago", seconds / 3600)
    } else if today.pred_opt() == Some(day) {
        "yesterday".into()
    } else if chrono::Datelike::year(&day) == chrono::Datelike::year(&today) {
        edited.format("%-d %b").to_string()
    } else {
        edited.format("%-d %b %Y").to_string()
    }
}

/// "1,240 words".
pub(crate) fn words(count: usize) -> String {
    let digits = count.to_string();
    let mut grouped = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{grouped} {}", if count == 1 { "word" } else { "words" })
}

fn category_noun(category: Category, count: usize) -> &'static str {
    let one = count == 1;
    match category {
        Category::Person if one => "person",
        Category::Person => "people",
        Category::Organization if one => "organization",
        Category::Organization => "organizations",
        Category::Email if one => "email",
        Category::Email => "emails",
        Category::Url if one => "web address",
        Category::Url => "web addresses",
        Category::Phone if one => "phone number",
        Category::Phone => "phone numbers",
        Category::Address if one => "address",
        Category::Address => "addresses",
        Category::Identity if one => "ID",
        Category::Identity => "IDs",
        Category::Tax if one => "tax ID",
        Category::Tax => "tax IDs",
        Category::Bank if one => "bank account",
        Category::Bank => "bank accounts",
        Category::CaseNumber if one => "case number",
        Category::CaseNumber => "case numbers",
        Category::Date if one => "date",
        Category::Date => "dates",
        Category::Other => "other",
    }
}

/// What hovering "24 replaced" lists: the distinct names those mentions
/// are, "14 names: 6 people · 4 organizations · 4 IDs". Each count keeps
/// its noun on its line ("1 phone number"): a long list wraps at the dots.
pub(crate) fn breakdown(replaced: &[(Category, usize)]) -> String {
    let names: usize = replaced.iter().map(|(_, n)| n).sum();
    let categories = replaced
        .iter()
        .map(|(category, n)| format!("{n} {}", category_noun(*category, *n)).replace(' ', "\u{a0}"))
        .collect::<Vec<_>>()
        .join(" · ");
    match names {
        1 => format!("1\u{a0}name: {categories}"),
        n => format!("{n}\u{a0}names: {categories}"),
    }
}

/// A ready row's Copy for AI, and its name for a screen reader.
pub(crate) const COPY_FOR_AI: &str = "Copy for AI";
/// A ready row's meta line: how many mentions its text replaced.
pub(crate) fn replaced_label(replaced: usize) -> String {
    format!("{replaced} replaced")
}
/// A ready row whose Copy for AI was refused as its text was about to leave.
pub(crate) const NOT_COPIED: &str = "Not copied";
/// A row that is not ready, after a Copy for AI on it: its state, and why.
pub(crate) fn not_copied(state: impl Display) -> String {
    format!("Not copied · {state}")
}
/// Under the rows until a drag has dropped.
pub(crate) const DRAG_HINT: &str = "Drag a preview into any app";
/// The rows' place with no document to list.
pub(crate) const NO_DOCUMENTS: &str = "Pseudonymized documents appear here";

/// The name a document leaves under when the Vault gave none: "Document
/// – 2026-11-25", the day `at` falls on in UTC, as `Vault::share_name`
/// dates a creation.
pub(super) fn neutral_name(at: SystemTime) -> String {
    let day = chrono::DateTime::<chrono::Utc>::from(at).format("%Y-%m-%d");
    format!("Document \u{2013} {day}")
}

// ---- The Clipboard section --------------------------------------------------

pub(crate) const CLIPBOARD: &str = "Clipboard";
pub(crate) const PSEUDONYMIZE: &str = "Pseudonymize";
pub(crate) const RESTORE: &str = "Restore";
/// The one button while the Vault is locked.
pub(crate) const UNLOCK_IN_MDOC: &str = "Unlock in mdoc…";
/// The source line with no source to use.
pub(crate) const NAMES_APPEAR: &str = "Names appear once a document is pseudonymized";
/// The ▾'s name for a screen reader.
pub(crate) const CHOOSE_WHOSE_NAMES: &str = "Choose whose names to use";
/// The picker's heading.
pub(crate) const CLIPBOARD_NAMES_FROM: &str = "Clipboard names from";
/// The source line: whose names the buttons use.
pub(crate) fn names_from(source: impl Display) -> String {
    format!("Names from {}", source)
}
/// A clipboard action with no text to act on.
pub(crate) const NO_TEXT: &str = "The clipboard has no text";
/// What Pseudonymize did: how many names it replaced.
pub(crate) fn names_replaced(count: usize) -> String {
    match count {
        0 => "No known names found".to_string(),
        1 => "1 name replaced".to_string(),
        n => format!("{n} names replaced"),
    }
}
impl ClipboardRestore {
    /// "7 names restored · 1 unrecognized".
    pub(crate) fn summary(&self) -> String {
        format!(
            "{} {} restored · {} unrecognized",
            self.restored,
            if self.restored == 1 { "name" } else { "names" },
            self.unrecognised
        )
    }
}

// ---- The footer, Restart to update and the tray's menu ----------------------

pub(crate) const OPEN_MDOC: &str = "Open mdoc";
pub(crate) const SETTINGS: &str = "Settings";
pub(crate) const QUIT: &str = "Quit";
/// The row above the Clipboard section once an update is ready.
pub(crate) fn restart_to_update(version: impl Display) -> String {
    format!("Restart to update to {version}")
}

// ---- The setting and the first-close note -----------------------------------

/// The setting's name, in Settings › General and in what mdoc says about
/// it. The same stored switch (`general.menu_bar`), on by default; the
/// default lives with the store's.
pub(crate) const SETTING: &str = if cfg!(windows) {
    "Keep mdoc in the system tray"
} else {
    "Show mdoc in the menu bar"
};

/// The first-close note's title.
pub(crate) const NOTE_TITLE: &str = if cfg!(windows) {
    "mdoc is still in the system tray."
} else {
    "mdoc is still in the menu bar."
};

pub(crate) const NOTE_TEXT: &str = "Your documents are here.";
pub(crate) const QUIT_WHEN_WINDOWS_CLOSE: &str = "Quit when windows close";
pub(crate) const GOT_IT: &str = "Got it";

/// The notice when the note's Quit when windows close could not save the
/// switch.
pub(crate) fn quit_when_windows_close_not_saved(error: impl Display) -> String {
    format!(
        "Could not save \u{201c}Quit when windows close\u{201d}: {error}. \
         Turn off Settings \u{203a} General \u{203a} {} to try again.",
        SETTING
    )
}

/// The log line when a General choice made outside Settings could not save.
pub(crate) fn general_not_saved(error: impl Display) -> String {
    format!("mdoc: could not save a General choice: {error}")
}
