//! Crash-report masking (ADR 0047): a report keeps the crash's place and
//! never document text, folder names or document names.
use std::path::Path;

/// Absolute paths lose their folders (`<path>/main.rs:12`, or
/// `~/…/main.rs` under the home folder), so a backtrace stays useful without
/// folder names.
pub fn mask(text: &str, home: Option<&Path>) -> String {
    let mut text = text.to_string();
    if let Some(home) = home.and_then(Path::to_str).filter(|h| h.len() > 1) {
        text = text.replace(home, "~");
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(start) = path_start(rest) {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | ')' | ','))
            .unwrap_or(tail.len());
        let path = &tail[..end];
        // Folder names can name clients: drop them.
        let name = path.rsplit(['/', '\\']).next().unwrap_or("");
        // Source files locate the crash; any other name may be a document's.
        let name = if name.split(':').next().is_some_and(|n| n.ends_with(".rs")) {
            name
        } else {
            "<file>"
        };
        out.push_str(if path.starts_with('~') {
            "~/…/"
        } else {
            "<path>/"
        });
        out.push_str(name);
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// The start of the next absolute path: `/a/b`, `~/a`, or `C:\a`.
fn path_start(text: &str) -> Option<usize> {
    let b = text.as_bytes();
    (0..b.len()).find(|&i| {
        let boundary = i == 0 || b[i - 1].is_ascii_whitespace() || b"(\"'`=".contains(&b[i - 1]);
        boundary
            && ((b[i] == b'/'
                && b.get(i + 1)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'.'))
                && text[i + 1..]
                    .split_whitespace()
                    .next()
                    .is_some_and(|w| w.contains('/'))
                || (b[i] == b'~' && b.get(i + 1) == Some(&b'/'))
                || (b[i].is_ascii_alphabetic()
                    && b.get(i + 1) == Some(&b':')
                    && b.get(i + 2) == Some(&b'\\')))
    })
}

/// A panic message can carry document text in any form: quoted by a slicing
/// panic (and the text may itself contain quotes), in `assert_eq!` debug
/// output, or plain in `panic!("{}", text)`, even text that happens to start
/// like a standard message. So the report names a known standard-library
/// panic with a fixed label and keeps nothing of the message itself; any
/// other message is withheld. The location and backtrace still place the crash.
pub fn safe_message(message: &str) -> String {
    let first = message.lines().next().unwrap_or("");
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    // Arithmetic panics are matched whole: they carry no values.
    const ARITHMETIC: [&str; 9] = [
        "attempt to add with overflow",
        "attempt to subtract with overflow",
        "attempt to multiply with overflow",
        "attempt to divide by zero",
        "attempt to divide with overflow",
        "attempt to calculate the remainder with a divisor of zero",
        "attempt to negate with overflow",
        "attempt to shift left with overflow",
        "attempt to shift right with overflow",
    ];
    let label = if let Some(known) = ARITHMETIC.iter().find(|a| first == **a) {
        known
    } else if first == "called `Option::unwrap()` on a `None` value" {
        "called `Option::unwrap()` on a `None` value"
    } else if first.starts_with("called `Result::unwrap()` on an `Err` value") {
        "called `Result::unwrap()` on an `Err` value (value withheld)"
    } else if first.starts_with("assertion `left == right` failed")
        || first.starts_with("assertion `left != right` failed")
    {
        "assertion failed (values withheld)"
    } else if first
        .strip_prefix("index out of bounds: the len is ")
        .and_then(|rest| rest.split_once(" but the index is "))
        .is_some_and(|(len, index)| digits(len) && digits(index))
    {
        "index out of bounds"
    } else if let Some(slice) = slicing_panic(message) {
        return slice;
    } else if first.starts_with("byte index ") {
        "string sliced at an invalid byte index (text withheld)"
    } else {
        "(message withheld: it may contain document text)"
    };
    label.to_string()
}

