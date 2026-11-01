//! The text detection and readiness read (ADR 0092, preprocessing): the
//! source with what only hides a value's characters undone, and a map from
//! its offsets back to the source's. HTML character references are decoded
//! ("Ann&#97; Petrova" reads "Anna Petrova"). The source itself never
//! changes: spans found in the view map back to whole source characters.
use std::ops::Range;

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
    let mut at = 0;
    while let Some(offset) = source[at..].find('&') {
        let start = at + offset;
        match character_reference(&source[start..]) {
            Some((len, value)) => {
                out.push((start..start + len, value));
                at = start + len;
            }
            None => at = start + 1,
        }
    }
    out
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
        assert!(View::new("plain").is_source());
    }
}
