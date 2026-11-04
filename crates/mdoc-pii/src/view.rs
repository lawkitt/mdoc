//! The text detection and readiness read (ADR 0092, preprocessing): the
//! source with what only hides a value's characters undone, and a map from
//! its offsets back to the source's. HTML character references are decoded
//! ("Ann&#97; Petrova" reads "Anna Petrova"), soft hyphens and zero-width
//! characters drop out ("Ма\u{ad}рина" reads "Марина") and letters written
//! with combining marks compose, as NFC writes them ("е\u{308}" reads "ё").
//! A table cell under an identifier column's header reads with it
//! ("Паспорт: 46 82 731609"); the header is inserted, so a span only over it
//! maps to an empty source range. Letter-spaced words read whole ("И Н Н"
//! reads "ИНН") and footnote references drop out (`**ИНН**[^1]:` reads
//! `**ИНН**:`). The source itself never changes: spans found in the view map back to
//! whole source characters. Adjacent emphasis runs read across their gap
//! (`*Pellam* *Industries Inc*`), and inline `<sup>`/`<sub>` tags read as
//! their inner text. Ordinals glue (`26<sup>th</sup>` reads `26th`);
//! other superscripts/subscripts start a separate word (`Durnell<sup>1</sup>`).
use std::ops::Range;
use unicode_normalization::UnicodeNormalization;

/// The source as detection reads it, with its offsets mapped back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// The text the model and the rules read.
    pub text: String,
    /// Each rewritten source range and the view range it became, in order.
    edits: Vec<Edit>,
    pub(super) markup: Vec<InlineMarkup>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    source: Range<usize>,
    view: Range<usize>,
}

impl View {
    pub fn new(source: &str) -> Self {
        Self::read(source, inline_markup(source))
    }

    fn read(source: &str, markup: Vec<InlineMarkup>) -> Self {
        let mut rewrites = rewrites(source, &markup);
        let spaced: Vec<_> = letter_spacing(source)
            .into_iter()
            .filter(|(range, _)| {
                let index = rewrites.partition_point(|(other, _)| other.end <= range.start);
                rewrites
                    .get(index)
                    .is_none_or(|(other, _)| range.end <= other.start)
            })
            .collect();
        rewrites.extend(spaced);
        rewrites.sort_by_key(|(range, _)| (range.start, range.end));
        let headers = table_headers(source);
        // A header goes in only between rewritten parts, never inside one.
        let inside = |at: usize| {
            let index = rewrites.partition_point(|(range, _)| range.end <= at);
            rewrites
                .get(index)
                .is_some_and(|(range, _)| range.start < at)
        };
        let headers: Vec<_> = headers
            .into_iter()
            .filter(|(range, _)| !inside(range.start))
            .collect();
        rewrites.extend(headers);
        Self::apply(source, markup, rewrites)
    }

    fn apply(
        source: &str,
        markup: Vec<InlineMarkup>,
        mut rewrites: Vec<(Range<usize>, String)>,
    ) -> Self {
        let mut text = String::with_capacity(source.len());
        let mut edits = Vec::new();
        let mut copied = 0;
        // An insertion sorts before a rewrite starting where it does.
        rewrites.sort_by_key(|(range, _)| (range.start, range.end));
        for (range, replacement) in rewrites {
            text.push_str(&source[copied..range.start]);
            let start = text.len();
            text.push_str(&replacement);
            edits.push(Edit {
                source: range.clone(),
                view: start..text.len(),
            });
            copied = range.end;
        }
        text.push_str(&source[copied..]);
        Self {
            text,
            edits,
            markup,
        }
    }

    /// Only this PR's emphasis gaps and ordinal tags, with the complete
    /// markup cache retained for source-span validation. Other view rewrites
    /// must not broaden discovery of an existing wording.
    pub(super) fn for_discovery(source: &str) -> Self {
        Self::emphasis(source, true)
    }

    fn emphasis(source: &str, check_roles: bool) -> Self {
        let markup = inline_markup_with(source, check_roles);
        let edits = markup
            .iter()
            .filter(|m| m.inner.is_none() || m.ordinal)
            .flat_map(|m| m.edits.iter().cloned())
            .collect();
        Self::apply(source, markup, edits)
    }

    /// Whether the view reads the source unchanged.
    pub fn is_source(&self) -> bool {
        self.edits.is_empty()
    }

    /// The source range `range` of the view stands for. A boundary inside a
    /// rewritten part widens to all of it, so a span covers whole source
    /// characters ("&#97;", never "&#9").
    pub fn source_range(&self, range: Range<usize>) -> Range<usize> {
        let mut source =
            self.source_offset(range.start, false)..self.source_offset(range.end, true);
        // Only a pair starting at/before the start (within its opening tag),
        // or inside this span, can have its complete inner text covered.
        let from = self
            .markup
            .partition_point(|m| m.range.start < source.start)
            .saturating_sub(1);
        let to = self.markup.partition_point(|m| m.range.start < source.end);
        for markup in &self.markup[from..to] {
            if let Some(inner) = &markup.inner
                && source.start <= inner.start
                && inner.end <= source.end
            {
                source.start = source.start.min(markup.range.start);
                source.end = source.end.max(markup.range.end);
            }
        }
        source
    }

    /// The view range source `range` reads as: the inverse of
    /// [`View::source_range`], widened the same way, so a span found in the
    /// source can be read as detection read it ("%41lice" in a link target
    /// reads "Alice").
    pub fn view_range(&self, range: Range<usize>) -> Range<usize> {
        self.view_offset(range.start, false)..self.view_offset(range.end, true)
    }

    fn view_offset(&self, at: usize, end: bool) -> usize {
        let index = self
            .edits
            .partition_point(|edit| edit.source.start < at || (!end && edit.source.start == at));
        let Some(edit) = index.checked_sub(1).map(|i| &self.edits[i]) else {
            return at;
        };
        if at < edit.source.end || (end && at == edit.source.end) {
            if end { edit.view.end } else { edit.view.start }
        } else {
            edit.view.end + (at - edit.source.end)
        }
    }

    fn source_offset(&self, at: usize, end: bool) -> usize {
        // The last edit starting before `at` (an end) or at it (a start).
        let index = self
            .edits
            .partition_point(|edit| edit.view.start < at || (!end && edit.view.start == at));
        let Some(edit) = index.checked_sub(1).map(|i| &self.edits[i]) else {
            return at;
        };
        if at < edit.view.end || (end && at == edit.view.end) {
            if end {
                edit.source.end
            } else {
                edit.source.start
            }
        } else {
            edit.source.end + (at - edit.view.end)
        }
    }
}

/// `text` as detection reads it.
pub fn decoded(text: &str) -> std::borrow::Cow<'_, str> {
    let view = View::new(text);
    if view.is_source() {
        std::borrow::Cow::Borrowed(text)
    } else {
        std::borrow::Cow::Owned(view.text)
    }
}

/// Undo only emphasis-run joins and ordinal sup/sub tags. Entities, line
/// breaks, underscores outside a joined gap and all other view rewrites stay
/// byte-for-byte. A validated mention's outer emphasis delimiters
/// belong to the surrounding document and may be absent from this fragment
/// (`Pellam* *Rooke* *Ltd`), so delimiter roles must be checked against the
/// complete source before calling this, not inferred again from the fragment.
pub(super) fn wording(text: &str) -> std::borrow::Cow<'_, str> {
    let view = View::emphasis(text, false);
    if view.is_source() {
        std::borrow::Cow::Borrowed(text)
    } else {
        std::borrow::Cow::Owned(view.text)
    }
}

