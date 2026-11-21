// The supervised conversion worker (#925 decision 10), through the shipping
// executable: the debug build's worker takes test faults from
// MDOC_CONVERSION_WORKER_FAULT.
use mdoc_import::{
    conversion_worker::{convert_with_command, convert_with_executable, supervise},
    import::{Converted, ImportError, convert_bytes, prepare_cancellable},
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

fn executable() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_mdoc"))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/import")
        .join(name)
}

/// `text.docx` converted by a worker with `fault`, and how long it took.
fn with_fault(
    fault: &str,
    cancel: &AtomicBool,
    timeout: Duration,
) -> (Result<Converted, ImportError>, Duration) {
    let mut command = Command::new(executable());
    command.env("MDOC_CONVERSION_WORKER_FAULT", fault);
    let started = Instant::now();
    let result = convert_with_command(
        std::fs::read(fixture("text.docx")).unwrap(),
        Some("docx"),
        cancel,
        command,
        timeout,
    );
    (result, started.elapsed())
}

/// Every non-PDF import fixture, and a CSV and an unknown file: the worker
/// gives byte-identical Markdown, comments and counts, or the same error, as
/// converting in this process.
#[test]
fn the_worker_converts_as_the_app_did_in_process() {
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("values.CSV");
    std::fs::write(&csv, "Name,Count\nApples,2\n").unwrap();
    let unknown = dir.path().join("unknown.bin");
    std::fs::write(&unknown, b"unknown format").unwrap();
    let mut paths = vec![csv, unknown];
    for entry in std::fs::read_dir(fixture("")).unwrap() {
        let path = entry.unwrap().path();
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !["md", "pdf", "py", "anydoc"].contains(&extension) {
            paths.push(path);
        }
    }
    paths.push(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-preview/comments.docx"),
    );
    assert!(paths.len() >= 16, "{paths:?}");
    let mut converted = 0;
    for path in paths {
        let bytes = std::fs::read(&path).unwrap();
        let extension = path.extension().and_then(|e| e.to_str());
        let in_process = convert_bytes(&bytes, extension).map_err(|e| e.to_string());
        let in_worker = convert_with_executable(
            bytes,
            extension,
            &AtomicBool::new(false),
            executable(),
            Duration::from_secs(60),
        )
        .map_err(|e| e.to_string());
        converted += usize::from(in_process.is_ok());
        assert_eq!(in_worker, in_process, "{}", path.display());
        // Everything the left-out line reads arrives: the comments and the
        // hidden passages of the comments fixture, for one.
        if path.ends_with("comments.docx") {
            let in_worker = in_worker.unwrap();
            assert_eq!(
                (
                    in_worker.comments.len(),
                    in_worker.comments_left_out,
                    in_worker.hidden_text_left_out,
                ),
                (6, 6, 1)
            );
        }
    }
    assert!(converted >= 12, "{converted}");
}

/// A panic, abort, failed allocation, stack overflow or bare non-zero
/// exit ends the worker, not this process, and reads as a failed converter.
#[test]
fn a_crashing_worker_is_an_import_error() {
    for fault in ["panic", "abort", "oom", "overflow", "exit"] {
        let (result, elapsed) = with_fault(fault, &AtomicBool::new(false), Duration::from_secs(60));
        let Err(error @ ImportError::ConverterFailed(_)) = result else {
            panic!("{fault}: {result:?}");
        };
        assert!(
            error
                .to_string()
                .starts_with("The converter stopped unexpectedly"),
            "{error}"
        );
        assert!(elapsed < Duration::from_secs(50), "{fault}: {elapsed:?}");
    }
}

/// A worker that exits cleanly with garbage, or a message of another
/// version, is a failed converter too.
#[test]
fn a_garbled_result_is_an_import_error() {
    for fault in ["garbage", "version"] {
        let (result, _) = with_fault(fault, &AtomicBool::new(false), Duration::from_secs(60));
        assert!(
            matches!(&result, Err(ImportError::ConverterFailed(_))),
            "{fault}: {result:?}"
        );
    }
}

/// A worker that answers with more than the supervisor reads is stopped,
/// and the import fails instead of exhausting this process's memory.
#[test]
fn an_oversized_result_is_an_import_error() {
    let (result, elapsed) = with_fault("flood", &AtomicBool::new(false), Duration::from_secs(100));
    assert!(
        matches!(&result, Err(ImportError::ConverterFailed(detail)) if detail.contains("MiB")),
        "{result:?}"
    );
    assert!(elapsed < Duration::from_secs(60), "{elapsed:?}");
}

/// A worker past its timeout is killed and reaped: supervision waits for
/// it to end, so returning well before its two-minute sleep proves it.
#[test]
fn a_worker_past_its_timeout_is_stopped() {
    let (result, elapsed) = with_fault("sleep", &AtomicBool::new(false), Duration::from_secs(2));
    assert!(
        matches!(result, Err(ImportError::ConverterTimeout)),
        "{result:?}"
    );
    assert!(elapsed < Duration::from_secs(60), "{elapsed:?}");
    assert!(
        ImportError::ConverterTimeout
            .to_string()
            .contains("longer than 2 minutes")
    );
}

/// Cancelling the import kills and reaps the running worker.
#[test]
fn cancelling_the_import_stops_the_worker() {
    let cancel = AtomicBool::new(false);
    let (result, elapsed) = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(500));
            cancel.store(true, Ordering::Relaxed);
        });
        with_fault("sleep", &cancel, Duration::from_secs(100))
    });
    assert!(matches!(result, Err(ImportError::Cancelled)), "{result:?}");
    assert!(elapsed < Duration::from_secs(60), "{elapsed:?}");
}

/// Once the app installs the worker, every import other than a PDF goes
/// through it (here, one that cannot start), and PDFs stay with
/// pdf-inspector in this process. The only test in this binary that uses
/// the installed worker.
#[test]
fn imports_other_than_pdfs_go_through_the_installed_worker() {
    supervise(PathBuf::from("no-such-worker-executable"));
    let cancel = AtomicBool::new(false);
    assert!(matches!(
        prepare_cancellable(&fixture("text.docx"), None, None, true, &cancel),
        Err(ImportError::ConverterFailed(_))
    ));
    let pdf = prepare_cancellable(&fixture("text.pdf"), None, None, true, &cancel).unwrap();
    assert!(pdf.is_pdf);
}
