//! Tokenizer-bounded model windows, including pathological-word rejection.
//! Each window comes with its schema+text token count.
//!
//! Windows hold whole paragraphs (issue #302). Fixed-size windows over the
//! whole source cut each repeat of a paragraph at a different place, and a
//! name cut at a window edge ("her husband", "Harrow" of "Harrow & Finch
//! Ltd") became a proposal for every repeat. Each heading starts a window,
//! so a section's windows don't depend on what precedes it. A paragraph
//! alone loses the context the model needs, so short ones share a window.
use super::{DetectError, MAX_INPUT_TOKENS, checkpoint};
use gliner2_rs::{
    Chunker, SchemaTask,
    processor::{SchemaTransformer, WhitespaceTokenSplitter},
};
use std::{ops::Range, sync::atomic::AtomicBool, time::Instant};

pub(super) fn bounded_windows(
    source: &str,
    transformer: &SchemaTransformer,
    tasks: &[SchemaTask],
    cancel: &AtomicBool,
    started: Instant,
) -> Result<Vec<(Range<usize>, usize)>, DetectError> {
    let mut pending = planned_windows(source)?;
    let mut windows = Vec::new();
    // Token counts of windows that fit, by text: a repeated paragraph is
    // tokenized once.
    let mut fits = std::collections::HashMap::new();
    while let Some(range) = pending.pop() {
        checkpoint(cancel, started)?;
        let text = &source[range.clone()];
        if let Some(&tokens) = fits.get(text) {
            windows.push((range, tokens));
            continue;
        }
        let record = transformer
            .transform(text, tasks)
            .map_err(|e| DetectError::Engine(e.to_string()))?;
        if record.input_ids.len() <= MAX_INPUT_TOKENS {
            fits.insert(text, record.input_ids.len());
            windows.push((range, record.input_ids.len()));
            continue;
        }
        // Subdivide using the model's own word splitter, and verify actual
        // schema+text tokens again. Reject a single pathological word rather
        // than truncate it or silently deliver partial detection coverage.
        let words = record.num_words();
        if words < 2 {
            return Err(DetectError::WordTooLong);
        }
        let size = words.div_ceil(2);
        let overlap = (size / 4).min(size - 1);
        pending.extend(chunks(source, range, size, overlap)?);
    }
    windows.sort_by_key(|(range, _)| range.start);
    Ok(windows)
}

/// Word windows before the token check: whole paragraphs packed up to 128
/// words, with each heading starting a window and kept with the paragraph
/// after it. A longer paragraph packs its own lines (list items, table rows)
/// the same way, and only a longer line gets overlapping windows, counted
/// from its own start. So a section is cut the same way wherever it sits.
/// Words come from the model's own splitter, as `Chunker` counts them, run
/// once over the source.
pub(super) fn planned_windows(source: &str) -> Result<Vec<Range<usize>>, DetectError> {
    let words: Vec<(usize, usize)> = WhitespaceTokenSplitter::new()
        .map_err(|e| DetectError::Engine(e.to_string()))?
        .split_with_offsets(source)
        .into_iter()
        .map(|(_, start, end)| (start, end))
        .collect();
    let index = |byte: usize| words.partition_point(|&(start, _)| start < byte);
    // Units of word indices: a paragraph with the headings before it, and
    // whether it has any (it starts a section).
    let mut units = Vec::new();
    let mut headings = None;
    for (block, heading) in paragraphs(source) {
        let begin = headings.take();
        let start = begin.unwrap_or(index(block.start));
        if heading {
            headings = Some(start);
        } else {
            units.push((start..index(block.end), begin.is_some()));
        }
    }
    units.extend(headings.map(|begin| (begin..words.len(), true)));
    let lines = |unit: &Range<usize>| {
        let mut at = words[unit.start].0;
        source[at..words[unit.end - 1].1]
            .split_inclusive('\n')
            .map(|line| {
                let range = index(at)..index(at + line.len());
                at += line.len();
                (range, false)
            })
            .collect::<Vec<_>>()
    };
    let windows = pack(units, |paragraph, windows| {
        windows.extend(pack(lines(&paragraph), |line, windows| {
            let mut start = line.start;
            loop {
                let end = (start + WINDOW_WORDS).min(line.end);
                windows.push(start..end);
                if end == line.end {
                    break;
                }
                start += WINDOW_WORDS - 32;
            }
        }));
    });
    Ok(windows
        .into_iter()
        .map(|range| words[range.start].0..words[range.end - 1].1)
        .collect())
}

