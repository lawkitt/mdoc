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

/// Write a masked report on panic, then run the default hook so the process
/// ends as before. Only the most recent report is kept.
pub fn install() {
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
            let report = compose(
                &message,
                &location,
                thread.name().unwrap_or("unnamed"),
                &std::backtrace::Backtrace::force_capture().to_string(),
            );
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&path, mask(&report, dirs::home_dir().as_deref()));
        }
        previous(info);
    }));
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
/// panic, in `assert_eq!` debug output, or plain in `panic!("{}", text)`. Keep
/// only the first line of a known standard-library message, cut before any
/// value it prints, with quoted parts masked unless they are code; withhold
/// everything else. The location and backtrace still place the crash.
fn safe_message(message: &str) -> String {
    const KNOWN: [&str; 8] = [
        "called `Option::unwrap()`",
        "called `Option::expect()`",
        "called `Result::unwrap()`",
        "called `Result::expect()`",
        "index out of bounds",
        "byte index ",
        "attempt to ",
        "assertion `",
    ];
    let first = message.lines().next().unwrap_or("");
    if !KNOWN.iter().any(|known| first.starts_with(known)) {
        return "(message withheld: it may contain document text)".into();
    }
    // `…on an `Err` value: <debug of the error>` — the error may hold text.
    let first = match first.find(" value: ") {
        Some(at) => &first[..at + " value".len()],
        None => first,
    };
    mask_quotes(first)
}

fn mask_quotes(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(open) = rest.find(['`', '\'', '"']) {
        let quote = rest[open..].chars().next().unwrap();
        let Some(len) = rest[open + 1..].find(quote) else {
            // An unmatched quote: drop the rest rather than guess.
            out.push_str(&rest[..open + 1]);
            out.push('…');
            return out;
        };
        let inner = &rest[open + 1..open + 1 + len];
        out.push_str(&rest[..open + 1]);
        let code = inner.contains("::")
            || inner.ends_with("()")
            || matches!(inner, "left == right" | "Err" | "None");
        out.push_str(if code { inner } else { "…" });
        out.push(quote);
        rest = &rest[open + 2 + len..];
    }
    out.push_str(rest);
    out
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
            "assertion `left == right` failed"
        );
        assert_eq!(
            safe_message("called `Result::unwrap()` on an `Err` value: Io(\"/Users/a/Иван.docx\")"),
            "called `Result::unwrap()` on an `Err` value"
        );
    }

    #[test]
    fn quoted_text_is_masked_but_code_kept() {
        assert_eq!(
            mask_quotes(
                "byte index 3 is not a char boundary; it is inside 'ж' (bytes 2..4) of `Иван Петров`"
            ),
            "byte index 3 is not a char boundary; it is inside '…' (bytes 2..4) of `…`"
        );
        assert_eq!(
            mask_quotes("called `Option::unwrap()` on a `None` value"),
            "called `Option::unwrap()` on a `None` value"
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
        assert!(report.contains("index out of bounds: '…'"));
        let url = issue_url(&"x".repeat(20_000));
        assert!(url.starts_with(ISSUES));
        assert!(url.len() < URL_BODY_LIMIT + 1000);
    }
}
