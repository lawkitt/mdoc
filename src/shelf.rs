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
use mdoc_markdown::{lines, spans, tables};
use mdoc_pii::{
    Category,
    restore::{ExportedIdentity, RestoreMap},
};
use mdoc_store::cases::{Case, CaseLink};
use sha2::Digest;
use std::{
    ops::Range,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// The shelf lists at most this many documents (ADR 0084).
pub(crate) const MAX_DOCUMENTS: usize = 8;
/// A preview is cut after about this many characters: more than two
/// wrapped lines of the shelf hold, so the view's clamp ends it.
const PREVIEW_CHARS: usize = 220;
/// A line is prose, and may start a preview, with at least this many
/// words and letters; shorter ones are page furniture (page numbers,
/// rules, "Page 1 of 5").
const PROSE_WORDS: usize = 3;
const PROSE_LETTERS: usize = 12;
/// A closed member larger than this is not read; it lists as changed.
pub(crate) const MAX_MEMBER_BYTES: u64 = 16 * 1024 * 1024;

/// Whether a listed document may leave through the shelf (ADR 0084).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Readiness {
    /// At least one applied alias and nothing pending: copy and drag.
    Ready,
    /// An open tab with this many proposals still to review.
    LeftToReview(usize),
    /// An open tab just edited: its proposals are being found again.
    Checking,
    /// A document whose text still holds this many known names by an
    /// original wording: a kept mention in an open tab, or a closed member
    /// that gained one.
    NamesNotReplaced(usize),
    /// A closed member whose file is not the text it saved while ready.
    ChangedSinceReview,
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
            Self::Checking => "Checking names".into(),
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

/// A ready document's preview: its first real prose as one paragraph,
/// Markdown syntax hidden and pseudonymized, ending in "…" when the text
/// goes on. Byte ranges mark what the view draws as alias chips, bold and
/// italic.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Preview {
    pub text: String,
    pub aliases: Vec<Range<usize>>,
    pub bold: Vec<Range<usize>>,
    pub italic: Vec<Range<usize>>,
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
    /// Only a ready document with prose has one.
    pub preview: Option<Preview>,
    /// Distinct names its text holds by an alias, per category, in
    /// [`Category::ALL`] order: "14 replaced" and its breakdown.
    pub replaced: Vec<(Category, usize)>,
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
    /// An edit's proposals are still being found again, so `pending` may
    /// undercount.
    pub checking: bool,
    /// Every name the tab knows, pending and kept ones included: they mask
    /// the title and preview, and any left in the text keeps it from
    /// leaving.
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
        let names = RestoreMap::new(tab.names);
        let readiness = if tab.pending > 0 {
            Readiness::LeftToReview(tab.pending)
        } else if tab.checking {
            Readiness::Checking
        } else {
            match names.known_names(tab.text) {
                0 => Readiness::Ready,
                missed => Readiness::NamesNotReplaced(missed),
            }
        };
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
            // The aliases reopening it relinks as applied, as the editor's
            // Copy for AI would name them.
            mdoc_pii::handoff::for_ai(text, names.exact_aliases(text))
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
        let preview = readiness
            .is_ready()
            .then(|| preview_lines(text, names))
            .flatten();
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
            replaced: replaced(text, names),
            for_ai,
        }
    }

    /// The title as the row shows it: without `.md` (ADR 0084).
    pub(crate) fn display_title(&self) -> &str {
        let lower = self.title.to_ascii_lowercase();
        [".md", ".markdown"]
            .iter()
            .find(|ext| lower.ends_with(*ext) && lower.len() > ext.len())
            .map_or(&self.title, |ext| {
                &self.title[..self.title.len() - ext.len()]
            })
    }

    /// How many distinct names its text replaced.
    pub(crate) fn replaced_count(&self) -> usize {
        self.replaced.iter().map(|(_, n)| n).sum()
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

/// Distinct names `text` holds by an alias, per category.
fn replaced(text: &str, names: &RestoreMap) -> Vec<(Category, usize)> {
    let identities = names.identities();
    let mut seen: Vec<&str> = Vec::new();
    let mut counts = [0usize; Category::ALL.len()];
    for token in names.tokens(text) {
        let Some(identity) = token.identity.and_then(|i| identities.get(i)) else {
            continue;
        };
        // The tab's names list one person in its mapping and its case.
        if seen.contains(&identity.alias.as_str()) {
            continue;
        }
        seen.push(&identity.alias);
        if let Some(at) = Category::ALL.iter().position(|c| *c == identity.category) {
            counts[at] += 1;
        }
    }
    Category::ALL
        .into_iter()
        .zip(counts)
        .filter(|(_, n)| *n > 0)
        .collect()
}

/// Text with byte ranges for bold and italic, built a piece at a time:
/// runs of whitespace collapse to one space and nothing leads with one.
#[derive(Default)]
struct Styled {
    text: String,
    bold: Vec<Range<usize>>,
    italic: Vec<Range<usize>>,
}

fn add_range(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    match ranges.last_mut() {
        Some(last) if last.end == range.start => last.end = range.end,
        _ => ranges.push(range),
    }
}

impl Styled {
    fn push(&mut self, piece: &str, bold: bool, italic: bool) {
        let start = self.text.len();
        for c in piece.chars() {
            if !c.is_whitespace() {
                self.text.push(c);
            } else if !self.text.is_empty() && !self.text.ends_with(' ') {
                self.text.push(' ');
            }
        }
        let end = self.text.len();
        if end > start {
            if bold {
                add_range(&mut self.bold, start..end);
            }
            if italic {
                add_range(&mut self.italic, start..end);
            }
        }
    }

    fn append(&mut self, other: Styled) {
        if !self.text.is_empty() && !self.text.ends_with(' ') {
            self.text.push(' ');
        }
        let shift = self.text.len();
        self.text.push_str(&other.text);
        let moved = |r: Range<usize>| r.start + shift..r.end + shift;
        self.bold.extend(other.bold.into_iter().map(moved));
        self.italic.extend(other.italic.into_iter().map(moved));
    }

    /// Keep the first `len` bytes (a char boundary), clipping the ranges.
    fn truncate(&mut self, len: usize) {
        self.text.truncate(len);
        for ranges in [&mut self.bold, &mut self.italic] {
            ranges.retain_mut(|r| {
                r.end = r.end.min(len);
                r.start < r.end
            });
        }
    }
}

/// One source line with its Markdown syntax hidden: markers and line
/// prefixes removed (escapes and entities as the editor shows them), bold
/// and italic kept as ranges.
fn plain_line(line: &str) -> Styled {
    let mut found = Vec::new();
    spans::scan_line(line, 0, line.len(), &mut found);
    found.sort_by_key(|span| span.range.start);
    let mut out = Styled::default();
    let mut at = 0;
    for span in found {
        if span.range.start < at {
            continue;
        }
        out.push(&line[at..span.range.start], false, false);
        let style = &span.style;
        if style.hide {
            if let Some(shown) = &style.replace {
                out.push(shown, style.bold, style.italic);
            }
        } else {
            out.push(&line[span.range.clone()], style.bold, style.italic);
        }
        at = span.range.end;
    }
    out.push(&line[at..], false, false);
    let len = out.text.trim_end().len();
    out.truncate(len);
    out
}

/// Whether a line reads as prose rather than page furniture.
fn is_prose(text: &str) -> bool {
    let words = text
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count();
    words >= PROSE_WORDS && text.chars().filter(|c| c.is_alphabetic()).count() >= PROSE_LETTERS
}

/// A ready document's preview (ADR 0084): its first prose lines, skipping
/// headings, tables, code, rules and lines too short to be prose, with the
/// Markdown syntax hidden, pseudonymized with every name the document knows,
/// cut after about [`PREVIEW_CHARS`] at a word and ending in "…" when the
/// text goes on. `None` without prose. Never an original: a name the
/// syntax split, which only shows once it is hidden, is replaced too.
pub(crate) fn preview_lines(text: &str, names: &RestoreMap) -> Option<Preview> {
    let mut styled = Styled::default();
    let mut fenced = false;
    let mut more = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced
            || lines::heading_level(trimmed).is_some()
            || lines::thematic_break(trimmed)
            || tables::is_table_row(trimmed)
            || trimmed.starts_with("<!--")
            || trimmed.starts_with("![")
        {
            continue;
        }
        let plain = plain_line(trimmed);
        if !is_prose(&plain.text) {
            continue;
        }
        if styled.text.chars().count() >= PREVIEW_CHARS {
            more = true;
            break;
        }
        styled.append(plain);
    }
    if styled.text.is_empty() {
        return None;
    }
    let (masked, _) = names.pseudonymize(&styled.text);
    if masked != styled.text {
        // Lengths moved: the masked text wins over bold and italic.
        styled = Styled {
            text: masked,
            ..Styled::default()
        };
    }
    if let Some((cut, _)) = styled.text.char_indices().nth(PREVIEW_CHARS) {
        let at_word = styled.text[..cut]
            .rfind(' ')
            .filter(|at| *at > 0)
            .unwrap_or(cut);
        styled.truncate(at_word);
        more = true;
    }
    let mut text = styled.text;
    if more {
        text.push('…');
    }
    let aliases = names
        .tokens(&text)
        .into_iter()
        .filter(|token| token.identity.is_some())
        .map(|token| token.range)
        .collect();
    Some(Preview {
        text,
        aliases,
        bold: styled.bold,
        italic: styled.italic,
    })
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