/// Source ranges and what the view reads for each, in order, never
/// overlapping.
fn rewrites(source: &str, markup: &[InlineMarkup]) -> Vec<(Range<usize>, String)> {
    let mut markup: Vec<_> = markup
        .iter()
        .flat_map(|m| m.edits.iter().cloned())
        .collect();
    markup.sort_by_key(|(r, _)| r.start);
    let mut markup = markup.iter().peekable();
    let mut out = Vec::new();
    let destinations = link_destinations(source);
    let opaque: Vec<Range<usize>> = destinations
        .iter()
        .flat_map(|destination| opaque_parts(source, destination))
        .collect();
    let mut opaque = opaque.into_iter().peekable();
    let mut destination = destinations.iter().peekable();
    let mut chars = source.char_indices().peekable();
    let mut previous = None;
    while let Some((at, c)) = chars.next() {
        let next = chars.peek().map(|&(_, next)| next);
        let before = previous;
        previous = Some(c);
        while destination.next_if(|range| range.end <= at).is_some() {}
        let hidden = destination.peek().filter(|range| range.start <= at);
        while opaque.next_if(|part| part.end <= at).is_some() {}
        if let Some((range, replacement)) = markup.next_if(|(range, _)| range.start == at) {
            out.push((range.clone(), replacement.clone()));
            while chars.next_if(|&(i, _)| i < range.end).is_some() {}
            previous = replacement.chars().next_back().or(before);
        } else if let Some(part) = opaque.next_if(|part| part.start <= at) {
            // "consultantplus://offline/ref=0A1B…": not the document's text.
            previous = None;
            while chars.next_if(|&(i, _)| i < part.end).is_some() {}
            out.push((at..part.end, String::new()));
        } else if hidden.is_some()
            && c == '%'
            && let Some((len, value)) = percent_encoded(&source[at..])
        {
            previous = value.chars().next_back();
            out.push((at..at + len, value));
            while chars.next_if(|&(i, _)| i < at + len).is_some() {}
        } else if let Some(link) = hidden
            && c == '_'
            && before.is_some_and(char::is_alphabetic)
            && (next.is_some_and(char::is_alphabetic)
                || percent_encoded(&source[at + 1..])
                    .is_some_and(|(_, value)| value.starts_with(char::is_alphabetic)))
            && !in_email(source, link, at)
        {
            // "clients/Alice_Morgan" names Alice Morgan.
            out.push((at..at + 1, " ".into()));
        } else if c == '_'
            && hidden.is_none()
            && before != Some('_')
            && let run = source[at..].len() - source[at..].trim_start_matches('_').len()
            && run <= 3
            && let after = source[at + run..].chars().next()
            && (!before.is_some_and(char::is_alphanumeric)
                && after.is_some_and(char::is_alphabetic)
                || before.is_some_and(char::is_alphabetic)
                    && !after.is_some_and(char::is_alphanumeric))
            && !in_address(source, at)
        {
            // "_Alva Ruskett_", "__ИНН__:": emphasis marks are syntax, not
            // the name's.
            out.push((at..at + run, String::new()));
            while chars.next_if(|&(i, _)| i < at + run).is_some() {}
        } else if c == '['
            && hidden.is_none()
            && let Some(len) = footnote_reference(source, at)
        {
            // "**ИНН**[^1]:" reads "**ИНН**:", a label before its value;
            // "Durnell[^2]" reads "Durnell".
            out.push((at..at + len, String::new()));
            while chars.next_if(|&(i, _)| i < at + len).is_some() {}
            previous = before;
        } else if note_marker(c) && before.is_some_and(char::is_alphabetic) {
            // "Avelina Durnell¹": a note marker is a word of its own.
            out.push((at..at + c.len_utf8(), format!(" {c}")));
        } else if c == '-'
            && before.is_some_and(char::is_alphabetic)
            && let Some(len) = line_end_hyphen(&source[at..])
        {
            out.push((at..at + len, String::new()));
            while chars.next_if(|&(i, _)| i < at + len).is_some() {}
        } else if c == '@'
            && before.is_some_and(char::is_alphanumeric)
            && let Some(len) = line_break(&source[at + 1..])
        {
            // An email wrapped after its "@" ("ksen.rav@\nsvod.invalid").
            out.push((at + 1..at + 1 + len, String::new()));
            while chars.next_if(|&(i, _)| i <= at + len).is_some() {}
            previous = None;
        } else if c == '<'
            && let Some(len) = line_break_tag(&source[at..])
            && !source[at + len..].starts_with(['\r', '\n'])
        {
            // "27 Wisp Lane<br>Norwich" reads "27 Wisp Lane, Norwich", one
            // value across the lines of its cell. At a line's end the line
            // break is already there, and the tag stays.
            out.push((at..at + len, ", ".into()));
            while chars.next_if(|&(i, _)| i < at + len).is_some() {}
            previous = None;
        } else if c == '&'
            && let Some((len, value)) = character_reference(&source[at..])
        {
            // A full-width form written as a reference reads as ASCII too.
            let value = narrowed(value);
            previous = value.chars().next_back();
            out.push((at..at + len, value));
            while chars.next_if(|&(i, _)| i < at + len).is_some() {}
        } else if invisible(c) {
            out.push((at..at + c.len_utf8(), String::new()));
        } else if let Some(narrow) = halfwidth(c) {
            out.push((at..at + c.len_utf8(), narrow.to_string()));
        } else if next.is_some_and(unicode_normalization::char::is_combining_mark) {
            // A letter and its combining marks, as NFC writes them
            // ("Арте\u{308}м" reads "Артём").
            let mut end = at + c.len_utf8();
            while let Some((i, mark)) =
                chars.next_if(|&(_, mark)| unicode_normalization::char::is_combining_mark(mark))
            {
                end = i + mark.len_utf8();
            }
            let composed: String = source[at..end].nfc().collect();
            if composed != source[at..end] {
                previous = composed.chars().next_back();
                out.push((at..end, composed));
            }
        }
    }
    out
}

/// Formatting wholly inside a value, with the source edits that read it
/// as text. Keeping paired tags together lets review require the whole pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InlineMarkup {
    pub range: Range<usize>,
    pub(super) inner: Option<Range<usize>>,
    ordinal: bool,
    pub edits: Vec<(Range<usize>, String)>,
}

/// PDF imports often close and reopen emphasis between a value's words.
/// Spaces, tabs and NBSP between matching runs read as one space; outer
/// emphasis stays as written. Mixed `*`/`_` runs and other inline HTML
/// (`<u>`, `<i>`, `<b>`, `<span>`, or tags with attributes) are out of scope.
/// Bare superscripts/subscripts keep their text: ordinal suffixes after a
/// digit glue to it, while footnotes and other text start a separate word.
/// Code, escaped syntax, HTML blocks/attributes and link targets keep theirs.
pub(super) fn inline_markup(source: &str) -> Vec<InlineMarkup> {
    inline_markup_with(source, true)
}

fn inline_markup_with(source: &str, check_roles: bool) -> Vec<InlineMarkup> {
    #[cfg(test)]
    if source.len() >= 20_000 {
        LARGE_MARKUP_SCANS.with(|count| count.set(count.get() + 1));
    }
    let mut out = Vec::new();
    let escaped = |at: usize| {
        source[..at]
            .bytes()
            .rev()
            .take_while(|&b| b == b'\\')
            .count()
            % 2
            == 1
    };
    let mut at = 0;
    while at < source.len() {
        let rest = &source[at..];
        let c = rest.chars().next().unwrap_or_default();
        if matches!(c, '*' | '_')
            && source[..at]
                .chars()
                .next_back()
                .is_some_and(|before| !before.is_whitespace() && before != c)
            && !escaped(at)
        {
            let run = rest.len() - rest.trim_start_matches(c).len();
            let after = rest[run..].trim_start_matches([' ', '\t', '\u{a0}']);
            let gap = rest[run..].len() - after.len();
            if run <= 3
                && gap > 0
                && after.starts_with(&rest[..run])
                && after[run..].starts_with(|after: char| !after.is_whitespace() && after != c)
            {
                let end = at + run * 2 + gap;
                out.push(InlineMarkup {
                    range: at..end,
                    inner: None,
                    ordinal: false,
                    edits: vec![(at..end, " ".into())],
                });
                at = end;
                continue;
            }
        } else if c == '<'
            && !escaped(at)
            && let Some(tag) = rest.get(..5)
            && (tag.eq_ignore_ascii_case("<sup>") || tag.eq_ignore_ascii_case("<sub>"))
            && let Some(close) = rest[5..].find('<').map(|i| at + 5 + i)
            && let Some(end_tag) = source.get(close..close + 6)
            && end_tag.as_bytes().get(1) == Some(&b'/')
            && end_tag[2..].eq_ignore_ascii_case(&tag[1..])
            && close > at + 5
            && !source[at + 5..close].contains(['\r', '\n', '`'])
            && !escaped(close)
        {
            let inner = &source[at + 5..close];
            let inner = decoded_ordinal(inner);
            let ordinal = source[..at].ends_with(|c: char| c.is_ascii_digit())
                && matches!(
                    inner.to_lowercase().as_str(),
                    "st" | "nd"
                        | "rd"
                        | "th"
                        | "-й"
                        | "-я"
                        | "-е"
                        | "-го"
                        | "-ый"
                        | "-ой"
                        | "-ая"
                        | "-ое"
                        | "-ого"
                );
            out.push(InlineMarkup {
                range: at..close + 6,
                inner: Some(at + 5..close),
                ordinal,
                edits: vec![
                    (at..at + 5, if ordinal { String::new() } else { " ".into() }),
                    (close..close + 6, String::new()),
                ],
            });
            // Inspect the inner text too, so its character references and
            // emphasis gaps still take their own rewrites.
        }
        at += c.len_utf8();
    }
    if out.is_empty() {
        return out;
    }
    let mut protected = link_destinations(source);
    let Ok(root) = markdown::to_mdast(source, &markdown::ParseOptions::gfm()) else {
        return Vec::new();
    };
    let mut stack = vec![&root];
    let mut openers = std::collections::HashSet::new();
    let mut closers = std::collections::HashSet::new();
    while let Some(node) = stack.pop() {
        use markdown::mdast::Node;
        if matches!(node, Node::Emphasis(_) | Node::Strong(_))
            && let Some(pos) = node.position()
        {
            let width = if matches!(node, Node::Strong(_)) {
                2
            } else {
                1
            };
            openers.extend(pos.start.offset..pos.start.offset + width);
            closers.extend(pos.end.offset - width..pos.end.offset);
        }
        let opaque = match node {
            Node::Code(_) | Node::InlineCode(_) => true,
            Node::Html(html) => !["<sup>", "</sup>", "<sub>", "</sub>"]
                .iter()
                .any(|tag| html.value.eq_ignore_ascii_case(tag)),
            _ => false,
        };
        if opaque && let Some(pos) = node.position() {
            protected.push(pos.start.offset..pos.end.offset);
        }
        stack.extend(node.children().into_iter().flatten());
    }
    out.retain(|m| {
        !protected
            .iter()
            .any(|p| p.start < m.range.end && m.range.start < p.end)
            && (!check_roles || m.inner.is_some() || {
                let run = source[m.range.clone()].chars().next().unwrap();
                let width = source[m.range.clone()]
                    .chars()
                    .take_while(|&c| c == run)
                    .count();
                let left = m.range.start..m.range.start + width;
                let right = m.range.end - width..m.range.end;
                let marked = |at: &usize| openers.contains(at) || closers.contains(at);
                (left.clone().all(|at| closers.contains(&at))
                    && right.clone().all(|at| openers.contains(&at)))
                    || (!left.into_iter().any(|at| marked(&at))
                        && !right.into_iter().any(|at| marked(&at)))
            })
    });
    out
}

