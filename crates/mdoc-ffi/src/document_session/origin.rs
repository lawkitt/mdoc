//! Documents that begin in mdoc: a new document, named after its
//! first line until the user renames it and discarded when it is left
//! empty, and the sample document, which Try a sample restores. The origin
//! is kept with the document's Vault record, so a new document goes on
//! following its first line after mdoc stops
//! (`DocumentSession::follow_first_line_in_vault`,
//! `discard_if_empty_in_vault`).

/// How a document began, when it began in mdoc.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum DocumentOrigin {
    /// Imported, pasted or scanned, or a new document the user renamed.
    #[default]
    Ordinary,
    /// New document: named after its first line until the user renames it;
    /// discarded when it is left empty.
    New,
    /// Try a sample's document, which Try a sample restores.
    Sample,
}

/// The longest title a first line gives, in characters.
const MAX_TITLE_CHARS: usize = 60;

/// The title a new document takes from its text: its first line
/// with words, as the line reads rendered, without a heading's `#`, a
/// quote's `>`, a list's or task's marker or the inline markers (`**`, a
/// link's target), its spaces collapsed, trimmed, and at most 60
/// characters, cut at a word when it can be. Blank lines, fences and rules
/// are passed over; a table row gives its cells. `None` when no line has
/// words.
#[uniffi::export]
pub fn first_line_title(text: String) -> Option<String> {
    title_from(&text)
}

pub(crate) fn title_from(text: &str) -> Option<String> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .find_map(line_title)
}

fn line_title(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("```")
        || trimmed.starts_with("~~~")
        || mdoc_markdown::lines::thematic_break(line)
        || mdoc_markdown::tables::is_table_separator(line)
    {
        return None;
    }
    let words = if mdoc_markdown::tables::is_table_row(line) {
        mdoc_markdown::tables::table_cells(line)
            .iter()
            .map(|cell| shown(cell))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        shown(line)
    };
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    (!words.is_empty()).then(|| shortened(&words))
}

/// `line` as the rendered view shows it: every byte but the hidden markers.
fn shown(line: &str) -> String {
    let mut spans = Vec::new();
    mdoc_markdown::spans::scan_line(line, 0, line.len(), &mut spans);
    let mut hidden: Vec<_> = spans
        .into_iter()
        .filter(|span| span.style.hide)
        .map(|span| span.range)
        .collect();
    hidden.sort_by_key(|range| range.start);
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    for range in hidden {
        if range.start > at {
            out.push_str(&line[at..range.start]);
        }
        at = at.max(range.end);
    }
    if at < line.len() {
        out.push_str(&line[at..]);
    }
    out
}

/// At most `MAX_TITLE_CHARS` characters, cut after the last whole word when
/// that keeps at least half of them.
fn shortened(words: &str) -> String {
    let Some((cut, _)) = words.char_indices().nth(MAX_TITLE_CHARS) else {
        return words.to_owned();
    };
    let head = &words[..cut];
    let at_word = match head.rfind(' ') {
        Some(space) if head[..space].chars().count() >= MAX_TITLE_CHARS / 2 => &head[..space],
        _ => head,
    };
    at_word.trim_end().to_owned()
}

#[cfg(test)]
mod tests;
