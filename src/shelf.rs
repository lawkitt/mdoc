//! The menu bar shelf's model (ADR 0084), GPUI-free: which documents it
//! lists and whether each is ready, the pseudonymized row it shows, the
//! text Copy for AI and a drag carry, the temporary `.md` exports a drag
//! drops, and Restore Clipboard. The window collects what it lists; the
//! shelf's view shows it.
//!
//! Invariants: nothing leaves through the shelf that is not ready; rows and
//! exports carry pseudonymized text only; no document text goes to app
//! data, and exports live in a private temporary folder until quit.

mod exports;
#[cfg(test)]
mod tests;

pub(crate) use exports::ShelfExports;
use mdoc_pii::restore::{ExportedIdentity, RestoreMap};
use mdoc_store::cases::{Case, CaseLink};
use sha2::Digest;
use std::{
    ops::Range,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// The shelf lists at most this many documents, then "Show all in mdoc".
pub(crate) const MAX_DOCUMENTS: usize = 8;
/// Preview lines under a ready document's title.
const PREVIEW_LINES: usize = 3;
/// A preview line is cut after this many characters.
const PREVIEW_CHARS: usize = 160;
/// A closed member larger than this is not read; it lists as changed.
pub(crate) const MAX_MEMBER_BYTES: u64 = 16 * 1024 * 1024;

/// Whether a listed document may leave through the shelf (ADR 0084).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Readiness {
    /// At least one applied alias and nothing pending: copy and drag.
    Ready,
    /// An open tab with this many proposals still to review.
    LeftToReview(usize),
    /// A closed member whose file is not the text it saved while ready.
    ChangedSinceReview,
    /// A closed member whose file still holds this many of the case's
    /// known names by an original wording.
    NamesNotReplaced(usize),
}

impl Readiness {
    pub(crate) fn is_ready(self) -> bool {
        self == Self::Ready
    }

    /// The row's state, as the row and VoiceOver read it.
    pub(crate) fn label(self) -> String {
        match self {
            Self::Ready => "ready".into(),
            Self::LeftToReview(n) => format!("{n} left to review"),
            Self::ChangedSinceReview => "Changed since review".into(),
            Self::NamesNotReplaced(1) => "1 name not replaced".into(),
            Self::NamesNotReplaced(n) => format!("{n} names not replaced"),
        }
    }
}

/// What a row opens: an open tab, or a closed member of a case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ShelfKey {
    Tab(u64),
    Member { case: String, path: PathBuf },
}

/// One preview line, pseudonymized, with its aliases' byte ranges for chips.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreviewLine {
    pub text: String,
    pub aliases: Vec<Range<usize>>,
}

/// One row of the shelf.
#[derive(Clone, Debug)]
pub(crate) struct ShelfDocument {
    pub key: ShelfKey,
    /// The title with known originals pseudonymized.
    pub title: String,
    pub case: Option<CaseLink>,
    pub readiness: Readiness,
    pub edited: SystemTime,
    pub words: usize,
    /// Up to three lines; only a ready document shows any.
    pub preview: Vec<PreviewLine>,
    /// Copy for AI's text; only a ready document has it.
    for_ai: Option<String>,
}

/// An open tab as the shelf needs it. The window builds this only for tabs
/// with applied aliases.
pub(crate) struct OpenTab<'a> {
    pub id: u64,
    /// The tab's title, as the sidebar shows it (may hold an original).
    pub title: &'a str,
    pub text: &'a str,
    /// Applied aliases in source order, as Copy for AI names them.
    pub aliases: Vec<&'a str>,
    /// Proposals still to review.
    pub pending: usize,
    /// Every name the tab knows, pending proposals included, to mask the
    /// title and preview.
    pub names: Vec<ExportedIdentity>,
    pub case: Option<CaseLink>,
    pub edited: SystemTime,
}

/// A closed member's file, read while the shelf is open.
pub(crate) struct ClosedMember<'a> {
    pub case: &'a Case,
    /// The member as in [`Case::members`].
    pub path: &'a Path,
    /// The file's text; `None` when it could not be read or is too large.
    pub text: Option<&'a str>,
    pub edited: SystemTime,
}