/// Decode references in a suffix without recursively inspecting markup.
fn decoded_ordinal(text: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    while at < text.len() {
        if text[at..].starts_with('&')
            && let Some((len, value)) = character_reference(&text[at..])
        {
            out.push_str(&value);
            at += len;
        } else {
            let c = text[at..].chars().next().unwrap();
            out.push(c);
            at += c.len_utf8();
        }
    }
    out
}

#[cfg(test)]
thread_local! {
    pub(super) static LARGE_MARKUP_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The length of the footnote reference at `at` in `source` (`[^1]`,
/// `[^tax]`: no whitespace or brackets in its label), unless it starts a
/// line and a colon follows, as its definition does.
fn footnote_reference(source: &str, at: usize) -> Option<usize> {
    let label = source[at..].strip_prefix("[^")?;
    let len = label.find(|c: char| c.is_whitespace() || matches!(c, '[' | ']'))?;
    if len == 0 || !label[len..].starts_with(']') {
        return None;
    }
    let end = at + 2 + len + 1;
    let line_start = source[..at].rfind('\n').map_or(0, |i| i + 1);
    // A definition may sit in a quote or a list item ("> [^1]: …").
    let prefix = source[line_start..at].trim_matches([' ', '\t', '>']);
    let item = prefix.strip_suffix(['.', ')']).unwrap_or(prefix);
    let definition = (prefix.is_empty()
        || matches!(prefix, "-" | "*" | "+")
        || !item.is_empty()
            && item.len() < prefix.len()
            && item.bytes().all(|b| b.is_ascii_digit()))
        && source[end..].starts_with(':');
    (!definition).then_some(end - at)
}

/// Letter-spaced words, as OCR and spaced-out captions write them
/// (#400, #448): "D E L V A   R O S K" reads "DELVA ROSK", "И Н Н:" reads
/// "ИНН:". A word is capital letters each one space apart, two at least and
/// one word of three at least, all of one script (Latin or Cyrillic); two
/// spaces or more part the words, and read as one. The run stands apart
/// ([`stands_apart`]): a word close by on either side makes initials or an
/// enumeration of it ("J R Tolkien", "пункты А Б В утратили"), and
/// lowercase one-letter words ("и в с", "Я и я", "a b c") are no letter
/// spacing. The spaces only are rewritten, so a span over the word maps to
/// its letters and the spaces between them.
fn letter_spacing(source: &str) -> Vec<(Range<usize>, String)> {
    let mut out = Vec::new();
    let mut at = 0;
    for line in source.split_inclusive('\n') {
        spaced_line(line.trim_end_matches(['\r', '\n']), at, &mut out);
        at += line.len();
    }
    out
}

/// [`letter_spacing`]'s rewrites in `line`, which starts at `offset`.
fn spaced_line(line: &str, offset: usize, out: &mut Vec<(Range<usize>, String)>) {
    let word_char =
        |c: char| c.is_alphanumeric() || unicode_normalization::char::is_combining_mark(c);
    let mut at = 0;
    while at < line.len() {
        let c = line[at..].chars().next().unwrap_or_default();
        let starts = c.is_alphabetic() && !line[..at].chars().next_back().is_some_and(word_char);
        let group = starts.then(|| spaced_group(line, at)).flatten();
        let Some(words) = group else {
            at += c.len_utf8();
            continue;
        };
        let end = words
            .last()
            .and_then(|word| word.last())
            .map_or(at, |letter| letter.end);
        for (index, word) in words.iter().enumerate() {
            for pair in word.windows(2) {
                out.push((offset + pair[0].end..offset + pair[1].start, String::new()));
            }
            if let Some(next) = words.get(index + 1) {
                let gap = word.last().map_or(0, |l| l.end)..next[0].start;
                out.push((offset + gap.start..offset + gap.end, " ".into()));
            }
        }
        at = end;
    }
}

/// The letter-spaced words starting at `start` of `line`, each its letters'
/// ranges, when they make a run [`letter_spacing`] reads whole.
fn spaced_group(line: &str, start: usize) -> Option<Vec<Vec<Range<usize>>>> {
    let first = spaced_word(line, start);
    if first.len() < 2 {
        return None;
    }
    let mut words = vec![first];
    loop {
        let end = words
            .last()
            .and_then(|word| word.last())
            .map_or(start, |l| l.end);
        let rest = &line[end..];
        let gap = rest.len() - rest.trim_start_matches(' ').len();
        if gap < 2 {
            break;
        }
        let word = spaced_word(line, end + gap);
        if word.len() < 2 {
            break;
        }
        words.push(word);
    }
    let letter = |range: &Range<usize>| line[range.clone()].chars().next().unwrap_or_default();
    let script = |c: char| match c {
        'A'..='Z' | 'a'..='z' | '\u{c0}'..='\u{24f}' => Some(0),
        '\u{400}'..='\u{52f}' => Some(1),
        _ => None,
    };
    let first_script = script(letter(&words[0][0]))?;
    let shaped = words.iter().all(|word| {
        word.len() >= 2
            && word.iter().all(|l| {
                let c = letter(l);
                c.is_uppercase() && script(c) == Some(first_script)
            })
    });
    let end = words.last()?.last()?.end;
    (shaped
        && words.iter().any(|word| word.len() >= 3)
        && stands_apart(line[..start].chars().rev().collect::<String>().as_str())
        && stands_apart(&line[end..]))
    .then_some(words)
}

/// The single letters, one space apart, starting at `start` of `line` (a
/// letter keeps its combining marks); a letter a word goes on from ends
/// them before it.
fn spaced_word(line: &str, start: usize) -> Vec<Range<usize>> {
    let mut letters = Vec::new();
    let mut at = start;
    loop {
        let mut chars = line[at..].chars();
        let Some(c) = chars.next().filter(|c| c.is_alphabetic()) else {
            break;
        };
        let marks: usize = chars
            .take_while(|&m| unicode_normalization::char::is_combining_mark(m))
            .map(char::len_utf8)
            .sum();
        let end = at + c.len_utf8() + marks;
        // A note marker after a letter is no letter of a word ("И Н Н¹:").
        if line[end..].starts_with(|c: char| c.is_alphanumeric() && !note_marker(c)) {
            break;
        }
        letters.push(at..end);
        let rest = &line[end..];
        if rest.starts_with(' ') && rest[1..].starts_with(char::is_alphabetic) {
            at = end + 1;
        } else {
            break;
        }
    }
    letters
}

/// Whether a letter-spaced run stands apart from the text on one side,
/// `side` read outward from the run past emphasis and note marks: nothing,
/// punctuation within two spaces, or a word three spaces or tabs away or
/// more, as a column's is. A word closer makes the run initials or an
/// enumeration ("J R R  Tolkien", "**J R R** **Tolkien**").
fn stands_apart(side: &str) -> bool {
    let marks = |c: char| matches!(c, '*' | '_' | '~') || note_marker(c);
    let side = side.trim_start_matches(marks);
    let text = side.trim_start_matches(char::is_whitespace);
    let gap = side[..side.len() - text.len()].chars().count();
    text.trim_start_matches(marks)
        .chars()
        .next()
        .is_none_or(|c| gap >= 3 || !c.is_alphanumeric())
}

/// The length of an HTML line break tag at the start of `text`: `<br>`,
/// `<br/>` or `<br />`, in any case.
pub fn line_break_tag(text: &str) -> Option<usize> {
    let rest = text
        .get(..3)
        .filter(|tag| tag.eq_ignore_ascii_case("<br"))?;
    let after = text[rest.len()..].trim_start_matches(' ');
    let after = after.strip_prefix('/').unwrap_or(after);
    after.starts_with('>').then(|| text.len() - after.len() + 1)
}

/// Column headers that name an identifier (#397): a cell under one reads
/// with its header ("| 46 82 731609 |" under "| Паспорт |" reads
/// "| Паспорт: 46 82 731609 |"), as the same value does after its label in
/// running text. Matched whole, case and "ё" ignored, after
/// [`header_name`] drops `*`, `:`, `#`, `№` and "No.". Only identifier
/// columns: a header before a name or a heading word ("Фамилия: Кий",
/// "Lead technician") makes the model read the header as part of the value;
/// name parts' columns are proposed by [`crate::columns`] instead.
const IDENTIFIER_HEADERS: &[&str] = &[
    // Russian.
    "паспорт",
    "паспорт рф",
    "номер паспорта",
    "серия и номер",
    "серия и номер паспорта",
    "инн",
    "снилс",
    "огрн",
    "огрнип",
    "кпп",
    "бик",
    "счет",
    "номер счета",
    "расчетный счет",
    "р/с",
    "банковский счет",
    "карта",
    "номер карты",
    "полис",
    "номер полиса",
    "телефон",
    "номер телефона",
    "e-mail",
    "email",
    "почта",
    "электронная почта",
    "эл. почта",
    // English.
    "tax id",
    "tin",
    "ein",
    "ssn",
    "ni number",
    "national insurance number",
    "passport",
    "passport number",
    "account",
    "account number",
    "bank account",
    "iban",
    "sort code",
    "routing",
    "routing number",
    "card",
    "card number",
    "policy",
    "policy number",
    "phone",
    "phone number",
    "telephone",
];

/// A column header as [`IDENTIFIER_HEADERS`] lists it: lowercase, "ё" as
/// "е", without `*`, `:`, `#`, `№` and a "No." word, words single-spaced.
pub(crate) fn header_name(header: &str) -> String {
    let header = header
        .to_lowercase()
        .replace('ё', "е")
        .replace(['*', ':', '#', '№'], " ");
    header
        .split_whitespace()
        .filter(|word| !matches!(*word, "no" | "no."))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Each cell of an identifier column ([`IDENTIFIER_HEADERS`]) with its
/// header inserted before the value, as empty source ranges and the header
/// they read: a value alone in a cell loses what it is (ADR 0092, table
/// cells read as sentences). Only a value with a digit or an "@" gets one:
/// a totals row's "Итого" or a dash in the column is no identifier. A span
/// over an inserted header maps to its cell; one only over it, to nothing.
fn table_headers(source: &str) -> Vec<(Range<usize>, String)> {
    let mut out = Vec::new();
    for table in tables(source) {
        let (_, header) = table.header;
        let names: Vec<Option<&str>> = table
            .columns
            .iter()
            .map(|cell| {
                let name = header[cell.clone()]
                    .trim_matches(|c: char| c.is_whitespace() || matches!(c, '*' | ':'));
                IDENTIFIER_HEADERS
                    .contains(&header_name(name).as_str())
                    .then_some(name)
            })
            .collect();
        for &(start, line) in &table.rows {
            for (cell, name) in cells(line).into_iter().zip(&names) {
                let Some(name) = name else { continue };
                let value = &line[cell.clone()];
                let content = start + cell.start + value.len() - value.trim_start().len();
                if identifier_like(value) {
                    out.push((content..content, format!("{name}: ")));
                }
            }
        }
    }
    out
}

/// A GFM table of the source: its header row and its body rows, each with
/// the offset its line starts at.
pub(crate) struct Table<'a> {
    pub(crate) header: (usize, &'a str),
    /// The header row's cells, in its line.
    pub(crate) columns: Vec<Range<usize>>,
    pub(crate) rows: Vec<(usize, &'a str)>,
}

/// The source's GFM tables: a header row a delimiter row of as many cells
/// follows, at most three spaces in and outside code blocks, and the rows
/// after it up to a blank line, another block or the next table's header.
pub(crate) fn tables(source: &str) -> Vec<Table<'_>> {
    let mut out = Vec::new();
    let mut lines = Vec::new();
    let mut at = 0;
    for line in source.split_inclusive('\n') {
        lines.push((at, line.trim_end_matches(['\r', '\n'])));
        at += line.len();
    }
    let mut row = 0;
    while row + 1 < lines.len() {
        let (_, header) = lines[row];
        let (_, delimiter) = lines[row + 1];
        // A code block's text is no table: skip a fence to its close.
        if let Some(fence) = fence(header) {
            row += 1;
            while row < lines.len() && !lines[row].1.trim_start().starts_with(fence) {
                row += 1;
            }
            row += 1;
            continue;
        }
        let columns = cells(header);
        let indent = header.len() - header.trim_start_matches(' ').len();
        if !header.contains('|')
            || indent > 3
            || header.starts_with('\t')
            || block_start(header)
            || !delimiter_row(delimiter)
            || cells(delimiter).len() != columns.len()
        {
            row += 1;
            continue;
        }
        let mut table = Table {
            header: lines[row],
            columns,
            rows: Vec::new(),
        };
        row += 2;
        // Up to the next table's header, which a delimiter row follows.
        while let Some(&found) = lines.get(row).filter(|(_, line)| {
            line.contains('|')
                && !block_start(line)
                && !lines
                    .get(row + 1)
                    .is_some_and(|(_, next)| delimiter_row(next))
        }) {
            table.rows.push(found);
            row += 1;
        }
        out.push(table);
    }
    out
}

/// Whether a cell's value, its character references decoded, has a digit
/// or an "@": "&#8212;" is a dash, no identifier.
fn identifier_like(value: &str) -> bool {
    let mut rest = value;
    while let Some(c) = rest.chars().next() {
        let (len, text) = match character_reference(rest) {
            Some((len, decoded)) if c == '&' => (len, decoded),
            _ => (c.len_utf8(), c.to_string()),
        };
        if text.contains(|c: char| c.is_numeric() || matches!(c, '@' | '＠')) {
            return true;
        }
        rest = &rest[len..];
    }
    false
}

/// A GFM table's delimiter row: "| --- | :-: |".
fn delimiter_row(line: &str) -> bool {
    let cells = cells(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let cell = line[cell.clone()].trim();
            // One colon at most on either side.
            let cell = cell.strip_prefix(':').unwrap_or(cell);
            let dashes = cell.strip_suffix(':').unwrap_or(cell);
            !dashes.is_empty() && dashes.bytes().all(|b| b == b'-')
        })
}

/// The fence a code block opens with on `line` ("```" or "~~~", at most
/// three spaces in), which its closing line starts with too.
pub(crate) fn fence(line: &str) -> Option<&'static str> {
    let text = line.trim_start_matches(' ');
    (line.len() - text.len() <= 3)
        .then(|| ["```", "~~~"].into_iter().find(|f| text.starts_with(f)))
        .flatten()
}

/// Whether `line` opens a block that ends a table or cannot be its header
/// row: blank, a heading, a quote, a fence, a list item or HTML.
fn block_start(line: &str) -> bool {
    let line = line.trim_start();
    line.is_empty()
        || line.starts_with(['#', '>', '<'])
        || line.starts_with("```")
        || line.starts_with("~~~")
        || line.starts_with(['-', '*', '+']) && line[1..].starts_with([' ', '\t'])
        || line.split_once(['.', ')']).is_some_and(|(number, rest)| {
            !number.is_empty()
                && number.len() <= 9
                && number.bytes().all(|b| b.is_ascii_digit())
                && rest.starts_with([' ', '\t'])
        })
}

/// The cells of a table row, between its unescaped pipes (a pipe after an
/// odd run of backslashes is escaped).
pub(crate) fn cells(line: &str) -> Vec<Range<usize>> {
    let mut pipes: Vec<usize> = line
        .match_indices('|')
        .map(|(i, _)| i)
        .filter(|&i| line[..i].bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 0)
        .collect();
    let body = line.trim();
    let offset = line.len() - line.trim_start().len();
    let end = offset + body.len();
    let leading = pipes.first() == Some(&offset);
    let trailing =
        pipes.last().is_some_and(|&last| last + 1 == end) && (pipes.len() > 1 || !leading);
    if !leading {
        pipes.insert(0, offset.wrapping_sub(1));
    }
    if !trailing {
        pipes.push(end);
    }
    pipes
        .windows(2)
        .map(|pair| pair[0].wrapping_add(1)..pair[1])
        .collect()
}

/// Link and image destinations ("](clients/Alice_Morgan)"): hidden source
/// that names people in its own spelling.
fn link_destinations(source: &str) -> Vec<Range<usize>> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for (open, _) in source.match_indices("](") {
        // Spaces or tabs, and one line ending, may come first ("]( url)").
        let mut start = open + 2;
        while matches!(bytes.get(start), Some(b' ' | b'\t')) {
            start += 1;
        }
        if bytes.get(start) == Some(&b'\r') {
            start += 1;
        }
        if bytes.get(start) == Some(&b'\n') {
            start += 1;
            while matches!(bytes.get(start), Some(b' ' | b'\t')) {
                start += 1;
            }
        }
        let mut depth = 0;
        let mut end = start;
        while let Some(&byte) = bytes.get(end) {
            match byte {
                b'(' => depth += 1,
                b')' if depth == 0 => break,
                b')' => depth -= 1,
                b' ' | b'\t' | b'\n' | b'\r' => break,
                _ => {}
            }
            end += 1;
        }
        if out
            .last()
            .is_none_or(|last: &Range<usize>| last.end <= start)
        {
            out.push(start..end);
        }
    }
    out
}

