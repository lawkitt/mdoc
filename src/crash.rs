//! Local crash reports the user may preview and send (ADR 0047). A panic hook
//! writes one masked report; the next launch offers it. Nothing is sent
//! automatically: Send opens a prefilled GitHub issue in the browser.
use std::path::{Path, PathBuf};

const ISSUES: &str = "https://github.com/lawkitt/mdoc/issues/new";
/// Room left for the report in an issue URL; browsers and GitHub cut longer ones.
const URL_BODY_LIMIT: usize = 6000;

pub fn report_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join("mdoc/crash-report.txt"))
}

/// The program file's modification time at launch: a different one at panic
/// time means the binary was rebuilt or updated underneath the running app,
/// and backtrace symbols (read from the file then) are unreliable.
static LAUNCH_MTIME: std::sync::OnceLock<Option<std::time::SystemTime>> =
    std::sync::OnceLock::new();
/// What the window showed last, in words and numbers only (no text).
static UI_STATE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

fn exe_mtime() -> Option<std::time::SystemTime> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| std::fs::metadata(exe).ok())
        .and_then(|meta| meta.modified().ok())
}

/// Record the window's state for the next report, e.g. "document · panel open".
/// Callers pass words and numbers only, never document text or names.
pub fn note_state(state: String) {
    if let Ok(mut current) = UI_STATE.lock()
        && *current != state
    {
        *current = state;
    }
}

/// Write a masked report on panic, then run the default hook so the process
/// ends as before. Only the most recent report is kept.
pub fn install() {
    LAUNCH_MTIME.get_or_init(exe_mtime);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(path) = report_path() {
            let location = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_default();
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "(no message)".into());
            let thread = std::thread::current();
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            let mut report = compose(
                &message,
                &location,
                thread.name().unwrap_or("unnamed"),
                &backtrace,
            );
            let context = context(
                &backtrace,
                LAUNCH_MTIME.get().copied().flatten() != exe_mtime(),
                UI_STATE.lock().map(|s| s.clone()).unwrap_or_default(),
            );
            if !context.is_empty() {
                // Context goes before the long backtrace so it survives the URL cut.
                if let Some(at) = report.find("\n\nBacktrace:") {
                    report.insert_str(at, &format!("\n\n{context}"));
                }
            }
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&path, mask(&report, dirs::home_dir().as_deref()));
        }
        previous(info);
    }));
}

/// Lines that place a crash: the first frame in mdoc's own code, whether the
/// program changed since launch, and what the window showed.
fn context(backtrace: &str, rebuilt: bool, state: String) -> String {
    let mut lines = Vec::new();
    if let Some(frame) = first_own_frame(backtrace) {
        lines.push(format!("First mdoc frame: {frame}"));
    }
    if rebuilt {
        lines.push(
            "The program file changed after launch (rebuilt or updated): \
             backtrace symbols may be wrong; the panic location above is still right."
                .to_string(),
        );
    }
    if !state.is_empty() {
        lines.push(format!("Window: {state}"));
    }
    lines.join("\n")
}

/// The innermost backtrace frame in mdoc's workspace: its function and its
/// workspace-relative source line, e.g. `mdoc_editor::tables::shape_cell_lines
/// at crates/mdoc-editor/src/tables.rs:1290:21`.
fn first_own_frame(backtrace: &str) -> Option<String> {
    let root = env!("CARGO_MANIFEST_DIR");
    let mut lines = backtrace.lines().peekable();
    while let Some(line) = lines.next() {
        // "  12: crate::module::function" then "             at path:line:col".
        let Some((_, function)) = line.trim_start().split_once(": ") else {
            continue;
        };
        // Capturing inside the panic hook puts its closure before the panic's
        // callers. It reports the failure; it is not the failing frame.
        if function.starts_with("mdoc::crash::install::") {
            continue;
        }
        let Some(at) = lines
            .peek()
            .and_then(|l| l.trim_start().strip_prefix("at "))
        else {
            continue;
        };
        let relative = at
            .strip_prefix(root)
            .map(|p| p.trim_start_matches('/'))
            .or_else(|| at.strip_prefix("./"));
        if let Some(relative) =
            relative.filter(|p| p.starts_with("src/") || p.starts_with("crates/"))
        {
            return Some(format!("{function} at {relative}"));
        }
    }
    None
}