/// The fingerprint of a text saved while ready: SHA-256, hex.
pub(crate) fn fingerprint(text: &str) -> String {
    sha2::Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl ShelfDocument {
    /// An open tab's row; `None` without an applied alias (not listed).
    pub(crate) fn of_tab(tab: OpenTab<'_>) -> Option<Self> {
        if tab.aliases.is_empty() {
            return None;
        }
        let readiness = if tab.pending == 0 {
            Readiness::Ready
        } else {
            Readiness::LeftToReview(tab.pending)
        };
        let names = RestoreMap::new(tab.names);
        let for_ai = readiness
            .is_ready()
            .then(|| mdoc_pii::handoff::for_ai(tab.text, tab.aliases.iter().copied()));
        Some(Self::new(
            ShelfKey::Tab(tab.id),
            tab.title,
            tab.text,
            &names,
            tab.case,
            readiness,
            tab.edited,
            for_ai,
        ))
    }

    /// A closed member's row: ready only while its file matches the
    /// fingerprint saved while ready, and holds none of the case's names.
    pub(crate) fn of_member(member: ClosedMember<'_>) -> Self {
        let names = RestoreMap::new(member.case.exported());
        let title = title_of(member.path);
        let key = ShelfKey::Member {
            case: member.case.id.clone(),
            path: member.path.to_path_buf(),
        };
        let link = Some(CaseLink::of(member.case));
        let Some(text) = member.text else {
            return Self::new(
                key,
                &title,
                "",
                &names,
                link,
                Readiness::ChangedSinceReview,
                member.edited,
                None,
            );
        };
        let missed = names.known_names(text);
        let readiness = if missed > 0 {
            Readiness::NamesNotReplaced(missed)
        } else if member.case.reviewed(member.path) == Some(fingerprint(text).as_str()) {
            Readiness::Ready
        } else {
            Readiness::ChangedSinceReview
        };
        let for_ai = readiness.is_ready().then(|| {
            let tokens = names.tokens(text);
            let aliases = tokens
                .iter()
                .filter(|t| t.identity.is_some())
                .map(|t| &text[t.range.clone()]);
            mdoc_pii::handoff::for_ai(text, aliases)
        });
        Self::new(
            key,
            &title,
            text,
            &names,
            link,
            readiness,
            member.edited,
            for_ai,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        key: ShelfKey,
        title: &str,
        text: &str,
        names: &RestoreMap,
        case: Option<CaseLink>,
        readiness: Readiness,
        edited: SystemTime,
        for_ai: Option<String>,
    ) -> Self {
        let preview = if readiness.is_ready() {
            preview_lines(text, names)
        } else {
            Vec::new()
        };
        Self {
            key,
            title: names.pseudonymize(title).0,
            case,
            readiness,
            edited,
            words: text
                .split_whitespace()
                .filter(|word| word.chars().any(char::is_alphanumeric))
                .count(),
            preview,
            for_ai,
        }
    }

    /// The text Copy for AI and a drag carry; `None` unless ready.
    pub(crate) fn text_for_ai(&self) -> Option<&str> {
        self.for_ai.as_deref().filter(|_| self.readiness.is_ready())
    }

    /// The `.md` file name a drag drops: the pseudonymized title, made safe
    /// for a file name.
    pub(crate) fn file_name(&self) -> String {
        let mut name: String = self
            .title
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
                c if c.is_control() => ' ',
                c => c,
            })
            .collect();
        let trimmed = name.trim().trim_start_matches('.').trim();
        name = trimmed.chars().take(120).collect();
        if name.is_empty() {
            name = "Document".into();
        }
        let lower = name.to_lowercase();
        if !(lower.ends_with(".md") || lower.ends_with(".markdown")) {
            name.push_str(".md");
        }
        name
    }
}

/// A document's title from its path: the file name, as the sidebar shows it.
pub(crate) fn title_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The first lines with text, plain and pseudonymized, with alias ranges.
fn preview_lines(text: &str, names: &RestoreMap) -> Vec<PreviewLine> {
    text.lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['#', '>', '-', '*', '+'])
                .trim()
        })
        .filter(|line| !line.is_empty())
        .take(PREVIEW_LINES)
        .map(|line| {
            let line: String = line.chars().take(PREVIEW_CHARS).collect();
            let text = names.pseudonymize(&line).0;
            let aliases = names
                .tokens(&text)
                .into_iter()
                .filter(|t| t.identity.is_some())
                .map(|t| t.range)
                .collect();
            PreviewLine { text, aliases }
        })
        .collect()
}

/// The shelf's list: open tabs, most recently edited first, then closed
/// members, most recently saved first; at most [`MAX_DOCUMENTS`].
pub(crate) fn list(
    mut tabs: Vec<ShelfDocument>,
    mut members: Vec<ShelfDocument>,
) -> Vec<ShelfDocument> {
    tabs.sort_by_key(|row| std::cmp::Reverse(row.edited));
    members.sort_by_key(|row| std::cmp::Reverse(row.edited));
    tabs.into_iter()
        .chain(members)
        .take(MAX_DOCUMENTS)
        .collect()
}

/// Closed members worth reading for a shelf with `room` rows left: every
/// member of `cases` that is not `open`, whose file exists, most recently
/// modified first. Only their metadata is read here.
pub(crate) fn closed_members<'a>(
    cases: &'a [Case],
    open: &[PathBuf],
    room: usize,
) -> Vec<(&'a Case, &'a Path, SystemTime)> {
    let mut found: Vec<_> = cases
        .iter()
        .flat_map(|case| case.members.iter().map(move |path| (case, path)))
        .filter(|(_, path)| !open.contains(path))
        .filter_map(|(case, path)| {
            let metadata = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
            Some((case, path.as_path(), metadata.modified().ok()?))
        })
        .collect();
    found.sort_by_key(|a| std::cmp::Reverse(a.2));
    found.dedup_by(|a, b| a.1 == b.1);
    found.truncate(room);
    found
}

/// Read a closed member's text while the shelf is open; `None` when it
/// cannot be read as UTF-8 or is larger than [`MAX_MEMBER_BYTES`].
pub(crate) fn read_member(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > MAX_MEMBER_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// What Restore Clipboard did (ADR 0084).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClipboardRestore {
    /// The clipboard text with the case's aliases restored.
    pub text: String,
    /// Distinct names restored, as the reply box counts them.
    pub restored: usize,
    /// Alias-shaped tokens naming none of the case's identities.
    pub unrecognised: usize,
}

impl ClipboardRestore {
    /// "7 names restored · 1 unrecognised".
    pub(crate) fn summary(&self) -> String {
        format!(
            "{} {} restored · {} unrecognised",
            self.restored,
            if self.restored == 1 { "name" } else { "names" },
            self.unrecognised
        )
    }
}

/// Restore Clipboard: replace one case's aliases in `text` with their
/// primary wordings, matching as Restore does. Cases are never mixed.
pub(crate) fn restore_clipboard(text: &str, names: Vec<ExportedIdentity>) -> ClipboardRestore {
    let restored = RestoreMap::new(names).restore_in_place(text);
    ClipboardRestore {
        restored: restored.values,
        unrecognised: restored.unresolved.len(),
        text: restored.text,
    }
}
