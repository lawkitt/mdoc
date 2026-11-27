//! The words several screens share, and `ui/`'s own: each string a user
//! reads here is a constant or a function, and one test keeps sentences out of
//! the view files of the folders it checks and synonyms CONTEXT.md forbids out
//! of every string the app shows.

/// The button that leaves a dialog or a card without acting.
pub(crate) const CANCEL: &str = "Cancel";
/// The button that closes a panel or a sheet.
pub(crate) const CLOSE: &str = "Close";
/// What a copy button says for a moment after it copied.
pub(crate) const COPIED: &str = "Copied";
/// The control that puts a notice or a hint away.
pub(crate) const DISMISS: &str = "Dismiss";
/// The answer that puts a question off.
pub(crate) const NOT_NOW: &str = "Not now";
/// A download card while its cancel is under way.
pub(crate) const CANCELLING: &str = "Cancelling…";
/// A download card before its first byte.
pub(crate) const PREPARING: &str = "Preparing…";
/// The confirm dialog's button that drops unsaved work.
pub(crate) const DONT_SAVE: &str = "Don't Save";
/// The confirm dialog's button that drops every unsaved change.
pub(crate) const DISCARD_ALL: &str = "Discard All";
/// How a quit that drops work starts its button's label.
pub(crate) const QUIT_WITHOUT: &str = "Quit without";

/// The palette row that cycles the theme, by the choice it switches to.
pub(crate) const SWITCH_TO_SYSTEM_THEME: &str = "Switch to system theme";
pub(crate) const SWITCH_TO_LIGHT_THEME: &str = "Switch to light theme";
pub(crate) const SWITCH_TO_DARK_THEME: &str = "Switch to dark theme";

/// A download card's progress.
pub(crate) fn downloading(percent: impl std::fmt::Display) -> String {
    format!("Downloading {percent}%")
}

/// What a screen reader adds to a disabled menu row: why it is off.
pub(crate) fn unavailable(reason: impl std::fmt::Display) -> String {
    format!("Unavailable. {reason}")
}

#[cfg(test)]
mod tests {
    //! The sentence test (#1319, decision 6).
    use std::path::{Path, PathBuf};

    /// Owner folders (and root files) not checked for sentences yet. An owner
    /// item that gives its folder a `text.rs` deletes its line here.
    const EXEMPT: &[&str] = &[
        "ai",
        "app",
        "commands",
        "document",
        "main",
        "review",
        "shelf",
        "workspace",
    ];

    /// Test-only files at the crate root, never checked.
    const TEST_FILES: &[&str] = &["exits.rs", "perf_tests.rs", "test_support.rs"];

    /// CONTEXT.md's avoided words the check skips: their bans are about naming
    /// a Project ("case", "matter") or the Shelf ("tray"), while the app's
    /// "Match case", its "case number" category and Windows' "system tray"
    /// are other words.
    const NOT_CHECKED: &[&str] = &["case", "matter", "tray"];