/// A string-slicing panic, kept as numbers: which index, the character it
/// fell inside (its byte range, size and script, never the character), and
/// the string's length when the message gives it. For `str::split_at(n)`,
/// the index is `n`, e.g. the length of a text run being shaped. Covers the
/// messages of Rust 1.99 and earlier.
fn slicing_panic(message: &str) -> Option<String> {
    let first = message.lines().next().unwrap_or("");
    let number = |s: &str| s.parse::<usize>().ok();
    // "start|end byte index N is out of bounds for string of length L"
    for side in ["start", "end"] {
        if let Some(rest) = first.strip_prefix(&format!("{side} byte index ")) {
            if let Some((n, len)) = rest.split_once(" is out of bounds for string of length ") {
                let (n, len) = (number(n)?, number(len)?);
                return Some(format!(
                    "string sliced out of bounds: {side} byte index {n}, string length {len} bytes"
                ));
            }
            // "... N is not a char boundary; it is inside 'c' (bytes A..B of string)"
            let (n, inside) = rest.split_once(" is not a char boundary; it is inside ")?;
            let n = number(n)?;
            let range = inside
                .rsplit_once(" (bytes ")?
                .1
                .strip_suffix(" of string)")?;
            return Some(format!(
                "string sliced inside a character: {side} byte index {n}; {}",
                describe_char(inside, range)?
            ));
        }
    }
    // "byte range starts at A but ends at B"
    if let Some(rest) = first.strip_prefix("byte range starts at ") {
        let (a, b) = rest.split_once(" but ends at ")?;
        return Some(format!(
            "string sliced with a reversed range: {}..{}",
            number(a)?,
            number(b)?
        ));
    }
    // Older: "byte index N is not a char boundary; it is inside 'c' (bytes A..B) of `text`"
    let rest = first.strip_prefix("byte index ")?;
    let (n, tail) = rest.split_once(" is not a char boundary; it is inside ")?;
    let n = number(n)?;
    let (inside, _) = tail.split_once(") of `")?;
    let (inside, range) = inside.rsplit_once(" (bytes ")?;
    let text = message[message.find(") of `")? + 6..].trim_end_matches('`');
    Some(format!(
        "string sliced inside a character: byte index {n}; {}; string ~{} bytes",
        describe_char(&format!("{inside} (bytes {range})"), range)?,
        text.len().max(1),
    ))
}