const WINDOW_WORDS: usize = 128;

/// Packs consecutive `units` (word index ranges) into windows of at most
/// [`WINDOW_WORDS`], each flagged unit starting a window; `long` cuts a unit
/// that alone is longer.
fn pack(
    units: Vec<(Range<usize>, bool)>,
    mut long: impl FnMut(Range<usize>, &mut Vec<Range<usize>>),
) -> Vec<Range<usize>> {
    let mut windows = Vec::new();
    let mut open: Option<Range<usize>> = None;
    for (unit, fresh) in units.into_iter().filter(|(unit, _)| !unit.is_empty()) {
        if let Some(window) = &mut open {
            if !fresh && unit.end - window.start <= WINDOW_WORDS {
                window.end = unit.end;
                continue;
            }
            windows.extend(open.take());
        }
        if unit.len() <= WINDOW_WORDS {
            open = Some(unit);
        } else {
            long(unit, &mut windows);
        }
    }
    windows.extend(open);
    windows
}

fn chunks(
    source: &str,
    range: Range<usize>,
    size: usize,
    overlap: usize,
) -> Result<impl Iterator<Item = Range<usize>>, DetectError> {
    let chunks = Chunker::new(size, overlap)
        .map_err(|e| DetectError::Engine(e.to_string()))?
        .split(&source[range.clone()])
        .map_err(|e| DetectError::Engine(e.to_string()))?;
    Ok(chunks
        .into_iter()
        .map(move |chunk| range.start + chunk.byte_start..range.start + chunk.byte_end))
}

/// Paragraphs: runs of lines between blank lines, with each ATX heading a
/// paragraph of its own (flagged). Blank runs are skipped.
fn paragraphs(source: &str) -> Vec<(Range<usize>, bool)> {
    let mut blocks = Vec::new();
    let mut open: Option<Range<usize>> = None;
    let mut start = 0;
    for line in source.split_inclusive('\n') {
        let range = start..start + line.len();
        start = range.end;
        let text = line.trim();
        if text.is_empty() || heading(text) {
            blocks.extend(open.take().map(|block| (block, false)));
            if !text.is_empty() {
                blocks.push((range, true));
            }
            continue;
        }
        match &mut open {
            Some(block) => block.end = range.end,
            None => open = Some(range),
        }
    }
    blocks.extend(open.map(|block| (block, false)));
    blocks
}

