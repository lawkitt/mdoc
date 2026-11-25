//! The paragraphs an edit changed, scanned at Done as a fresh document is.
//! The review remembers its paragraphs as last scanned, each with its
//! applied aliases read as their originals, so a replacement, a Keep or Not
//! a name never counts as an edit, and an Undo back to scanned text owes
//! nothing. A paragraph is the lines between blank lines: a list or a table
//! is scanned whole, its syntax intact. Kept in memory only.
use super::{Detection, Review};
use std::{collections::HashSet, hash::Hasher, ops::Range, sync::Arc};

/// The paragraphs as last scanned.
#[derive(Clone, Debug, Default)]
pub(crate) enum Scanned {
    /// The review has seen no text yet: the first it sees counts as scanned.
    #[default]
    Unset,
    /// The normalized paragraphs' hashes.
    Paragraphs(HashSet<u64>),
}

/// The changed paragraphs of one text, joined by blank lines for a scan.
pub struct EditedText {
    source: Arc<str>,
    text: String,
    /// Each paragraph's start in `text` and its range in `source`.
    pieces: Vec<(usize, Range<usize>)>,
}

impl EditedText {
    /// What the scan reads.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The changed paragraphs' ranges in the source.
    pub fn ranges(&self) -> impl Iterator<Item = &Range<usize>> {
        self.pieces.iter().map(|(_, range)| range)
    }

    /// `detections` of [`Self::text`] in the source's offsets; one across
    /// two paragraphs is dropped.
    fn place(&self, detections: Vec<Detection>) -> Vec<Detection> {
        detections
            .into_iter()
            .filter_map(|mut detection| {
                let i = self
                    .pieces
                    .partition_point(|(at, _)| *at <= detection.range.start)
                    .checked_sub(1)?;
                let (at, range) = &self.pieces[i];
                (detection.range.end <= at + range.len()).then(|| {
                    let start = range.start + detection.range.start - at;
                    detection.range = start..start + detection.range.len();
                    detection
                })
            })
            .collect()
    }
}

/// `source`'s paragraphs: maximal runs of lines that are not blank, each
/// without its last line break.
pub(crate) fn paragraphs(source: &str) -> Vec<Range<usize>> {
    let mut paragraphs = Vec::new();
    let mut open: Option<Range<usize>> = None;
    let mut at = 0;
    for line in source.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim().is_empty() {
            paragraphs.extend(open.take());
        } else {
            let end = at + content.len();
            match &mut open {
                Some(paragraph) => paragraph.end = end,
                None => open = Some(at..end),
            }
        }
        at += line.len();
    }
    paragraphs.extend(open);
    paragraphs
}

impl Review {
    /// Each paragraph of the review's text with two hashes: of its text as
    /// written, and with its applied aliases read as their originals. A
    /// paragraph counts as scanned when either was: the second covers a
    /// replacement made since, the first an alias relinked as applied
    /// when a document reopens without its review.
    fn normalized_paragraphs(&self) -> Vec<(Range<usize>, [u64; 2])> {
        let applied = self.applied();
        let mut next = 0;
        paragraphs(&self.source)
            .into_iter()
            .map(|paragraph| {
                let mut raw = std::collections::hash_map::DefaultHasher::new();
                raw.write(self.source[paragraph.clone()].as_bytes());
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                let mut at = paragraph.start;
                while let Some(alias) = applied.get(next).filter(|a| a.range.start < paragraph.end)
                {
                    next += 1;
                    if alias.range.start < at || alias.range.end > paragraph.end {
                        continue;
                    }
                    hasher.write(self.source[at..alias.range.start].as_bytes());
                    hasher.write(alias.step.original().as_bytes());
                    at = alias.range.end;
                }
                hasher.write(self.source[at..paragraph.end].as_bytes());
                (paragraph, [raw.finish(), hasher.finish()])
            })
            .collect()
    }

    /// The text as it is now counts as scanned, with what was scanned before.
    pub(crate) fn mark_scanned(&mut self) {
        let hashes = (self.normalized_paragraphs().into_iter()).flat_map(|(_, h)| h);
        match &mut self.scanned {
            Scanned::Paragraphs(scanned) => scanned.extend(hashes),
            Scanned::Unset => self.scanned = Scanned::Paragraphs(hashes.collect()),
        }
    }

    /// The text as it is now counts as scanned if no text has been yet.
    pub(crate) fn settle_scanned(&mut self) {
        if matches!(self.scanned, Scanned::Unset) {
            self.mark_scanned();
        }
    }

    /// Nothing of the text counts as scanned: a document reopened that
    /// may have been left mid-edit owes a scan of all of it.
    pub fn owe_whole_text(&mut self) {
        self.revision.advance();
        self.scanned = Scanned::Paragraphs(HashSet::new());
    }

    /// The paragraphs changed since they were scanned, in source order.
    fn edited_paragraphs(&self) -> Vec<Range<usize>> {
        let Scanned::Paragraphs(scanned) = &self.scanned else {
            return Vec::new();
        };
        self.normalized_paragraphs()
            .into_iter()
            .filter(|(_, hashes)| !hashes.iter().any(|h| scanned.contains(h)))
            .map(|(range, _)| range)
            .collect()
    }

    /// Whether an edit changed a paragraph that has not been scanned since:
    /// until [`Self::ingest_edited`], nothing counts as fully reviewed.
    pub fn edits_unscanned(&self) -> bool {
        !self.edited_paragraphs().is_empty()
    }

    /// The changed paragraphs to scan, as [`Self::ingest_edited`] takes
    /// them; `None` when none changed.
    pub fn edited_text(&self) -> Option<EditedText> {
        let ranges = self.edited_paragraphs();
        if ranges.is_empty() {
            return None;
        }
        let mut text = String::new();
        let pieces = ranges
            .into_iter()
            .map(|range| {
                if !text.is_empty() {
                    text.push_str("\n\n");
                }
                let at = text.len();
                text.push_str(&self.source[range.clone()]);
                (at, range)
            })
            .collect();
        Some(EditedText {
            source: self.source.clone(),
            text,
            pieces,
        })
    }

    /// A scan of `edited` (`detections` in its text's offsets), taken as a
    /// fresh scan's finds; the changed paragraphs count as scanned after.
    /// `Ok(false)`, with nothing taken, when the text changed since
    /// `edited` was read: its paragraphs are owed still.
    pub fn ingest_edited(
        &mut self,
        source: &str,
        edited: &EditedText,
        detections: Vec<Detection>,
    ) -> Result<bool, String> {
        if *edited.source != *source || *self.source != *source {
            return Ok(false);
        }
        self.ingest(source, edited.place(detections))?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "edited_tests.rs"]
mod tests;