/// "a 2-byte Cyrillic character at bytes 4..6", from `'и' (bytes 4..6...` and
/// the range text. The character itself is never kept.
fn describe_char(inside: &str, range: &str) -> Option<String> {
    let c = inside.strip_prefix('\'')?.chars().next()?;
    let (a, b) = range.split_once("..")?;
    let (a, b) = (a.parse::<usize>().ok()?, b.parse::<usize>().ok()?);
    let script = match c as u32 {
        0x0000..=0x007F => "ASCII",
        0x0080..=0x024F => "Latin",
        0x0370..=0x03FF => "Greek",
        0x0400..=0x052F => "Cyrillic",
        0x0590..=0x05FF => "Hebrew",
        0x0600..=0x06FF => "Arabic",
        0x2000..=0x206F => "punctuation",
        0x20A0..=0x20CF => "currency",
        0x2100..=0x2BFF => "symbol",
        0x1F000..=0x1FAFF => "emoji",
        _ => "other",
    };
    Some(format!(
        "inside a {}-byte {script} character at bytes {a}..{b}",
        c.len_utf8()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_lose_folders_and_non_source_names() {
        let home = Path::new("/Users/anna");
        let text = "at /Users/anna/Developer/mdoc/src/main.rs:12:5\n\
                    at /rustc/abc/library/core/src/panicking.rs:75\n\
                    opened C:\\Users\\anna\\Contract.docx today";
        assert_eq!(
            mask(text, Some(home)),
            "at ~/…/main.rs:12:5\n\
             at <path>/panicking.rs:75\n\
             opened <path>/<file> today"
        );
        assert_eq!(mask("ratio 1/2 and a/b", None), "ratio 1/2 and a/b");
    }

    #[test]
    fn only_known_panic_messages_are_kept() {
        let withheld = "(message withheld: it may contain document text)";
        assert_eq!(safe_message("Иван Петров signed"), withheld);
        assert_eq!(safe_message("unexpected \"Иван\""), withheld);
        assert_eq!(
            safe_message("assertion `left == right` failed\n  left: \"Иван\"\n right: \"Пётр\""),
            "assertion failed (values withheld)"
        );
        assert_eq!(
            safe_message("called `Result::unwrap()` on an `Err` value: Io(\"/Users/a/Иван.docx\")"),
            "called `Result::unwrap()` on an `Err` value (value withheld)"
        );
    }

    #[test]
    fn known_panics_get_fixed_labels() {
        assert_eq!(
            safe_message(
                "byte index 1 is not a char boundary; it is inside 'é' (bytes 0..2) of `é`CLIENT_SECRET`"
            ),
            "string sliced inside a character: byte index 1; inside a 2-byte Latin character \
             at bytes 0..2; string ~16 bytes"
        );
        assert_eq!(
            safe_message("index out of bounds: the len is 3 but the index is 5"),
            "index out of bounds"
        );
        assert_eq!(
            safe_message("index out of bounds: the client Иван"),
            "(message withheld: it may contain document text)"
        );
        assert_eq!(
            safe_message("attempt to settle the dispute with Иван"),
            "(message withheld: it may contain document text)"
        );
        assert_eq!(
            safe_message("attempt to add with overflow"),
            "attempt to add with overflow"
        );
    }

    #[test]
    fn slicing_panics_keep_numbers_but_never_the_text() {
        // Rust 1.99's split_at/slice messages.
        let inside = safe_message(
            "end byte index 37 is not a char boundary; it is inside 'и' (bytes 36..38 of string)",
        );
        assert_eq!(
            inside,
            "string sliced inside a character: end byte index 37; inside a 2-byte Cyrillic \
             character at bytes 36..38"
        );
        assert!(!inside.contains('и'));
        assert_eq!(
            safe_message("start byte index 9 is out of bounds for string of length 4"),
            "string sliced out of bounds: start byte index 9, string length 4 bytes"
        );
        assert_eq!(
            safe_message("byte range starts at 6 but ends at 2"),
            "string sliced with a reversed range: 6..2"
        );
        // Anything that does not parse exactly is withheld.
        assert_eq!(
            safe_message("end byte index of Иван Петров"),
            "(message withheld: it may contain document text)"
        );
        let old = safe_message(
            "byte index 3 is not a char boundary; it is inside 'ж' (bytes 2..4) of `Секрет Иванова`",
        );
        assert!(!old.contains("Секрет") && !old.contains('ж'), "{old}");
    }

    /// The messages this toolchain really produces, not copies of them.
    #[test]
    fn real_slicing_panics_are_recognised() {
        let message = |f: fn()| {
            let payload = std::panic::catch_unwind(f).unwrap_err();
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap()
        };
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let split = message(|| {
            let text = std::hint::black_box(String::from("Сильвестрова"));
            let _ = text.split_at(std::hint::black_box(3));
        });
        let range = message(|| {
            let text = std::hint::black_box(String::from("ab"));
            let _ = &text[std::hint::black_box(5)..];
        });
        std::panic::set_hook(hook);
        let split = safe_message(&split);
        assert!(
            split.starts_with("string sliced inside a character"),
            "{split}"
        );
        assert!(
            split.contains("byte index 3") && split.contains("2-byte Cyrillic"),
            "{split}"
        );
        assert!(!split.contains("Сильвестрова"));
        assert!(safe_message(&range).starts_with("string sliced out of bounds"));
    }
}
