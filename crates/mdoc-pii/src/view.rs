//! The text detection and readiness read (ADR 0092, preprocessing): the
//! source with what only hides a value's characters undone, and a map from
//! its offsets back to the source's. HTML character references are decoded
//! ("Ann&#97; Petrova" reads "Anna Petrova"), soft hyphens and zero-width
//! characters drop out ("Ма\u{ad}рина" reads "Марина") and letters written
//! with combining marks compose, as NFC writes them ("е\u{308}" reads "ё").
//! The source itself never changes: spans found in the view map back to
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
        for (range, replacement) in rewrites(source) {
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
        } else if c == '&'
            && let Some((len, value)) = character_reference(&source[at..])
        {
            previous = value.chars().next_back();
            out.push((at..at + len, value));
            while chars.next_if(|&(i, _)| i < at + len).is_some() {}
        } else if invisible(c) {
            out.push((at..at + c.len_utf8(), String::new()));
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
                out.push((at..end, composed));
            }
        }
    }
    out
}

/// Link and image destinations ("](clients/Alice_Morgan)"): hidden source
/// that names people in its own spelling.
fn link_destinations(source: &str) -> Vec<Range<usize>> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for (open, _) in source.match_indices("](") {
        let start = open + 2;
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

/// Whether `at` in `destination` is inside an email address
/// ("mailto:alice_morgan@example.invalid"), whose underscores are its own.
fn in_email(source: &str, destination: &Range<usize>, at: usize) -> bool {
    const PARTS: [char; 6] = ['/', '?', '#', '&', '=', ':'];
    let start = source[destination.start..at]
        .rfind(PARTS)
        .map_or(destination.start, |i| destination.start + i + 1);
    let end = source[at..destination.end]
        .find(PARTS)
        .map_or(destination.end, |i| at + i);
    source[start..end].contains('@')
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
        let view = View::new("Melora Brin-\nsett");
        let source = "Melora Brin-\nsett";
        let start = view.text.find("Melora").unwrap();
        assert_eq!(
            &source[view.source_range(start..start + "Melora Brinsett".len())],
            "Melora Brin-\nsett"
        );
        assert!(View::new("plain").is_source());
    }
}
