// Exercise the shipping worker rather than the headless UI test adapter.
#[allow(dead_code)]
#[path = "../src/docx_preview.rs"]
mod docx_preview;

use docx_preview::{PreviewError, render_with_executable};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

#[test]
fn real_worker_renders_searchable_pdf_and_reports_failure() {
    let executable = Path::new(env!("CARGO_BIN_EXE_mdoc"));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import/text.docx");
    let original = std::fs::read(&source).unwrap();
    let preview = render_with_executable(
        &source,
        Arc::new(AtomicBool::new(false)),
        executable,
        Duration::from_secs(30),
    )
    .unwrap();
    let pdf = gpui_pdf::parse(Arc::new(std::fs::read(&preview.pdf_path).unwrap())).unwrap();
    assert!(!pdf.pages().is_empty());
    let text = gpui_pdf::extract_page_text(&pdf, 0).unwrap().text();
    assert!(!text.trim().is_empty());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let artifact = preview.pdf_path.clone();
    drop(preview);
    let cleanup_started = std::time::Instant::now();
    while artifact.exists() {
        assert!(cleanup_started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }

    let result = render_with_executable(
        Path::new("missing.docx"),
        Arc::new(AtomicBool::new(false)),
        executable,
        Duration::from_secs(30),
    );
    assert!(matches!(result, Err(PreviewError::Render(_))));
    assert!(
        !std::process::Command::new(executable)
            .arg("--mdoc-docx-worker")
            .status()
            .unwrap()
            .success()
    );
}

#[cfg(unix)]
#[test]
fn timeout_and_cancellation_kill_and_reap_worker() {
    use std::{os::unix::fs::PermissionsExt, sync::atomic::Ordering, time::Instant};
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("worker");
    let pid = dir.path().join("pid");
    std::fs::write(&executable, "#!/bin/sh\necho $$ > \"$2\"\nexec sleep 60\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let result = render_with_executable(
        &pid,
        Arc::new(AtomicBool::new(false)),
        &executable,
        Duration::from_secs(1),
    );
    assert!(matches!(result, Err(PreviewError::Timeout)));
    let assert_dead = || {
        let worker_pid = std::fs::read_to_string(&pid).unwrap();
        assert!(
            !std::process::Command::new("kill")
                .args(["-0", worker_pid.trim()])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    };
    assert_dead();
    std::fs::remove_file(&pid).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    std::thread::scope(|scope| {
        let cancel = cancelled.clone();
        let pid = &pid;
        scope.spawn(move || {
            let start = Instant::now();
            while !pid.exists() {
                assert!(start.elapsed() < Duration::from_secs(5));
                std::thread::sleep(Duration::from_millis(5));
            }
            cancel.store(true, Ordering::Relaxed);
        });
        let result = render_with_executable(pid, cancelled, &executable, Duration::from_secs(10));
        assert!(matches!(result, Err(PreviewError::Cancelled)));
    });
    assert_dead();
}

#[cfg(unix)]
#[test]
fn crash_and_false_success_are_failures() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("worker");
    for status in [0, 9] {
        std::fs::write(&executable, format!("#!/bin/sh\nexit {status}\n")).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(matches!(
            render_with_executable(
                Path::new("unused"),
                Arc::new(AtomicBool::new(false)),
                &executable,
                Duration::from_secs(5)
            ),
            Err(PreviewError::Render(_))
        ));
    }
}

#[test]
fn oversize_input_is_rejected_in_the_worker() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("oversized.docx");
    std::fs::File::create(&source)
        .unwrap()
        .set_len(50 * 1024 * 1024 + 1)
        .unwrap();
    let result = render_with_executable(
        &source,
        Arc::new(AtomicBool::new(false)),
        Path::new(env!("CARGO_BIN_EXE_mdoc")),
        Duration::from_secs(30),
    );
    assert!(matches!(result, Err(PreviewError::Render(message)) if message.contains("50 MiB")));
}

#[test]
fn coverage_corpus_preserves_text_links_and_landscape() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-preview/coverage.docx");
    let started = std::time::Instant::now();
    let preview = render_with_executable(
        &source,
        Arc::new(AtomicBool::new(false)),
        Path::new(env!("CARGO_BIN_EXE_mdoc")),
        Duration::from_secs(30),
    )
    .unwrap();
    let conversion = started.elapsed();
    let pdf = gpui_pdf::parse(Arc::new(std::fs::read(&preview.pdf_path).unwrap())).unwrap();
    let first_page = gpui_pdf::render_page(&pdf, 0, 1.0).unwrap();
    assert!(
        first_page
            .as_bytes(0)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[2] > 200 && p[0] < 50 && p[1] < 50),
        "embedded red image must be visible"
    );
    eprintln!(
        "coverage worker={conversion:?}, through first raster={:?}, pages={}",
        started.elapsed(),
        pdf.pages().len()
    );
    let text = (0..pdf.pages().len())
        .filter_map(|i| gpui_pdf::extract_page_text(&pdf, i))
        .map(|p| p.text())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "Qualification heading",
        "Привет мир",
        "Проверка поиска",
        "Merged horizontal",
        "Merged vertical",
        "Cell alpha",
        "Cell beta",
        "List level 0",
        "List level 1",
        "Example hyperlink",
        "Qualification header",
        "Qualification footer",
        "Footnote content",
        "Page marker 9",
        "Landscape final section",
    ] {
        assert!(
            text.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .contains(
                    &expected
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>()
                ),
            "missing {expected:?} in {text}"
        );
    }
    assert!(gpui_pdf::page_dims(&pdf).iter().any(|(w, h)| w > h));
    assert!(gpui_pdf::page_links(&pdf).iter().flatten().any(|link| matches!(&link.target, gpui_pdf::LinkTarget::Uri(uri) if uri == "https://example.com")));
    assert!(preview.warnings.is_empty(), "{:?}", preview.warnings);
}
