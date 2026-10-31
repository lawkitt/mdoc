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
/// A preview reads at most this much of one paragraph's source, so a huge
/// paragraph costs no more than a long one (the span scanner also numbers
/// a line's spans in a `u16`).
const PARAGRAPH_BYTES: usize = 32 * 1024;
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
    /// The case's name as the row shows it, pseudonymized with the names
    /// the document knows: a case is often named after a person.
    pub case_label: Option<String>,
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
            case_label: case.as_ref().map(|case| names.pseudonymize(&case.name).0),
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

    /// The card a drag shows under the pointer (ADR 0084): the title as the
    /// row shows it and the preview, both pseudonymized; `None` unless
    /// ready.
    pub(crate) fn drag_card(&self) -> Option<(String, String)> {
        self.readiness.is_ready().then(|| {
            (
                self.display_title().to_string(),
                self.preview
                    .as_ref()
                    .map(|preview| preview.text.clone())
                    .unwrap_or_default(),
            )
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

    /// Remove the bytes in `cuts` (sorted, disjoint, on char boundaries) in
    /// one pass, moving the ranges after each: a long paragraph with many
    /// cuts stays linear.
    fn remove_all(&mut self, cuts: &[Range<usize>]) {
        if cuts.is_empty() {
            return;
        }
        let mut text = String::with_capacity(self.text.len());
        let mut at = 0;
        // Bytes removed before each cut, and in all.
        let mut before = Vec::with_capacity(cuts.len());
        let mut gone = 0;
        for cut in cuts {
            before.push(gone);
            text.push_str(&self.text[at..cut.start]);
            at = cut.end;
            gone += cut.len();
        }
        text.push_str(&self.text[at..]);
        self.text = text;
        let moved = |at: usize| {
            // The cuts wholly before `at`, then one `at` may sit inside.
            let k = cuts.partition_point(|cut| cut.end <= at);
            match cuts.get(k) {
                Some(cut) if cut.start < at => cut.start - before[k],
                Some(_) => at - before[k],
                None => at - gone,
            }
        };
        for ranges in [&mut self.bold, &mut self.italic] {
            ranges.retain_mut(|r| {
                *r = moved(r.start)..moved(r.end);
                r.start < r.end
            });
        }
    }
}

/// Marks where a paragraph's source lines met, while its syntax is hidden
/// over them all; plain text never holds it.
const LINE_MARK: char = '\u{1f}';

/// One paragraph's text with its lines' furniture dropped: a line that
/// is not prose and holds no word of a known name, initials included (page
/// numbers, "Page 1 of 5"). A short line with such a word stays, so a name
/// split over lines is still masked whole.
fn drop_furniture(mut paragraph: Styled, names: &RestoreMap) -> Styled {
    let words: std::collections::HashSet<String> = names
        .identities()
        .iter()
        .flat_map(|identity| identity.originals.iter())
        .flat_map(|original| {
            original
                .split(|c: char| !c.is_alphanumeric())
                // An initial counts too: "J." above "Smith" is half a name.
                .filter(|word| !word.is_empty())
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        })
        .collect();
    let furniture = |line: &str| {
        !is_prose(line)
            && !line
                .split(|c: char| !c.is_alphanumeric())
                .any(|word| words.contains(&word.to_lowercase()))
    };
    let marks: Vec<usize> = paragraph
        .text
        .match_indices(LINE_MARK)
        .map(|(at, _)| at)
        .collect();
    let mark_len = LINE_MARK.len_utf8();
    // Each line's bytes: from after the mark before it to its own mark.
    let starts: Vec<usize> = std::iter::once(0)
        .chain(marks.iter().map(|at| at + mark_len))
        .collect();
    let ends: Vec<usize> = marks
        .iter()
        .copied()
        .chain(std::iter::once(paragraph.text.len()))
        .collect();
    // Each furniture line goes with one mark beside it; overlapping cuts
    // merge, and they apply from the end so earlier offsets hold.
    let mut cuts: Vec<Range<usize>> = (0..starts.len())
        .filter(|&line| furniture(&paragraph.text[starts[line]..ends[line]]))
        .map(|line| {
            if line + 1 < starts.len() {
                starts[line]..starts[line + 1]
            } else if line > 0 {
                ends[line - 1]..ends[line]
            } else {
                0..ends[line]
            }
        })
        .collect();
    cuts.sort_by_key(|cut| cut.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for cut in cuts {
        match merged.last_mut() {
            Some(last) if cut.start <= last.end => last.end = last.end.max(cut.end),
            _ => merged.push(cut),
        }
    }
    paragraph.remove_all(&merged);
    // Each remaining mark goes with a space after it.
    let marks: Vec<Range<usize>> = paragraph
        .text
        .match_indices(LINE_MARK)
        .map(|(at, _)| {
            let end = at + mark_len;
            at..end + usize::from(paragraph.text[end..].starts_with(' '))
        })
        .collect();
    paragraph.remove_all(&marks);
    if paragraph.text.starts_with(' ') {
        paragraph.remove_all(&[Range { start: 0, end: 1 }]);
    }
    let len = paragraph.text.trim_end().len();
    paragraph.truncate(len);
    paragraph
}

/// A line's source without a trailing hard break (`\\`, `<br>`): it joins
/// its paragraph's next line like a space.
fn without_hard_break(line: &str) -> &str {
    for tag in ["<br>", "<br/>", "<br />"] {
        if let Some(rest) = line.strip_suffix(tag) {
            return rest.trim_end();
        }
    }
    let slashes = line.len() - line.trim_end_matches('\\').len();
    if slashes % 2 == 1 {
        return &line[..line.len() - 1];
    }
    line
}

/// `line` with each `<br>` (any case, `<br/>`, `<br />`) read as the space
/// it shows as, so a name broken by one is still masked whole.
fn breaks_as_spaces(line: &str) -> std::borrow::Cow<'_, str> {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("<br") {
        return line.into();
    }
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find("<br") {
        let start = at + found;
        let rest = lower[start + 3..].trim_start();
        let rest = rest.strip_prefix('/').unwrap_or(rest).trim_start();
        if let Some(after) = rest.strip_prefix('>') {
            let end = lower.len() - after.len();
            out.push_str(&line[at..start]);
            out.push(' ');
            at = end;
        } else {
            out.push_str(&line[at..start + 3]);
            at = start + 3;
        }
    }
    out.push_str(&line[at..]);
    out.into()
}

/// Lowercase letters and digits of `text`, its HTML tags left out: how a
/// name reads however it was split.
fn squeezed(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag && c.is_alphanumeric() => out.extend(c.to_lowercase()),
            _ => {}
        }
    }
    out
}

