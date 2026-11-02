//! The text detection and readiness read (ADR 0092, preprocessing): the
//! source with what only hides a value's characters undone, and a map from
//! its offsets back to the source's. HTML character references are decoded
//! ("Ann&#97; Petrova" reads "Anna Petrova"), soft hyphens and zero-width
//! characters drop out ("Ма\u{ad}рина" reads "Марина") and letters written
//! with combining marks compose, as NFC writes them ("е\u{308}" reads "ё").
//! A table cell under an identifier column's header reads with it
//! ("Паспорт: 46 82 731609"); the header is inserted, so a span only over it
//! maps to an empty source range. The source itself never changes: spans found in the view map back to
//! whole source characters.
use std::ops::Range;
use unicode_normalization::UnicodeNormalization;

/// The source as detection reads it, with its offsets mapped back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// The text the model and the rules read.
    pub text: String,
    /// Each rewritten source range and the view range it became, in order.
    edits: Vec<Edit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    source: Range<usize>,
    view: Range<usize>,
}

impl View {
    pub fn new(source: &str) -> Self {
        let mut text = String::with_capacity(source.len());
        let mut edits = Vec::new();
        let mut copied = 0;
        let mut rewrites = rewrites(source);
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
        Self { text, edits }
    }

    /// Whether the view reads the source unchanged.
    pub fn is_source(&self) -> bool {
        self.edits.is_empty()
    }

    /// The source range `range` of the view stands for. A boundary inside a
    /// rewritten part widens to all of it, so a span covers whole source
    /// characters ("&#97;", never "&#9").
    pub fn source_range(&self, range: Range<usize>) -> Range<usize> {
        self.source_offset(range.start, false)..self.source_offset(range.end, true)
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

/// Source ranges and what the view reads for each, in order, never
/// overlapping.
fn rewrites(source: &str) -> Vec<(Range<usize>, String)> {
    let mut out = Vec::new();
    let destinations = link_destinations(source);
    let mut destination = destinations.iter().peekable();
    let mut chars = source.char_indices().peekable();
    let mut previous = None;
    while let Some((at, c)) = chars.next() {
        let next = chars.peek().map(|&(_, next)| next);
        let before = previous;
        previous = Some(c);
        while destination.next_if(|range| range.end <= at).is_some() {}
        let hidden = destination.peek().filter(|range| range.start <= at);
        if hidden.is_some()
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
            && (!before.is_some_and(char::is_alphanumeric) && next.is_some_and(char::is_alphabetic)
                || before.is_some_and(char::is_alphabetic)
                    && !next.is_some_and(char::is_alphanumeric))
            && !in_address(source, at)
        {
            // "_Alva Ruskett_": emphasis marks are syntax, not the name's.
            out.push((at..at + 1, String::new()));
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
/// "Lead technician") makes the model read the header as part of the value.
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
fn header_name(header: &str) -> String {
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
        let columns = cells(header);
        if !header.contains('|')
            || block_start(header)
            || !delimiter_row(delimiter)
            || cells(delimiter).len() != columns.len()
        {
            row += 1;
            continue;
        }
        let names: Vec<Option<&str>> = columns
            .into_iter()
            .map(|cell| {
                let name = header[cell]
                    .trim_matches(|c: char| c.is_whitespace() || matches!(c, '*' | ':'));
                IDENTIFIER_HEADERS
                    .contains(&header_name(name).as_str())
                    .then_some(name)
            })
            .collect();
        row += 2;
        while let Some(&(start, line)) = lines
            .get(row)
            .filter(|(_, line)| line.contains('|') && !block_start(line))
        {
            for (cell, name) in cells(line).into_iter().zip(&names) {
                let Some(name) = name else { continue };
                let value = &line[cell.clone()];
                let content = start + cell.start + value.len() - value.trim_start().len();
                if value.contains(|c: char| c.is_numeric() || matches!(c, '@' | '＠')) {
                    out.push((content..content, format!("{name}: ")));
                }
            }
            row += 1;
        }
    }
    out
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
fn cells(line: &str) -> Vec<Range<usize>> {
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
/// UTF-8 bytes of one letter, "%D0%90") and its length.
fn percent_encoded(text: &str) -> Option<(usize, String)> {
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
    let value = String::from_utf8(bytes).ok()?;
    // Only what a name is spelt with: a space or a letter.
    value
        .chars()
        .all(|c| c == ' ' || c.is_alphabetic())
        .then_some((3 * len, value))
}

/// The length of a line-end hyphen that splits a word at the start of
/// `text` ("-\n" and the next line's indent before a lowercase letter, as
/// in "Brin-\nsett"), which the word reads without.
fn line_end_hyphen(text: &str) -> Option<usize> {
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
pub fn wrapped_value(source: &str, range: &Range<usize>) -> bool {
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
    let start = source[..at].rfind(char::is_whitespace).map_or(0, |i| i + 1);
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
        assert!(View::new("# ИНН | x\n| --- | --- |\n| 1234567890 | 1 |").is_source());
        assert_eq!(header_name(" **Account No.:** "), "account");
        assert_eq!(header_name("Расчётный  счёт"), "расчетный счет");
    }
}