fn compose(message: &str, location: &str, thread: &str, backtrace: &str) -> String {
    format!(
        "mdoc {} — {} {}\nPanic in thread '{thread}' at {location}:\n{}\n\nBacktrace:\n{backtrace}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        safe_message(message),
    )
}

/// The pending report, masked again in case an older build wrote it.
pub fn pending() -> Option<String> {
    let text = std::fs::read_to_string(report_path()?).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| mask(text, dirs::home_dir().as_deref()))
}

pub fn discard() {
    // Tests never touch the real app data.
    if cfg!(test) {
        return;
    }
    if let Some(path) = report_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// A new GitHub issue prefilled with the report, cut to fit a URL.
pub fn issue_url(report: &str) -> String {
    let mut body = report.to_string();
    if body.len() > URL_BODY_LIMIT {
        let mut cut = URL_BODY_LIMIT;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        body.truncate(cut);
        body.push_str("\n… (cut; use Copy for the full report)");
    }
    let body = format!(
        "<!-- Check that nothing here is confidential before submitting. -->\n\n```\n{body}\n```\n"
    );
    url::Url::parse_with_params(ISSUES, [("title", "Crash report"), ("body", body.as_str())])
        .map(String::from)
        .unwrap_or_else(|_| ISSUES.into())
}

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
fn safe_message(message: &str) -> String {
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

    #[test]
    fn context_names_the_first_mdoc_frame_and_a_rebuilt_binary() {
        let root = env!("CARGO_MANIFEST_DIR");
        let backtrace = format!(
            "   0: mdoc::crash::install::{{{{closure}}}}\n             at {root}/src/crash.rs:57:29\n\
             \x20  1: std::panicking::begin_panic\n             at /rustc/abc/library/std/src/panicking.rs:10:5\n\
             \x20  2: gpui_macos::text_system::layout_line\n             at /Users/a/.cargo/registry/src/gpui-pre-macos/src/text_system.rs:543:17\n\
             \x20  3: mdoc_editor::element::EditorElement::prepaint\n             at {root}/crates/mdoc-editor/src/element.rs:964:58\n\
             \x20  4: mdoc::main\n             at ./src/main.rs:157:13\n"
        );
        let lines = context(&backtrace, true, "Markdown, 10 bytes, zoom 100%".into());
        assert!(lines.contains(
            "First mdoc frame: mdoc_editor::element::EditorElement::prepaint at crates/mdoc-editor/src/element.rs:964:58"
        ), "{lines}");
        assert!(lines.contains("program file changed after launch"));
        assert!(lines.contains("Window: Markdown, 10 bytes, zoom 100%"));
        assert_eq!(context(&backtrace, false, String::new()).lines().count(), 1);
        // A report keeps the context ahead of the backtrace.
        let report = compose("x", "src/a.rs:1:1", "main", "frames");
        assert!(report.contains("\n\nBacktrace:"));
        assert!(
            first_own_frame("0: mdoc::crash::install::{{closure}}\n    at ./src/crash.rs:57:29\n")
                .is_none()
        );
    }

    #[test]
    fn a_report_names_the_version_and_the_issue_url_is_bounded() {
        let report = compose(
            "index out of bounds: 'secret'",
            "src/a.rs:1:1",
            "main",
            "frames",
        );
        assert!(report.starts_with(&format!("mdoc {}", env!("CARGO_PKG_VERSION"))));
        let url = issue_url(&"x".repeat(20_000));
        assert!(url.starts_with(ISSUES));
        assert!(url.len() < URL_BODY_LIMIT + 1000);
    }
}