/// The link target schemes whose targets are read as text: web addresses,
/// email and phone links. A relative target has none.
const READ_SCHEMES: [&str; 6] = ["http", "https", "mailto", "tel", "sms", "callto"];

/// The parts of `destination` detection never reads. In a target of another
/// scheme ("consultantplus://offline/ref=7C2E9A41…;dst=100015",
/// "garant://10064072/12") they are its references: a hash (a run of hex
/// digits and letters, eight or more, with a digit) and a number shorter
/// than ten digits. Such a target points into another program and is not
/// the document's text, but a name ("Nesket2026" reads "Nesket"), an email
/// ("xmpp:alice1985+legal@…") or a long number (a phone, a tax number,
/// "112-233-445 95") in it still reads.
fn opaque_parts(source: &str, destination: &Range<usize>) -> Vec<Range<usize>> {
    let target = &source[destination.clone()];
    // "<consultantplus://…>": a destination in angle brackets.
    let open = usize::from(target.starts_with('<'));
    let Some(colon) = target[open..].find(':').map(|at| open + at) else {
        return Vec::new();
    };
    let scheme = &target[open..colon];
    // "C:" is a drive, not a scheme.
    if scheme.len() < 2
        || !scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        || READ_SCHEMES
            .iter()
            .any(|read| scheme.eq_ignore_ascii_case(read))
    {
        return Vec::new();
    }
    // The target after its scheme, each character with its source range:
    // a percent escape ("%D0%90") or a character reference ("&#64;") is the
    // character it stands for.
    let mut units = Vec::new();
    let mut i = destination.start + colon + 1;
    while i < destination.end {
        let rest = &source[i..destination.end];
        let c = rest.chars().next().unwrap_or_default();
        let (len, value) = if c == '%'
            && let Some(escape) = percent_char(rest)
        {
            escape
        } else if c == '&'
            && let Some((len, value)) = character_reference(rest)
        {
            let mut chars = value.chars();
            (
                len,
                chars
                    .next()
                    .filter(|_| chars.next().is_none())
                    .unwrap_or(' '),
            )
        } else {
            (c.len_utf8(), c)
        };
        units.push((i..i + len, value));
        i += len;
    }
    // An email reads whole: its local part and domain around each "@".
    let mut email = vec![false; units.len()];
    for at in 0..units.len() {
        if !matches!(units[at].1, '@' | '＠') {
            continue;
        }
        email[at] = true;
        for i in (0..at).rev() {
            let c = units[i].1;
            if !(c.is_alphanumeric() || ".!$'*+-=^_{|}~".contains(c)) {
                break;
            }
            email[i] = true;
        }
        for i in at + 1..units.len() {
            let c = units[i].1;
            if !(c.is_alphanumeric() || matches!(c, '.' | '-')) {
                break;
            }
            email[i] = true;
        }
    }
    // Runs of letters and digits outside emails, as ranges of units.
    let mut runs: Vec<Range<usize>> = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let word = |i: usize| units[i].1.is_alphanumeric() && !email[i];
        if word(i) {
            let start = i;
            while i < units.len() && word(i) {
                i += 1;
            }
            runs.push(start..i);
        } else {
            i += 1;
        }
    }
    let numeric = |i: usize| units[i].1.is_numeric();
    let mut hidden: Vec<Range<usize>> = Vec::new();
    let mut r = 0;
    while r < runs.len() {
        let run = runs[r].clone();
        if run.clone().all(numeric) {
            // A number's groups joined by a space, "-" or brackets are one
            // number ("112-233-445 95", "7 (900) 000-82-14").
            let mut end = r + 1;
            let mut digits = run.len();
            while let Some(next) = runs.get(end)
                && next.clone().all(numeric)
                && next.start - runs[end - 1].end <= 2
                && units[runs[end - 1].end..next.start]
                    .iter()
                    .all(|(_, c)| matches!(c, ' ' | '-' | '(' | ')'))
            {
                digits += next.len();
                end += 1;
            }
            if digits < 10 {
                hidden.extend(runs[r..end].iter().cloned());
            }
            r = end;
            continue;
        }
        if run.len() >= 8
            && run.clone().any(numeric)
            && units[run.clone()]
                .iter()
                .all(|(_, c)| c.is_ascii_hexdigit())
        {
            hidden.push(run);
        } else {
            // "Nesket2026": the word reads, the number is a reference.
            let mut i = run.start;
            while i < run.end {
                let start = i;
                let digit = numeric(i);
                while i < run.end && numeric(i) == digit {
                    i += 1;
                }
                if digit && i - start < 10 {
                    hidden.push(start..i);
                }
            }
        }
        r += 1;
    }
    hidden
        .into_iter()
        .map(|run| units[run.start].0.start..units[run.end - 1].0.end)
        .collect()
}