/// Whether `text` still spells out a known name of two words or more,
/// ignoring the spaces, marks and tags between its letters. The preview's
/// last line of defence against a split the masking did not see.
fn spells_an_original(text: &str, names: &RestoreMap) -> bool {
    let text = squeezed(text);
    names
        .identities()
        .iter()
        .flat_map(|identity| identity.originals.iter())
        .filter(|original| original.split_whitespace().nth(1).is_some())
        .map(|original| squeezed(original))
        .any(|original| original.chars().count() >= 4 && text.contains(&original))
}

/// `line` without its leading blockquote markers (each with one space
/// after it), keeping the indentation after them. Four spaces or more
/// before a `>` make code, not a quote.
fn without_quotes(mut line: &str) -> &str {
    loop {
        let rest = line.trim_start_matches(' ');
        if line.len() - rest.len() >= 4 {
            return line;
        }
        let Some(rest) = rest.strip_prefix('>') else {
            return line;
        };
        line = rest.strip_prefix(' ').unwrap_or(rest);
    }
}

/// The columns `text` takes, a tab as four.
fn width(text: &str) -> usize {
    text.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

/// A block a code fence opened in, outermost first.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Container {
    /// A blockquote level: its lines start with `>`.
    Quote,
    /// A list item whose content starts this many columns in: its lines
    /// are blank or indented at least that far.
    Item(usize),
}