    fn src() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().unwrap() != "tests" {
                    rust_files(&path, out);
                }
            } else if path.extension().is_some_and(|e| e == "rs")
                && path.file_name().unwrap() != "tests.rs"
            {
                out.push(path);
            }
        }
    }

    /// The string literals outside comments, as (byte offset, contents),
    /// and which bytes are code (not a comment or a literal).
    fn lex(text: &str) -> (Vec<(usize, String)>, Vec<bool>) {
        let b = text.as_bytes();
        let mut code = vec![true; b.len()];
        let mut literals = Vec::new();
        let mut i = 0;
        let mark = |code: &mut Vec<bool>, from: usize, to: usize| {
            for c in &mut code[from..to] {
                *c = false;
            }
        };
        while i < b.len() {
            if b[i..].starts_with(b"//") {
                let end = text[i..].find('\n').map_or(b.len(), |n| i + n);
                mark(&mut code, i, end);
                i = end;
            } else if b[i..].starts_with(b"/*") {
                let (mut depth, start) = (0, i);
                loop {
                    if b[i..].starts_with(b"/*") {
                        depth += 1;
                        i += 2;
                    } else if b[i..].starts_with(b"*/") {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                mark(&mut code, start, i);
            } else if (b[i] == b'r' || b[i..].starts_with(b"br"))
                && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'))
                && {
                    let r = if b[i] == b'b' { i + 2 } else { i + 1 };
                    let hashes = b[r..].iter().take_while(|&&c| c == b'#').count();
                    b.get(r + hashes) == Some(&b'"')
                }
            {
                let r = if b[i] == b'b' { i + 2 } else { i + 1 };
                let hashes = b[r..].iter().take_while(|&&c| c == b'#').count();
                let open = r + hashes + 1;
                let close = format!("\"{}", "#".repeat(hashes));
                let end = open + text[open..].find(&close).unwrap();
                literals.push((i, text[open..end].to_string()));
                mark(&mut code, i, end + close.len());
                i = end + close.len();
            } else if b[i] == b'"' {
                let start = i;
                i += 1;
                while b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                literals.push((start, text[start + 1..i].to_string()));
                i += 1;
                mark(&mut code, start, i);
            } else if b[i] == b'\'' {
                // A char literal; a lifetime has no closing quote nearby.
                let rest = &text[i + 1..];
                let len = if let Some(escaped) = rest.strip_prefix('\\') {
                    escaped.find('\'').map(|n| n + 2)
                } else {
                    rest.chars()
                        .next()
                        .map(char::len_utf8)
                        .filter(|&n| rest[n..].starts_with('\''))
                };
                match len {
                    Some(n) => {
                        mark(&mut code, i, i + n + 2);
                        i += n + 2;
                    }
                    None => i += 1,
                }
            } else {
                i += text[i..].chars().next().unwrap().len_utf8();
            }
        }
        (literals, code)
    }

    /// The byte ranges of `#[cfg(test)]` items: they are not the app's text.
    fn test_ranges(text: &str, code: &[bool]) -> Vec<(usize, usize)> {
        let b = text.as_bytes();
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(n) = text[from..].find("#[cfg(test)]") {
            let at = from + n;
            from = at + 1;
            if !code[at] {
                continue;
            }
            let mut i = at + "#[cfg(test)]".len();
            while i < b.len() && !(code[i] && (b[i] == b'{' || b[i] == b';')) {
                i += 1;
            }
            if i < b.len() && b[i] == b'{' {
                let mut depth = 0;
                while i < b.len() {
                    if code[i] && b[i] == b'{' {
                        depth += 1;
                    } else if code[i] && b[i] == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    i += 1;
                }
            }
            out.push((at, i + 1));
        }
        out
    }

    /// The app's string literals, file by file, outside tests.
    fn app_literals() -> Vec<(PathBuf, Vec<String>)> {
        let mut files = Vec::new();
        rust_files(&src(), &mut files);
        files
            .into_iter()
            .filter(|path| {
                let rel = path.strip_prefix(src()).unwrap();
                !(rel.components().count() == 1 && TEST_FILES.contains(&rel.to_str().unwrap()))
            })
            .map(|path| {
                let text = std::fs::read_to_string(&path).unwrap();
                let (literals, code) = lex(&text);
                let tests = test_ranges(&text, &code);
                let kept = (literals.into_iter())
                    .filter(|(at, _)| !tests.iter().any(|&(a, b)| (a..b).contains(at)))
                    .map(|(_, literal)| literal)
                    .collect();
                (path, kept)
            })
            .collect()
    }

    /// A word of an identifier, such as `UiPanel`: two humps or more.
    fn identifier_like(token: &str) -> bool {
        let humps = token.chars().filter(char::is_ascii_uppercase).count();
        token.chars().all(|c| c.is_ascii_alphanumeric()) && humps >= 2
    }

    /// Words a user reads as a sentence: a capital first, and a lowercase
    /// word after a space, closing punctuation or a format placeholder.
    fn is_sentence(literal: &str) -> bool {
        let mut bare = String::new();
        let mut depth = 0;
        for c in literal.chars() {
            match c {
                '{' => depth += 1,
                '}' if depth > 0 => depth -= 1,
                _ if depth == 0 => bare.push(c),
                _ => {}
            }
        }
        let Some(first) = bare.chars().find(|c| c.is_alphabetic()) else {
            return false;
        };
        let words: Vec<&str> = (bare.split_whitespace())
            .filter(|w| w.chars().any(char::is_alphabetic))
            .collect();
        if !first.is_uppercase() || words.iter().all(|w| identifier_like(w)) {
            return false;
        }
        let lower_word =
            (bare.split(' ').skip(1)).any(|w| w.starts_with(|c: char| c.is_lowercase()));
        let closes = bare.trim_end().ends_with(['.', '!', '?', '…', ':']);
        lower_word || closes || literal.contains('{')
    }

    fn checked(path: &Path) -> bool {
        let rel = path.strip_prefix(src()).unwrap();
        let first = rel
            .components()
            .next()
            .unwrap()
            .as_os_str()
            .to_str()
            .unwrap();
        let owner = first.trim_end_matches(".rs");
        !EXEMPT.contains(&owner) && path.file_name().unwrap() != "text.rs"
    }

    /// CONTEXT.md's `Avoid "…"` words, less the ones the check skips.
    fn avoided() -> Vec<String> {
        let context = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../CONTEXT.md");
        let text = std::fs::read_to_string(context).unwrap();
        let mut out = Vec::new();
        for line in text.lines() {
            let Some(at) = line.find("Avoid ") else {
                continue;
            };
            let sentence = &line[at..line[at..].find('.').map_or(line.len(), |n| at + n)];
            for (i, part) in sentence.split('"').enumerate() {
                if i % 2 == 1 && !NOT_CHECKED.contains(&part.to_lowercase().as_str()) {
                    out.push(part.to_lowercase());
                }
            }
        }
        assert!(out.contains(&"anonymization".to_string()), "{out:?}");
        out
    }

    fn contains_word(literal: &str, word: &str) -> bool {
        let lower = literal.to_lowercase();
        lower.match_indices(word).any(|(at, _)| {
            let before = lower[..at].chars().next_back();
            let after = lower[at + word.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
    }

    /// Done when (#1319 item 11): a sentence in a view file of a folder not
    /// exempt, or a synonym CONTEXT.md forbids in a string the app shows,
    /// fails this test.
    #[test]
    fn views_hold_no_sentences_and_strings_no_forbidden_synonyms() {
        let avoided = avoided();
        let mut found = Vec::new();
        for (path, literals) in app_literals() {
            let rel = path.strip_prefix(src()).unwrap().display().to_string();
            for literal in literals {
                if checked(&path) && is_sentence(&literal) {
                    found.push(format!("{rel}: a sentence outside text.rs: {literal:?}"));
                }
                for word in &avoided {
                    if contains_word(&literal, word) {
                        found.push(format!("{rel}: CONTEXT.md avoids {word:?}: {literal:?}"));
                    }
                }
            }
        }
        assert!(found.is_empty(), "{}", found.join("\n"));
    }

    #[test]
    fn the_rules_read_sentences_and_identifiers_apart() {
        assert!(is_sentence("Downloading {}%"));
        assert!(is_sentence("Unavailable. {reason}"));
        assert!(is_sentence("Choose model…"));
        assert!(is_sentence("Keep mdoc in the system tray"));
        assert!(!is_sentence("Cancel"));
        assert!(!is_sentence("UiPanel UiMenu"));
        assert!(!is_sentence("{DIALOG} UiPanel"));
        assert!(!is_sentence("mod-shift-p"));
        assert!(contains_word("No anonymization here", "anonymization"));
        assert!(!contains_word("Match cases", "case"));
    }
}
