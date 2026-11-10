//! Documents that begin in mdoc (ADR 0109): a new document, named after its
//! first line until the user renames it and discarded when it is left
//! empty, and the sample document, which Try a sample restores. The origin
//! is kept with the document (`documents.json`), so a new document goes on
//! following its first line after mdoc stops.
use super::{Document, DocumentSession, no_document};
use crate::{
    CoreError,
    library::{Library, RenamedDocument},
};
use std::sync::Arc;

/// How a document began, when it began in mdoc.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
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

impl DocumentOrigin {
    pub(crate) fn is_ordinary(&self) -> bool {
        *self == Self::Ordinary
    }
}

/// The longest title a first line gives, in characters.
const MAX_TITLE_CHARS: usize = 60;

/// The title a new document takes from its text (ADR 0109): its first line
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

#[uniffi::export]
impl DocumentSession {
    /// A new document's title follows its first line (ADR 0109): renamed
    /// after `first_line_title` of its text when that changed since it last
    /// followed, as Rename… renames (its file, and its case while the case
    /// holds only it and is named after it), keeping it a new document.
    /// `None` when nothing changed: the line gives no title or the same one,
    /// or the document is not a new one (the user renamed it).
    pub fn follow_first_line(
        &self,
        library: Arc<Library>,
    ) -> Result<Option<RenamedDocument>, CoreError> {
        let _saving = self.lock_saving();
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        if document.origin != DocumentOrigin::New {
            return Ok(None);
        }
        let Some(title) = title_from(&document.review.text) else {
            return Ok(None);
        };
        let unchanged = match &document.followed {
            Some(followed) => *followed == title,
            None => document.title == title,
        };
        if unchanged {
            document.followed = Some(title);
            return Ok(None);
        }
        let renamed = document.rename(&library, &title)?;
        document.followed = Some(title);
        Ok(Some(renamed))
    }

    /// A new document left empty goes (ADR 0109): its file, and its case
    /// when that holds nothing else (no other document, no name, no reply),
    /// as it was made for it. True when it went; the session then holds no
    /// document. Any other document stays, even empty, as does a new one
    /// with text or that the user renamed.
    pub fn discard_if_empty(&self, library: Arc<Library>) -> Result<bool, CoreError> {
        let _saving = self.lock_saving();
        let mut guard = self.lock_document();
        let Some(document) = guard.as_ref() else {
            return Ok(false);
        };
        if document.origin != DocumentOrigin::New || !document.review.text.trim().is_empty() {
            return Ok(false);
        }
        if let Some(path) = &document.path {
            library.delete_document(path.display().to_string())?;
        }
        if let Some(case) = &document.case {
            library.delete_case_if_unused(case)?;
        }
        *guard = None;
        Ok(true)
    }
}

impl Document {
    /// `DocumentSession::rename_document`: the title, the file and the case
    /// as the library renames them; the session keeps the renamed file.
    pub(super) fn rename(
        &mut self,
        library: &Library,
        title: &str,
    ) -> Result<RenamedDocument, CoreError> {
        let lock = library.lock()?;
        let renamed = match &self.path {
            Some(path) => library.rename_locked(&lock, path, title)?,
            None => {
                library.rename_unsaved_locked(&lock, self.case.as_deref(), &self.title, title)?
            }
        };
        if let Some(path) = &renamed.path {
            let path = std::path::PathBuf::from(path);
            self.stored.path = Some(std::path::absolute(&path)?);
            self.path = Some(path);
        }
        self.title = renamed.title.clone();
        Ok(renamed)
    }
}

#[cfg(test)]
mod tests;