/// The containers `line` opens before its content, outermost first.
fn containers_of(mut line: &str) -> Vec<Container> {
    let mut found = Vec::new();
    loop {
        let rest = line.trim_start_matches(' ');
        if let Some(rest) = rest.strip_prefix('>') {
            found.push(Container::Quote);
            line = rest.strip_prefix(' ').unwrap_or(rest);
        } else if let Some((prefix, ..)) = lines::list_prefix(line) {
            let after = &line[prefix..];
            let spaces = after.len() - after.trim_start_matches(' ').len();
            // Up to four more spaces belong to the marker; more make code.
            let extra = if spaces < 4 { spaces } else { 0 };
            found.push(Container::Item(width(&line[..prefix]) + extra));
            line = &after[extra..];
        } else {
            return found;
        }
    }
}

/// `line` inside `containers`, or `None` once one of them has ended: a
/// quote at a line without `>`, a list item at a line indented less than
/// its content (a sibling item, prose after the list).
fn inside<'a>(mut line: &'a str, containers: &[Container]) -> Option<&'a str> {
    for container in containers {
        match container {
            Container::Quote => {
                let rest = line.trim_start_matches(' ').strip_prefix('>')?;
                line = rest.strip_prefix(' ').unwrap_or(rest);
            }
            Container::Item(content) => {
                if line.trim().is_empty() {
                    continue;
                }
                let lead = &line[..line.len() - line.trim_start().len()];
                if width(lead) < *content {
                    return None;
                }
                // Drop the item's indent, whitespace by whitespace.
                let mut columns = 0;
                let cut = line
                    .char_indices()
                    .find(|(_, c)| {
                        if columns >= *content {
                            return true;
                        }
                        columns += if *c == '\t' { 4 } else { 1 };
                        false
                    })
                    .map_or(line.len(), |(at, _)| at);
                line = &line[cut..];
            }
        }
    }
    Some(line)
}

/// An open code fence.
struct Fence {
    /// Its character and length.
    mark: (u8, usize),
    /// The blocks it opened in.
    containers: Vec<Container>,
}

/// One source line with its Markdown syntax hidden: markers and line
/// prefixes removed (escapes and entities as the editor shows them), bold
/// and italic kept as ranges.
fn plain_line(line: &str) -> Styled {
    let mut out = Styled::default();
    plain_into(&mut out, line, (false, false), 0);
    let len = out.text.trim_end().len();
    out.truncate(len);
    out
}

/// Characters that may open inline syntax inside a span's text.
const INLINE_MARKERS: [char; 9] = ['*', '_', '~', '=', '`', '<', '[', '\\', '&'];

/// Hide `text`'s syntax into `out`, under `(bold, italic)`. A link's label
/// comes back as one span, so a span still holding markers is scanned
/// again, a level or two deep, until none show.
fn plain_into(out: &mut Styled, text: &str, (bold, italic): (bool, bool), depth: u8) {
    let mut found = Vec::new();
    spans::scan_line(text, 0, text.len(), &mut found);
    found.sort_by_key(|span| span.range.start);
    let mut at = 0;
    for span in found {
        if span.range.start < at {
            continue;
        }
        out.push(&text[at..span.range.start], bold, italic);
        let style = &span.style;
        let styled = (bold || style.bold, italic || style.italic);
        let piece = &text[span.range.clone()];
        if style.hide {
            if let Some(shown) = &style.replace {
                out.push(shown, styled.0, styled.1);
            }
        } else if depth < 2
            && !style.code
            && span.range != (0..text.len())
            && piece.contains(INLINE_MARKERS)
        {
            plain_into(out, piece, styled, depth + 1);
        } else {
            out.push(piece, styled.0, styled.1);
        }
        at = span.range.end;
    }
    out.push(&text[at..], bold, italic);
}

/// Whether a line reads as prose rather than page furniture.
fn is_prose(text: &str) -> bool {
    let words = text
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count();
    words >= PROSE_WORDS && text.chars().filter(|c| c.is_alphabetic()).count() >= PROSE_LETTERS
}

