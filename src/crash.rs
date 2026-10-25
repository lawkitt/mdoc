//! Local crash reports the user may preview and send (ADR 0047). A panic hook
//! writes one masked report; the next launch offers it. Nothing is sent
//! automatically: Send opens a prefilled GitHub issue in the browser.
use mdoc_store::crash::{mask, safe_message};
use std::path::PathBuf;

const ISSUES: &str = "https://github.com/lawkitt/mdoc/issues/new";
/// Room left for the report in an issue URL; browsers and GitHub cut longer ones.
const URL_BODY_LIMIT: usize = 6000;

pub fn report_path() -> Option<PathBuf> {
    mdoc_store::app_paths::data_local_dir().map(|dir| dir.join("mdoc/crash-report.txt"))
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

#[cfg(test)]
mod tests {
    use super::*;

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