/// Whether the underscore at `at` in `destination` is in an email address's
/// local part ("mailto:alice_morgan@example.invalid", "&#64;" or "%40" for
/// its "@"), whose underscores are its own.
fn in_email(source: &str, destination: &Range<usize>, at: usize) -> bool {
    let mut i = at;
    while i < destination.end {
        let rest = &source[i..destination.end];
        if rest.starts_with(['@', '＠']) || rest.starts_with("%40") {
            return true;
        }
        let c = rest.chars().next().unwrap_or_default();
        if c == '&' {
            match character_reference(rest) {
                Some((_, value)) if value == "@" || value == "＠" => return true,
                Some((len, _)) => i += len,
                // A query's "&" ends the part.
                None => return false,
            }
            continue;
        }
        if matches!(c, '/' | '?' | '#' | '=' | ':') {
            return false;
        }
        i += c.len_utf8();
    }
    false
}

/// The percent-encoded character at the start of `text` ("%20", or the
/// UTF-8 bytes of one character, "%D0%90") and its length.
fn percent_char(text: &str) -> Option<(usize, char)> {
    let byte = |at: usize| {
        let hex = text
            .get(at + 1..at + 3)
            .filter(|_| text[at..].starts_with('%'))?;
        u8::from_str_radix(hex, 16).ok()
    };
    let first = byte(0)?;
    let len = match first {
        0x20..=0x7e => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return None,
    };
    let bytes: Vec<u8> = (0..len).map(|i| byte(3 * i)).collect::<Option<_>>()?;
    let value = String::from_utf8(bytes).ok()?.chars().next()?;
    Some((3 * len, value))
}

/// The percent-encoded space or letter at the start of `text` ("%20",
/// "%D0%90"): what a name is spelt with. Its length and value.
fn percent_encoded(text: &str) -> Option<(usize, String)> {
    let (len, value) = percent_char(text)?;
    (value == ' ' || value.is_alphabetic()).then(|| (len, value.to_string()))
}

/// `text` with the line-end hyphens that split its words dropped, as the
/// view reads them: a wording found in the source ("Верес-\nтова Дарья
/// Ильинична") reads as the name it is ("Верестова Дарья Ильинична"). A
/// hyphen before a capital ("Санкт-\nПетербург"), after a space (a dash)
/// or before a list marker stays, and so does its line break.
pub fn unwrapped(text: &str) -> std::borrow::Cow<'_, str> {
    let mut out = String::new();
    let mut from = 0;
    let mut previous = None;
    let mut chars = text.char_indices();
    while let Some((at, c)) = chars.next() {
        if c == '-'
            && previous.is_some_and(char::is_alphabetic)
            && let Some(len) = line_end_hyphen(&text[at..])
        {
            out.push_str(&text[from..at]);
            from = at + len;
            while chars.offset() < from {
                chars.next();
            }
            previous = None;
        } else {
            previous = Some(c);
        }
    }
    if from == 0 {
        return std::borrow::Cow::Borrowed(text);
    }
    out.push_str(&text[from..]);
    std::borrow::Cow::Owned(out)
}

/// The length of a line-end hyphen that splits a word at the start of
/// `text` ("-\n" and the next line's indent before a lowercase letter, as
/// in "Brin-\nsett"), which the word reads without.
pub(crate) fn line_end_hyphen(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('-')?;
    let len = line_break(rest)?;
    rest[len..]
        .starts_with(char::is_lowercase)
        .then_some(1 + len)
}

/// The length of the line break and the next line's indent at the start of
/// `text`, when the next line goes on with text.
fn line_break(text: &str) -> Option<usize> {
    let rest = text
        .strip_prefix("\r\n")
        .or_else(|| text.strip_prefix('\n'))?;
    let word = rest.trim_start_matches([' ', '\t']);
    (!word.is_empty() && !word.starts_with(['\r', '\n'])).then_some(text.len() - word.len())
}

/// Whether the line breaks inside `range` of `source` only wrap one value
/// (ADR 0092: OCR and narrow columns wrap names, emails, phones): each is a
/// line-end hyphen inside a word, or the value ends its last line but for
/// closing punctuation. "Delva Rosk\nExhibition company" before ": …" is two
/// lines' text, not one wrapped value.
///
/// An address the rule pack's `span` pattern found whole (`address`) is laid
/// out over lines of its own, the pattern having read its parts on each
/// ("4410 W. Quarry Parkway, Suite 210\nBrindlecombe, OH 45999"), so text may
/// follow it on its last line (", and to …") when it ends in its number (a
/// postcode, ZIP or flat number, not a town that may be the next field's
/// first word: "12 Oak Road,\nDate" of "Date of birth"), that text is a word
/// apart ("Room 2" of "Room 2/3" is not) and it is no field's label (": …").
pub fn wrapped_value(source: &str, range: &Range<usize>, address: bool) -> bool {
    let value = &source[range.clone()];
    let plain = value.match_indices('\n').any(|(i, _)| {
        let at = range.start + i;
        let hyphen = source[..at].strip_suffix('\r').unwrap_or(&source[..at]);
        !(hyphen.ends_with('-')
            && hyphen[..hyphen.len() - 1]
                .chars()
                .next_back()
                .is_some_and(char::is_alphabetic)
            && line_end_hyphen(&source[hyphen.len() - 1..]).is_some())
    });
    let rest = source[range.end..].split(['\r', '\n']).next().unwrap_or("");
    !plain
        || rest
            .chars()
            .all(|c| c.is_whitespace() || ".,;)»\"'".contains(c))
        || (address && numbered_end(value, rest))
}