/// `line` without its blockquote markers (`>`, nested `>>` too).
fn unquoted(line: &str) -> &str {
    unquote(line, usize::MAX)
}

/// `line` without up to `levels` blockquote markers.
fn unquote(mut line: &str, levels: usize) -> &str {
    for _ in 0..levels {
        let Some(prefix) = lines::blockquote_prefix(line) else {
            break;
        };
        // One prefix may hold nested `>>`: count each `>`.
        let markers = line[..prefix].matches('>').count();
        if markers > 1 {
            // Strip one `>` (and a following space) at a time.
            let one = line.find('>').map_or(0, |at| at + 1);
            line = line[one..].strip_prefix(' ').unwrap_or(&line[one..]);
            continue;
        }
        line = line[prefix..].trim_start();
    }
    line
}

/// `line` without its containers' markers: blockquotes (`>`, nested `>>`
/// too, and an alert's `[!NOTE]`), list items (`-`, `1.`) and task boxes,
/// however they nest.
fn block_content(mut line: &str) -> &str {
    loop {
        if let Some(prefix) = lines::blockquote_prefix(line) {
            line = line[prefix..].trim_start();
            if let Some((_, alert, _)) = mdoc_markdown::syntax::alert_prefix(line) {
                line = line[alert..].trim_start();
            }
        } else if let Some((prefix, ..)) = lines::task_prefix(line) {
            line = line[prefix..].trim_start();
        } else if let Some((prefix, ..)) = lines::list_prefix(line) {
            line = line[prefix..].trim_start();
        } else {
            return line;
        }
    }
}

/// `names` with each original also in the wording a preview shows it in:
/// runs of whitespace (non-breaking spaces too) as one space, so a stored
/// "Anna  Petrova" is masked once the preview has collapsed it.
fn preview_names(names: &RestoreMap) -> RestoreMap {
    RestoreMap::new(
        names
            .identities()
            .iter()
            .map(|identity| {
                let mut identity = identity.clone();
                let collapsed: Vec<_> = identity
                    .originals
                    .iter()
                    .map(|original| original.split_whitespace().collect::<Vec<_>>().join(" "))
                    .filter(|collapsed| !identity.originals.iter().any(|o| **o == **collapsed))
                    .collect();
                identity
                    .originals
                    .extend(collapsed.into_iter().map(Into::into));
                identity
            })
            .collect(),
    )
}

/// A code fence's character and length: three or more of `` ` `` or `~`.
fn fence(line: &str) -> Option<(u8, usize)> {
    let c = *line.as_bytes().first()?;
    if c != b'`' && c != b'~' {
        return None;
    }
    let n = line.bytes().take_while(|b| *b == c).count();
    (n >= 3).then_some((c, n))
}