/// `#` to `######` followed by a space or the end of the line.
fn heading(line: &str) -> bool {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&hashes)
        && line[hashes..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    #[test]
    fn paragraphs_split_at_blank_lines_and_headings() {
        let source = "# Title\nIntro line one\nline two\n\n\n## Parties\n1. A\n2. B\n   \n#hashtag text\nlast";
        let blocks: Vec<_> = paragraphs(source)
            .into_iter()
            .map(|(r, heading)| (&source[r], heading))
            .collect();
        assert_eq!(
            blocks,
            [
                ("# Title\n", true),
                ("Intro line one\nline two\n", false),
                ("## Parties\n", true),
                ("1. A\n2. B\n", false),
                ("#hashtag text\nlast", false)
            ]
        );
        assert!(paragraphs("\n \n").is_empty());
    }

    #[test]
    fn windows_pack_whole_paragraphs_and_cut_only_long_lines() {
        let long = "word ".repeat(300);
        let list: String = (0..30)
            .map(|n| format!("{n}. Item {n} of the list.\n"))
            .collect();
        let source = format!(
            "# Title\n\nShort one.\n\nTwo.\n\n{long}\n\n{list}\n## Last\n\n🙂 Анна конец\n\n## Trailing"
        );
        let ranges = planned_windows(&source).unwrap();
        let windows: Vec<_> = ranges.iter().map(|r| &source[r.clone()]).collect();
        assert_eq!(windows[0], "# Title\n\nShort one.\n\nTwo.");
        // 300 words on one line: three overlapping windows that cover it.
        let first = source.find("word").unwrap();
        assert_eq!(ranges[1].start, first);
        assert_eq!(ranges[3].end, first + long.trim_end().len());
        assert!(ranges[1..4].windows(2).all(|w| w[1].start < w[0].end));
        // A list longer than a window is packed by whole items.
        assert!(windows[4].starts_with("0. Item 0") && windows[4].ends_with("list."));
        assert!(windows[5].starts_with(char::is_numeric) && windows[5].ends_with("list."));
        // Each heading starts a window.
        assert_eq!(windows[6..], ["## Last\n\n🙂 Анна конец", "## Trailing"]);
    }

    /// The qualification set's long settlement: one section's text, unmarked.
    pub(crate) fn settlement_section() -> String {
        let corpus: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/pseudonymization/corpus.json"
        ))
        .unwrap();
        let fixture = corpus
            .iter()
            .find(|f| f["id"] == "en-settlement-long")
            .unwrap();
        let marks = regex::Regex::new(r"⟦[A-Z]+\|([^⟧]+)⟧").unwrap();
        marks
            .replace_all(fixture["annotated"].as_str().unwrap(), "$1")
            .into_owned()
    }

    #[test]
    fn no_window_cuts_a_paragraph_or_line_of_a_long_document() {
        let long = settlement_section().repeat(40);
        let lines: Vec<_> = long
            .split_inclusive('\n')
            .scan(0, |at, line| {
                let start = *at + (line.len() - line.trim_start().len());
                *at += line.len();
                Some(start..start + line.trim().len())
            })
            .filter(|line| !line.is_empty())
            .collect();
        let starts: std::collections::HashSet<_> = lines.iter().map(|l| l.start).collect();
        let ends: std::collections::HashSet<_> = lines.iter().map(|l| l.end).collect();
        let windows = planned_windows(&long).unwrap();
        for window in &windows {
            assert!(
                starts.contains(&window.start),
                "{:?}",
                &long[window.clone()]
            );
            assert!(ends.contains(&window.end), "{:?}", &long[window.clone()]);
        }
        assert!(windows.windows(2).all(|w| w[0].end < w[1].start));
        // Paragraphs that fit a window are never split.
        let splitter = WhitespaceTokenSplitter::new().unwrap();
        for (block, _) in paragraphs(&long) {
            if splitter.split_with_offsets(&long[block.clone()]).len() <= WINDOW_WORDS {
                let text = long[block.clone()].trim();
                let start = block.start + long[block].find(text).unwrap();
                assert!(
                    windows
                        .iter()
                        .any(|w| w.start <= start && start + text.len() <= w.end),
                    "{text:?}"
                );
            }
        }
        // Every section gets the windows it gets alone.
        let section = settlement_section();
        let alone: Vec<_> = planned_windows(&section)
            .unwrap()
            .into_iter()
            .map(|r| &section[r])
            .collect();
        let repeated: Vec<_> = windows.iter().map(|r| &long[r.clone()]).collect();
        assert_eq!(repeated, alone.repeat(40));
        let bodies = paragraphs(&section).iter().filter(|(_, h)| !h).count();
        assert!(alone.len() < bodies, "paragraphs share windows");
    }
}