/// Whether a value's last word holds a digit and the text after it on its
/// line (`rest`) is a word apart, after closing punctuation, and no label.
fn numbered_end(value: &str, rest: &str) -> bool {
    let after = rest.trim_start_matches(|c| ".,;)»\"'".contains(c));
    value
        .rsplit(char::is_whitespace)
        .next()
        .is_some_and(|word| word.contains(|c: char| c.is_ascii_digit()))
        && (after.is_empty() || after.starts_with(char::is_whitespace))
        && !rest.trim_start().starts_with(':')
}

/// `text` with its full-width forms as ASCII ([`halfwidth`]).
fn narrowed(text: String) -> String {
    if text.chars().any(|c| halfwidth(c).is_some()) {
        text.chars().map(|c| halfwidth(c).unwrap_or(c)).collect()
    } else {
        text
    }
}

/// The ASCII character a full-width form stands for, as NFKC reads it
/// ("４６８２" is "4682"), and the ideographic space as a space.
fn halfwidth(c: char) -> Option<char> {
    match c {
        '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0),
        '\u{3000}' => Some(' '),
        _ => None,
    }
}

/// Whether the character at `at` is inside an email address: the run of
/// non-space characters around it holds an "@" ("_roman@kadry.invalid" keeps
/// its underscore).
fn in_address(source: &str, at: usize) -> bool {
    let start = source[..at]
        .char_indices()
        .rfind(|&(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let end = source[at..]
        .find(char::is_whitespace)
        .map_or(source.len(), |i| at + i);
    source[start..end].contains(['@', '＠'])
}

/// A superscript digit, as a note marker writes it after a word ("¹").
pub fn note_marker(c: char) -> bool {
    matches!(c, '¹' | '²' | '³' | '⁰' | '⁴'..='⁹')
}

/// Characters that only break a word for the eye or a renderer: the soft
/// hyphen and zero-width spaces and joiners ("Ма\u{ad}рина" is "Марина").
fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{ad}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}'
    )
}