/// A ready document's preview (ADR 0084): its first prose lines, skipping
/// headings, tables, code, rules and lines too short to be prose, with the
/// Markdown syntax hidden, pseudonymized with every name the document knows,
/// cut after about [`PREVIEW_CHARS`] at a word and ending in "…" when the
/// text goes on. `None` without prose. Never an original: a name the
/// syntax split, which only shows once it is hidden, is replaced too.
pub(crate) fn preview_lines(text: &str, names: &RestoreMap) -> Option<Preview> {
    let names = &preview_names(names);
    let mut styled = Styled::default();
    // The paragraph's source, its lines joined by spaces.
    let mut paragraph = String::new();
    let mut fenced: Option<Fence> = None;
    let mut more = false;
    // A paragraph's lines read as one (Markdown joins them), so its syntax
    // is hidden and it is masked whole before it is judged: a name split
    // over two lines, by an entity or by emphasis running on, is never left
    // half shown.
    //
    // A paragraph cut at [`PARAGRAPH_BYTES`] (`capped`) may end in half a
    // name, so it shows only when its text runs well past the preview's
    // length, which keeps that end out of sight; either way it is the last.
    //
    // A paragraph that still spells out a known name once masked, whole
    // and before any cut, gives no preview at all (`refused`).
    let refused = std::cell::Cell::new(false);
    let finish = |paragraph: &mut String, styled: &mut Styled, capped: bool| -> bool {
        let source = std::mem::take(paragraph);
        if source.is_empty() {
            return capped;
        }
        let mut done = drop_furniture(plain_line(&source), names);
        let (masked, _) = names.pseudonymize(&done.text);
        if masked != done.text {
            done = Styled {
                text: masked,
                ..Styled::default()
            };
        }
        if spells_an_original(&done.text, names) {
            refused.set(true);
            return true;
        }
        if !is_prose(&done.text) {
            return capped;
        }
        if styled.text.chars().count() >= PREVIEW_CHARS {
            return true;
        }
        if capped && done.text.chars().count() < 2 * PREVIEW_CHARS {
            return true;
        }
        styled.append(done);
        capped
    };
    // The content column of the list item the text is in, 0 outside lists:
    // code is indented four columns past it.
    let mut item_content = 0;
    for line in text.lines() {
        let trimmed = line.trim();
        // A block inside quotes or list items is what it is without them.
        let block = block_content(trimmed);
        if let Some(Fence {
            mark: (c, n),
            containers,
        }) = &fenced
        {
            match inside(line, containers) {
                // The block the fence opened in has ended, and its code with
                // it: this line is read for what it is outside.
                None => fenced = None,
                Some(inner) => {
                    // Only a fence of the same character, at least as long,
                    // with nothing after it, closes the block; markers
                    // written inside the code are its text.
                    let inner = inner.trim();
                    if fence(inner).is_some_and(|(cc, nn)| {
                        cc == *c && nn >= *n && inner[nn..].trim().is_empty()
                    }) {
                        fenced = None;
                    }
                    continue;
                }
            }
        }
        if fenced.is_none() && !trimmed.is_empty() {
            // Indented code starts only where no paragraph runs on.
            let raw = without_quotes(line);
            let indent = width(&raw[..raw.len() - raw.trim_start().len()]);
            if paragraph.is_empty() && indent < item_content {
                // Back out of the list.
                item_content = 0;
            }
            if paragraph.is_empty() && indent >= item_content + 4 {
                continue;
            }
            if let Some(Container::Item(content)) = containers_of(raw).first() {
                item_content = *content;
            }
        }
        let structure = fence(block).is_some()
            || block.is_empty()
            || lines::heading_level(block).is_some()
            || lines::thematic_break(trimmed)
            || lines::thematic_break(block)
            || tables::is_table_row(block)
            || block.starts_with("<!--")
            || block.starts_with("![");
        // A structure line ends a paragraph, and a list item starts one;
        // a quote's lines run on.
        let item = lines::list_prefix(unquoted(trimmed)).is_some();
        if structure || item {
            more = finish(&mut paragraph, &mut styled, false);
            if more {
                break;
            }
        }
        if let Some(mark) = fence(block) {
            let mut containers = containers_of(line);
            // A fence on a list item's later line is in that item too.
            let raw = without_quotes(line);
            if item_content > 0
                && !containers.iter().any(|c| matches!(c, Container::Item(_)))
                && width(&raw[..raw.len() - raw.trim_start().len()]) >= item_content
            {
                containers.push(Container::Item(item_content));
            }
            fenced = Some(Fence { mark, containers });
            continue;
        }
        if structure {
            continue;
        }
        if !paragraph.is_empty() {
            paragraph.push(' ');
            paragraph.push(LINE_MARK);
            paragraph.push(' ');
        }
        let block = breaks_as_spaces(without_hard_break(block));
        let room = PARAGRAPH_BYTES.saturating_sub(paragraph.len());
        if block.len() >= room {
            // Up to the limit, at a character boundary.
            let mut cut = room;
            while !block.is_char_boundary(cut) {
                cut -= 1;
            }
            paragraph.push_str(&block[..cut]);
            more = finish(&mut paragraph, &mut styled, true);
            break;
        }
        paragraph.push_str(&block);
    }
    if !more {
        more = finish(&mut paragraph, &mut styled, false);
    }
    if refused.get() {
        return None;
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
    if spells_an_original(&text, names) {
        return None;
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