/// The character reference at the start of `text` (`&#97;`, `&#x61;`,
/// `&amp;`) as CommonMark decodes it, and its length.
fn character_reference(text: &str) -> Option<(usize, String)> {
    let bytes = text.as_bytes();
    let (digits, radix, max) = match (bytes.get(1), bytes.get(2)) {
        (Some(b'#'), Some(b'x' | b'X')) => (3, 16, 6),
        (Some(b'#'), _) => (2, 10, 7),
        _ => (1, 0, 31),
    };
    let len = bytes[digits..]
        .iter()
        .take(max + 1)
        .take_while(|b| match radix {
            16 => b.is_ascii_hexdigit(),
            10 => b.is_ascii_digit(),
            _ => b.is_ascii_alphanumeric(),
        })
        .count();
    if len == 0 || len > max || bytes.get(digits + len) != Some(&b';') {
        return None;
    }
    let end = digits + len + 1;
    let value = &text[digits..digits + len];
    let decoded = if radix == 0 {
        // Named references as markdown-rs knows them (HTML 5).
        let options = markdown::ParseOptions::default();
        let node = markdown::to_mdast(&text[..end], &options).ok()?;
        let decoded = node.to_string();
        (decoded != text[..end]).then_some(decoded)?
    } else {
        let code = u32::from_str_radix(value, radix).ok()?;
        match char::from_u32(code) {
            Some(c) if code != 0 && !c.is_control() || matches!(c, '\t' | '\n' | '\r') => {
                c.to_string()
            }
            _ => char::REPLACEMENT_CHARACTER.to_string(),
        }
    };
    Some((end, decoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_emphasis_and_script_tags_read_as_one_value() {
        for (source, expected, value, original) in [
            (
                "See *Pellam* *Industries Inc*.",
                "See *Pellam Industries Inc*.",
                "Pellam Industries Inc",
                "Pellam* *Industries Inc",
            ),
            (
                "**Rooke & Venn** **Groundworks Ltd**",
                "**Rooke & Venn Groundworks Ltd**",
                "Rooke & Venn Groundworks Ltd",
                "Rooke & Venn** **Groundworks Ltd",
            ),
            (
                "d/b/a Quillon** **Freight Media",
                "d/b/a Quillon Freight Media",
                "Quillon Freight Media",
                "Quillon** **Freight Media",
            ),
            (
                "**4410 Quarry Street** **Brindlecombe, CA 95999**",
                "**4410 Quarry Street Brindlecombe, CA 95999**",
                "4410 Quarry Street Brindlecombe, CA 95999",
                "4410 Quarry Street** **Brindlecombe, CA 95999",
            ),
            (
                "_Pellam_ _Industries Inc_",
                "Pellam Industries Inc",
                "Pellam Industries Inc",
                "Pellam_ _Industries Inc",
            ),
            (
                "4300 South 26<sup>th</sup> Street",
                "4300 South 26th Street",
                "26th Street",
                "26<sup>th</sup> Street",
            ),
            (
                "4300 South 26<SUB>th</SUB> Street",
                "4300 South 26th Street",
                "26th Street",
                "26<SUB>th</SUB> Street",
            ),
            (
                "At 26<sup>t&#104;</sup> Street",
                "At 26th Street",
                "26th Street",
                "26<sup>t&#104;</sup> Street",
            ),
        ] {
            let view = View::new(source);
            assert_eq!(view.text, expected, "{source}");
            let start = view.text.find(value).unwrap();
            let range = view.source_range(start..start + value.len());
            assert_eq!(&source[range.clone()], original, "{source}");
            assert_eq!(&view.text[view.view_range(range)], value);
        }
        let source = "At 26<sup>th</sup> Street";
        let view = View::new(source);
        let at = view.text.find("th").unwrap();
        assert_eq!(&source[view.source_range(at..at + 1)], "t");
        assert_eq!(&source[view.source_range(at + 1..at + 2)], "h");
    }

    #[test]
    fn inline_markup_keeps_markdown_structure_and_code() {
        for source in [
            "* real list item",
            "# A heading",
            "**bold** text",
            "**a whole value**",
            "A*\n*B",
            "A** ** B",
            "A**** ****B",
            "Pellam** **Ltd and more** text",
            "snake_case",
            "`26<sup>th</sup> Street`",
            "`A** **B`",
            "```html\n26<sup>th</sup> Street\nA** **B\n```",
            "    26<sup>th</sup> Street\n    A** **B\n",
            "At 26<sup class=\"n\">th</sup> Street",
            "At 26<sup>th</sub> Street",
            "At 26\\<sup>th</sup> Street",
            "A\\* *B",
            "[file](https://x.invalid/A** **B)",
            "<span title=\"A** **B\">label</span>",
            "| A | B |\n|---|---|\n| C | D |",
        ] {
            assert_eq!(View::new(source).text, source, "{source}");
        }
        // Normalizing text within a list, heading or cell keeps its markers.
        assert_eq!(
            View::new("* **A** **B**\n# *C* *D*\n| **E** **F** |").text,
            "* **A B**\n# *C D*\n| **E F** |"
        );
        for (source, expected) in [
            ("*A* _B_", "*A* B"),
            ("**A** __B__", "**A** B"),
            ("***A*** ___B___", "***A*** B"),
        ] {
            assert!(inline_markup(source).is_empty(), "{source}");
            assert_eq!(View::new(source).text, expected);
        }
    }

    #[test]
    fn ordinals_glue_but_other_superscripts_are_separate_words() {
        for tag in ["sup", "sub"] {
            for suffix in ["st", "nd", "rd", "th", "TH", "-й", "-я", "-е", "-го"] {
                let source = format!("Flat 2<{tag}>{suffix}</{tag}>");
                let view = View::new(&source);
                assert_eq!(view.text, format!("Flat 2{suffix}"));
                assert_eq!(view.source_range(0..view.text.len()), 0..source.len());
                let start = view.text.find(suffix).unwrap();
                let range = view.source_range(start..view.text.len());
                assert_eq!(&source[range], format!("<{tag}>{suffix}</{tag}>"));
            }
            for (source, expected) in [
                (format!("Durnell<{tag}>th</{tag}>"), "Durnell th"),
                (format!("26<{tag}>1</{tag}>"), "26 1"),
                (format!("26<{tag}>other</{tag}>"), "26 other"),
            ] {
                assert_eq!(View::new(&source).text, expected);
            }
        }
    }

    /// #498: an address found whole over its own lines stays one value with
    /// text after it on its last line; any other value over two lines, or an
    /// address before a field's label, is two lines' text (#459).
    #[test]
    fn an_address_over_its_own_lines_is_one_value_with_text_after_it() {
        let wrapped = |source: &str, value: &str, address| {
            let start = source.find(value).unwrap();
            wrapped_value(source, &(start..start + value.len()), address)
        };
        let source = "Notices to Larkfield Supply LLC, 4410 W. Quarry Parkway, Suite 210\r\nBrindlecombe, OH 45999, and to Harrow House.";
        let address = "4410 W. Quarry Parkway, Suite 210\r\nBrindlecombe, OH 45999";
        assert!(wrapped(source, address, true));
        assert!(!wrapped(source, address, false));
        // Ending its line, any value is one, as before.
        assert!(wrapped(
            "At 12 Oak Road\nLeeds LS1 4AB.\nNext",
            "12 Oak Road\nLeeds LS1 4AB",
            false
        ));
        // A field's label after it ends the address's last part.
        let source = "Depositor: 12 Oak Road\nLeeds LS1 4AB Email: ann@x.invalid";
        assert!(!wrapped(source, "12 Oak Road\nLeeds LS1 4AB Email", true));
        let source = "Depositor: Delva Rosk\nExhibition company: Cinder Ltd";
        assert!(!wrapped(source, "Delva Rosk\nExhibition company", false));
        assert!(!wrapped(source, "Delva Rosk\nExhibition company", true));
        // It ends in its number, before a word apart (review of #508): not
        // a town that is the next field's first word, nor part of a number.
        let source = "12 Oak Road,\nDate of birth: 14/03/1988";
        assert!(!wrapped(source, "12 Oak Road,\nDate", true));
        for (source, ok) in [
            ("12 Oak Road\nRoom 2, and to", true),
            ("12 Oak Road\nRoom 2 and to", true),
            ("12 Oak Road\nRoom 2/3, Leeds", false),
            ("12 Oak Road\nRoom 2.3, Leeds", false),
            ("12 Oak Road\nRoom 2-3, Leeds", false),
            ("12 Oak Road\nRoom 2, Email: x", true),
            ("12 Oak Road\nRoom 2 : x", false),
        ] {
            assert_eq!(wrapped(source, "12 Oak Road\nRoom 2", true), ok, "{source}");
        }
        // On one line, text after a value never matters.
        assert!(wrapped("12 Oak Road, Leeds: x", "12 Oak Road", false));
    }

    #[test]
    fn character_references_decode_and_map_back_to_whole_references() {
        let source = "Dear Ann&#97; Petrova &amp; Ism&#x61;y O&#39;Verren, &nope; &#; AT&T";
        let view = View::new(source);
        assert_eq!(
            view.text,
            "Dear Anna Petrova & Ismay O'Verren, &nope; &#; AT&T"
        );
        let span = |text: &str| {
            let start = view.text.find(text).unwrap();
            &source[view.source_range(start..start + text.len())]
        };
        assert_eq!(span("Anna Petrova"), "Ann&#97; Petrova");
        assert_eq!(span("Ismay O'Verren"), "Ism&#x61;y O&#39;Verren");
        assert_eq!(span("Petrova"), "Petrova");
        assert_eq!(span("& Ism"), "&amp; Ism");
        // A boundary inside a decoded character takes all of it.
        assert_eq!(span("Anna"), "Ann&#97;");
        assert_eq!(span("Ann"), "Ann");
        assert_eq!(span("AT&T"), "AT&T");
        assert_eq!(decoded("no references"), "no references");
        // Invisible characters drop out and Russian letters compose.
        let source = "Ма\u{ad}рина и Арте\u{308}м Заи\u{306}цев, pavel@\u{200b}vector.invalid";
        let view = View::new(source);
        assert_eq!(view.text, "Марина и Артём Зайцев, pavel@vector.invalid");
        let span = |text: &str| {
            let start = view.text.find(text).unwrap();
            &source[view.source_range(start..start + text.len())]
        };
        assert_eq!(span("Марина"), "Ма\u{ad}рина");
        assert_eq!(span("рина"), "рина");
        assert_eq!(span("Ма"), "Ма");
        assert_eq!(span("Артём Зайцев"), "Арте\u{308}м Заи\u{306}цев");
        assert_eq!(span("ё"), "е\u{308}");
        assert_eq!(View::new("Mae\u{308}la Runnell").text, "Maëla Runnell");
        assert_eq!(span("pavel@vector.invalid"), "pavel@\u{200b}vector.invalid");
        // A word split by a line-end hyphen reads whole; a hyphen before a
        // capital, a digit or a dash at a line's end stays.
        let source = "Melora Brin-\nsett, Верес-\r\n  това, Ли-\nВельт, 000-\n74, list -\nnext, ksen@\n svod.invalid";
        let view = View::new(source);
        assert_eq!(
            view.text,
            "Melora Brinsett, Верестова, Ли-\nВельт, 000-\n74, list -\nnext, ksen@svod.invalid"
        );
        // A wording reads the same, its other line breaks kept (#388).
        assert_eq!(
            unwrapped(
                "Верес-\n  това Дарья, Санкт-\nПетербург, list -\nnext, 000-\n74, Ли-\r\nвельт"
            ),
            "Верестова Дарья, Санкт-\nПетербург, list -\nnext, 000-\n74, Ливельт"
        );
        assert!(matches!(
            unwrapped("Верес-\n> това"),
            std::borrow::Cow::Borrowed("Верес-\n> това")
        ));
        // Link destinations read "%20" and "_" between letters as spaces,
        // and percent-encoded letters as letters; visible text keeps them.
        let source = "[a](x.invalid/Liora%20Nesket/%D0%90%D0%BD%D0%BD%D0%B0_Ёлкина?q=%3D) a_b Liora%20Nesket ![s](Alice_Morgan.png \"t_t\")";
        let view = View::new(source);
        assert_eq!(
            view.text,
            "[a](x.invalid/Liora Nesket/Анна Ёлкина?q=%3D) a_b Liora%20Nesket ![s](Alice Morgan.png \"t_t\")"
        );
        let span = |text: &str| {
            let start = view.text.find(text).unwrap();
            &source[view.source_range(start..start + text.len())]
        };
        assert_eq!(span("Анна Ёлкина"), "%D0%90%D0%BD%D0%BD%D0%B0_Ёлкина");
        assert_eq!(span("Liora Nesket"), "Liora%20Nesket");
        // Note markers stand apart and underscore emphasis drops out.
        let source = "Avelina Durnell¹, _Alva Ruskett_ and snake_case x². Mail _roman@kadry.invalid or ann_@x.invalid.";
        let view = View::new(source);
        assert_eq!(
            view.text,
            "Avelina Durnell ¹, Alva Ruskett and snake_case x ². Mail _roman@kadry.invalid or ann_@x.invalid."
        );
        let at = |text: &str| view.text.find(text).unwrap();
        let span = |text: &str| &source[view.source_range(at(text)..at(text) + text.len())];
        assert_eq!(span("Avelina Durnell"), "Avelina Durnell");
        assert_eq!(span("Alva Ruskett"), "Alva Ruskett");
        // Full-width forms read as ASCII.
        let view = View::new("Паспорт ４６８２\u{3000}７３１６０４.");
        assert_eq!(view.text, "Паспорт 4682 731604.");
        assert_eq!(view.source_range(15..26), 15..48);
        let view = View::new("Melora Brin-\nsett");
        let source = "Melora Brin-\nsett";
        let start = view.text.find("Melora").unwrap();
        assert_eq!(
            &source[view.source_range(start..start + "Melora Brinsett".len())],
            "Melora Brin-\nsett"
        );
        assert!(View::new("plain").is_source());
    }

    /// #400, #448: a letter-spaced name or label reads as its words, and a
    /// span over them maps to the spaced source.
    #[test]
    fn letter_spaced_words_read_whole() {
        let source =
            "Caption:\n\nD E L V A   R O S K\nИ Н Н: 508641732966\nС Н И Л С: 1\n| Ё Л К А |";
        let view = View::new(source);
        assert_eq!(
            view.text,
            "Caption:\n\nDELVA ROSK\nИНН: 508641732966\nСНИЛС: 1\n| ЁЛКА |"
        );
        let at = |text: &str| view.text.find(text).unwrap();
        let span = |text: &str| &source[view.source_range(at(text)..at(text) + text.len())];
        assert_eq!(span("DELVA ROSK"), "D E L V A   R O S K");
        assert_eq!(span("ROSK"), "R O S K");
        assert_eq!(span("DELVA"), "D E L V A");
        assert_eq!(span("ИНН"), "И Н Н");
        assert_eq!(span("508641732966"), "508641732966");
        let name = source.find("R O S K").unwrap();
        assert_eq!(&view.text[view.view_range(name..name + 7)], "ROSK");
        // Words two spaces apart, a column three spaces away, combining marks.
        assert_eq!(
            View::new("D E L V A  R O S K   Date").text,
            "DELVA ROSK   Date"
        );
        assert_eq!(View::new("Подпись: И\u{306} О Ж").text, "Подпись: ЙОЖ");
        assert_eq!(View::new("И Н Н¹: 5").text, "ИНН ¹: 5");
        assert_eq!(View::new("J R R¹ Tolkien").text, "J R R ¹ Tolkien");
        assert_eq!(
            View::new("**J R R** **Tolkien** and J R R\u{a0}Tolkien").text,
            "**J R R Tolkien** and J R R\u{a0}Tolkien"
        );
        for source in [
            // Initials and one-letter words next to a word.
            "J. R. R. Tolkien and J R R Tolkien",
            "J R R  Tolkien",
            "**J R R** Tolkien and J R R\tTolkien",
            "Smith *J R R*",
            "Я и я.",
            "D e l v a  R o s k",
            "Signed by A B C Holdings",
            "Пункты А Б В утратили силу.",
            "и в с к о у я",
            "Сторона А и Сторона Б, к о у",
            // Lowercase enumerations and two letters.
            "options a b c are open",
            "a b c",
            "A B",
            "A B   C D",
            // Table rows and separators.
            "| a | b | c |\n|---|---|---|\n|A|B|C|",
            "+ - - - +",
            // Letters of two scripts, or of mixed case.
            "C H И Л C",
            "A b C d",
            // Two spaces between letters are no letter spacing.
            "D  E  L  V  A",
        ] {
            assert!(
                View::new(source).is_source(),
                "{source}: {}",
                View::new(source).text
            );
        }
    }

    /// #448: a footnote reference reads as nothing, so a label keeps its
    /// value (`**ИНН**[^1]:`); a footnote's definition stays.
    #[test]
    fn footnote_references_drop_out() {
        let source = "__ИНН__[^1]: 508614372930\n**ИНН[^tax]:** 1\nKira Gelts[^n2] signed.\n\n[^1]: Значение.\n  [^tax]: Note.\n> [^q]: Quote.\n- [^l]: Item.\n12) [^n]: Item.\n[^ x] [^] [^a";
        let view = View::new(source);
        assert_eq!(
            view.text,
            "ИНН: 508614372930\n**ИНН:** 1\nKira Gelts signed.\n\n[^1]: Значение.\n  [^tax]: Note.\n> [^q]: Quote.\n- [^l]: Item.\n12) [^n]: Item.\n[^ x] [^] [^a"
        );
        let at = |text: &str| view.text.find(text).unwrap();
        let span = |text: &str| &source[view.source_range(at(text)..at(text) + text.len())];
        assert_eq!(span("Kira Gelts"), "Kira Gelts");
        assert_eq!(span("ИНН: 5"), "ИНН__[^1]: 5");
        assert_eq!(View::new("**ИНН**[^1]: 1").text, "**ИНН**: 1");
        // An underscore run past emphasis is a blank, and "\u{a0}" is one
        // character before an emphasis mark.
        assert_eq!(View::new("Name ____Ann").text, "Name ____Ann");
        assert_eq!(View::new("\u{a0}_Alva Ruskett_").text, "\u{a0}Alva Ruskett");
    }

    /// #397: a cell of an identifier column reads with its header; a name
    /// column, a totals row and a table-like line get nothing.
    #[test]
    fn identifier_columns_read_with_their_header() {
        let source = "| Фамилия | **Паспорт №** | Счёт: | Сумма |\n\
                      | --- | :-: | --- | --- |\n\
                      | Кий | 46 82 &#55;31609 | a\\|40817810 | 100 |\n\
                      | Итого |  | Итого | 100 |\n\
                      \n\
                      Не | таблица\n\
                      | Tax ID | Account No. | Policy holder |\r\n\
                      |---|---|---|\r\n\
                      | 83-4918276 | — | Oona Keld |\r\n";
        let view = View::new(source);
        assert_eq!(
            view.text,
            "| Фамилия | **Паспорт №** | Счёт: | Сумма |\n\
             | --- | :-: | --- | --- |\n\
             | Кий | Паспорт №: 46 82 731609 | Счёт: a\\|40817810 | 100 |\n\
             | Итого |  | Итого | 100 |\n\
             \n\
             Не | таблица\n\
             | Tax ID | Account No. | Policy holder |\r\n\
             |---|---|---|\r\n\
             | Tax ID: 83-4918276 | — | Oona Keld |\r\n"
        );
        let at = |text: &str| view.text.find(text).unwrap();
        let span = |range: Range<usize>| &source[view.source_range(range)];
        // A span over the header and the value maps to the cell.
        let passport = at("Паспорт №: 46");
        assert_eq!(span(passport..at(" | Счёт: a")), "46 82 &#55;31609");
        assert_eq!(span(at("Tax ID: 8")..at(" | —")), "83-4918276");
        // A span only over the header maps to nothing.
        let header = view.source_range(passport..at(": 46") + 2);
        assert!(header.is_empty());
        assert_eq!(header.start, source.find("46 82").unwrap());
        // The value alone maps as it reads, and the source maps back.
        let value = source.find("83-4918276").unwrap();
        let value = value..value + "83-4918276".len();
        assert_eq!(&view.text[view.view_range(value.clone())], "83-4918276");
        assert_eq!(view.source_range(view.view_range(value.clone())), value);
        assert!(View::new("| Фамилия | Имя |\n| --- | --- |\n| Кий | Ева |").is_source());
        // An even run of backslashes leaves the pipe a column break; a
        // heading or a list item ends the table; a delimiter takes one
        // colon at most on either side.
        assert_eq!(
            View::new("| Notes | ИНН | Name |\n| --- | --- | --- |\n| x\\\\| 508614729031 | Bob 1234567890 |\n# Итог | 1234567890\n- a | 1234567890").text,
            "| Notes | ИНН | Name |\n| --- | --- | --- |\n| x\\\\| ИНН: 508614729031 | Bob 1234567890 |\n# Итог | 1234567890\n- a | 1234567890"
        );
        assert!(View::new("| ИНН |\n| ::---:: |\n| 1234567890 |").is_source());
        // A code block's table and an indented one are code.
        assert!(View::new("~~~text\n| Phone |\n| --- |\n| 1234567890 |\n~~~").is_source());
        assert!(View::new("    | Phone |\n    | --- |\n    | 1234567890 |").is_source());
        assert_eq!(
            View::new("```\n| a |\n```\n| Phone |\n| --- |\n| 1234567890 |").text,
            "```\n| a |\n```\n| Phone |\n| --- |\n| Phone: 1234567890 |"
        );
        assert_eq!(
            View::new("| Phone |\n| --- |\n| &#8212; |\n| &#43;44 &#55;700 |").text,
            "| Phone |\n| --- |\n| — |\n| Phone: +44 7700 |"
        );
        assert_eq!(
            View::new("| ИНН |\n| --- |\n| 7701234567 |\n| Цена |\n| --- |\n| 1234567890 |").text,
            "| ИНН |\n| --- |\n| ИНН: 7701234567 |\n| Цена |\n| --- |\n| 1234567890 |"
        );
        assert!(View::new("# ИНН | x\n| --- | --- |\n| 1234567890 | 1 |").is_source());
        assert_eq!(header_name(" **Account No.:** "), "account");
        assert_eq!(header_name("Расчётный  счёт"), "расчетный счет");
    }

    /// #494: a target of another scheme is a reference into another
    /// program; its hashes and short numbers are hidden, its names, emails
    /// and long numbers read. Web, email and phone targets read as before.
    #[test]
    fn link_targets_of_other_schemes_hide_their_references() {
        let source = "[п. 2](consultantplus://offline/ref=0A1B2C3D4E5F60718293A4B5C6D7E8F9;dst=100012) \
            [ст. 5](garant://12345678/5) [скан](file:///C:/Users/Liora%20Nesket-2026/scan_7c2e9a41.pdf) \
            [ф](file:///C:/scans/7707083893.pdf) [чат](xmpp:ivan1985+legal@x.invalid?chat) \
            [д](C:/docs/12.pdf)";
        let view = View::new(source);
        assert_eq!(
            view.text,
            "[п. 2](consultantplus://offline/ref=;dst=) [ст. 5](garant:///) \
            [скан](file:///C:/Users/Liora Nesket-/scan_.pdf) \
            [ф](file:///C:/scans/7707083893.pdf) [чат](xmpp:ivan1985+legal@x.invalid?chat) \
            [д](C:/docs/12.pdf)"
        );
        // A span next to a hidden part maps back over all of it.
        let start = view.text.find("ref=;").unwrap();
        assert_eq!(
            &source[view.source_range(start..start + 5)],
            "ref=0A1B2C3D4E5F60718293A4B5C6D7E8F9;"
        );
        let start = view.text.find("Liora Nesket").unwrap();
        assert_eq!(
            &source[view.source_range(start..start + "Liora Nesket".len())],
            "Liora%20Nesket"
        );
        for read in [
            "[a](HTTP://x.invalid/7707083893)",
            "[m](mailto:alice_morgan1985@example.invalid)",
            "[t](tel:+79000008214)",
            "[s](sms:+7-900-000-82-14)",
            "[c](callto:+79000008214)",
        ] {
            assert_eq!(View::new(read).text, read);
        }
        assert_eq!(
            View::new("[a](https://x.invalid/case/7707083893/Liora%20Nesket)").text,
            "[a](https://x.invalid/case/7707083893/Liora Nesket)"
        );
        // An email written with a reference or an escape reads whole, and
        // an escaped letter counts as a letter ("Док12" is a reference).
        assert_eq!(
            View::new("[ч](xmpp:ivan1985&#64;x.invalid) [п](xmpp:ivan1985%40x.invalid)").text,
            "[ч](xmpp:ivan1985@x.invalid) [п](xmpp:ivan1985%40x.invalid)"
        );
        assert_eq!(
            View::new("[д](garant://%D0%94%D0%BE%D0%BA12/5)").text,
            "[д](garant://Док/)"
        );
        // A name glued to a number reads, a short hex run is no hash, an
        // email's local part holds "=", a number's groups join, and a
        // destination in angle brackets is the same target.
        assert_eq!(
            View::new(
                "[x](file:///clients/Liora_Nesket2026.pdf) [h](garant://ab12/fade1234) \
                [e](xmpp:alice1985=legal@x.invalid) [s](file:///snils:112-233-445%2095) \
                [p](file:///7%20(900)%20000-82-14/2026-11-02) \
                [a](<consultantplus://offline/ref=0A1B2C3D;dst=100012>)"
            )
            .text,
            "[x](file:///clients/Liora Nesket.pdf) [h](garant://ab/) \
            [e](xmpp:alice1985=legal@x.invalid) [s](file:///snils:112-233-445 95) \
            [p](file:///7 (900) 000-82-14/--) [a](<consultantplus://offline/ref=;dst=>)"
        );
    }
}
