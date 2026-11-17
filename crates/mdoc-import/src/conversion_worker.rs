//! The supervised conversion worker (desktop only, #925 decision 10): AnyDoc
//! converts every document other than a PDF in a child process, the app's
//! own executable in its `--mdoc-conversion-worker` mode, as the DOCX
//! preview does (ADR 0005).
//! The desktop release profile aborts on a panic, so a converter panic, stack
//! overflow, abort or failed allocation would otherwise end the app; here it
//! ends the worker, and the import fails with a plain error.
//!
//! The document's bytes go to the worker on stdin and its file extension as
//! an argument, so the worker never opens the user's file; it answers with
//! one versioned JSON message on stdout and exits 0. Nothing is written to
//! disk. The supervisor kills and reaps it on timeout, cancellation or app
//! quit; any other exit, or a missing or unreadable message, is a failed
//! converter.
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::import::{Converted, ImportError, contained, convert_bytes};

/// The argument that runs the app's executable as the conversion worker.
pub const WORKER_FLAG: &str = "--mdoc-conversion-worker";
/// The message format; the supervisor refuses any other.
const VERSION: u32 = 1;
/// The worker converts on a thread this large: deeply nested documents
/// recurse, and a process's main thread is 1 MiB on Windows.
const STACK_BYTES: usize = 8 * 1024 * 1024;
const POLL: Duration = Duration::from_millis(10);
/// The largest message the supervisor reads: the import limit, so a worker
/// cannot exhaust the app's memory with its answer.
pub const MAX_MESSAGE_BYTES: u64 = 256 * 1024 * 1024;
/// The most app quit waits for running workers to be killed and reaped.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(5);

static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// Converts every later non-PDF import in `executable`'s worker mode. The
/// app calls it once at launch with its own executable; without it
/// conversion stays in process (tests, the headless modes).
pub fn supervise(executable: PathBuf) {
    let _ = EXECUTABLE.set(executable);
}

/// The worker executable [`supervise`] installed, if any.
pub(crate) fn installed() -> Option<&'static Path> {
    EXECUTABLE.get().map(PathBuf::as_path)
}

/// Called off-thread by GPUI's graceful-quit hook: running workers are
/// killed and reaped, and no new one starts.
pub fn shutdown_workers() {
    // Sequentially consistent with `Running::enter` and `check_cancel`: a
    // supervision this does not count sees the flag and starts no worker.
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
    let started = Instant::now();
    while RUNNING.load(Ordering::SeqCst) != 0 && started.elapsed() < SHUTDOWN_WAIT {
        thread::sleep(Duration::from_millis(5));
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), ImportError> {
    if cancel.load(Ordering::Relaxed) || SHUTTING_DOWN.load(Ordering::SeqCst) {
        Err(ImportError::Cancelled)
    } else {
        Ok(())
    }
}

/// One message from the worker.
#[derive(serde::Serialize, serde::Deserialize)]
struct Message {
    version: u32,
    outcome: Outcome,
}

/// Read first, so a message of another version is refused by its number
/// rather than by a parse error.
#[derive(serde::Deserialize)]
struct Header {
    version: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Outcome {
    Converted(Converted),
    Failed(Failure),
}

/// The import errors conversion can end in, as the worker sends them.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "kebab-case")]
enum Failure {
    Encrypted,
    Unsupported,
    Damaged,
    ResourceLimit,
    Read(String),
    Convert(String),
    Converter(String),
}

impl From<ImportError> for Failure {
    fn from(error: ImportError) -> Self {
        match error {
            ImportError::Encrypted => Self::Encrypted,
            ImportError::Unsupported => Self::Unsupported,
            ImportError::Damaged => Self::Damaged,
            ImportError::ResourceLimit => Self::ResourceLimit,
            ImportError::Read(error) => Self::Read(error.to_string()),
            ImportError::Convert(detail) => Self::Convert(detail),
            ImportError::ConverterFailed(detail) => Self::Converter(detail),
            // `convert_bytes` ends in none of the others.
            other => Self::Convert(other.to_string()),
        }
    }
}

impl From<Failure> for ImportError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Encrypted => Self::Encrypted,
            Failure::Unsupported => Self::Unsupported,
            Failure::Damaged => Self::Damaged,
            Failure::ResourceLimit => Self::ResourceLimit,
            Failure::Read(detail) => Self::Read(std::io::Error::other(detail)),
            Failure::Convert(detail) => Self::Convert(detail),
            Failure::Converter(detail) => Self::ConverterFailed(detail),
        }
    }
}

/// Kills and reaps the worker however supervision ends.
struct Worker(std::process::Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Counts a supervision for [`shutdown_workers`] while it lasts.
struct Running;
impl Running {
    fn enter() -> Self {
        RUNNING.fetch_add(1, Ordering::SeqCst);
        Self
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Converts a document's `bytes` in `executable`'s worker mode.
pub fn convert_with_executable(
    bytes: Vec<u8>,
    extension: Option<&str>,
    cancel: &AtomicBool,
    executable: &Path,
    timeout: Duration,
) -> Result<Converted, ImportError> {
    convert_with_command(bytes, extension, cancel, Command::new(executable), timeout)
}

/// [`convert_with_executable`] with the worker's command prepared by the
/// caller (tests set its environment); the worker's arguments, stdio and
/// window flags are added here.
pub fn convert_with_command(
    bytes: Vec<u8>,
    extension: Option<&str>,
    cancel: &AtomicBool,
    mut command: Command,
    timeout: Duration,
) -> Result<Converted, ImportError> {
    // Counted before the shutdown check, so app quit either waits for this
    // worker or this sees the quit (`shutdown_workers`).
    let _running = Running::enter();
    check_cancel(cancel)?;
    command.arg(WORKER_FLAG);
    if let Some(extension) = extension {
        command.arg(extension);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut worker = Worker(command.spawn().map_err(|error| {
        ImportError::ConverterFailed(format!("the worker did not start: {error}"))
    })?);
    // Fed and drained from threads of their own, so a worker that stops
    // reading, or a message larger than the pipe, cannot stall supervision;
    // both end once the worker is gone.
    let stdin = worker.0.stdin.take();
    let feeder = thread::spawn(move || {
        if let Some(mut stdin) = stdin {
            let _ = stdin.write_all(&bytes);
        }
    });
    let stdout = worker.0.stdout.take();
    let oversized = std::sync::Arc::new(AtomicBool::new(false));
    let drain = thread::spawn({
        let oversized = oversized.clone();
        move || {
            let mut message = Vec::new();
            let read = stdout?
                .take(MAX_MESSAGE_BYTES + 1)
                .read_to_end(&mut message);
            if message.len() as u64 > MAX_MESSAGE_BYTES {
                oversized.store(true, Ordering::Relaxed);
                return None;
            }
            read.ok().map(|_| message)
        }
    });
    let started = Instant::now();
    let outcome = loop {
        if let Err(cancelled) = check_cancel(cancel) {
            break Err(cancelled);
        }
        if started.elapsed() >= timeout {
            break Err(ImportError::ConverterTimeout);
        }
        if oversized.load(Ordering::Relaxed) {
            break Err(ImportError::ConverterFailed(format!(
                "the worker's result is over {} MiB",
                MAX_MESSAGE_BYTES / 1024 / 1024
            )));
        }
        match worker.0.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(POLL),
            Err(error) => {
                break Err(ImportError::ConverterFailed(format!(
                    "the worker could not be watched: {error}"
                )));
            }
        }
    };
    drop(worker);
    let _ = feeder.join();
    let message = drain.join().ok().flatten();
    read_message(outcome?, message)
}

/// The result of a worker that exited with `status`, from its `message`.
fn read_message(status: ExitStatus, message: Option<Vec<u8>>) -> Result<Converted, ImportError> {
    let failed = |detail: String| Err(ImportError::ConverterFailed(detail));
    if !status.success() {
        return failed(format!("the worker stopped: {status}"));
    }
    let Some(text) = message.filter(|text| !text.is_empty()) else {
        return failed("the worker sent no result".into());
    };
    match serde_json::from_slice::<Header>(&text) {
        Ok(Header { version: VERSION }) => {}
        Ok(Header { version }) => {
            return failed(format!(
                "the worker sent message version {version}, not {VERSION}"
            ));
        }
        Err(error) => return failed(format!("the worker's result is unreadable: {error}")),
    }
    match serde_json::from_slice::<Message>(&text) {
        Ok(Message {
            outcome: Outcome::Converted(converted),
            ..
        }) => Ok(converted),
        Ok(Message {
            outcome: Outcome::Failed(failure),
            ..
        }) => Err(failure.into()),
        Err(error) => failed(format!("the worker's result is unreadable: {error}")),
    }
}

/// The worker mode: `args` follow [`WORKER_FLAG`] (the document's
/// extension, if it has one). Reads the document from stdin, converts it and
/// writes the message to stdout. Returns the process's exit code.
pub fn run_worker(args: &[std::ffi::OsString]) -> i32 {
    let extension = args
        .first()
        .and_then(|extension| extension.to_str())
        .map(str::to_owned);
    let mut bytes = Vec::new();
    if let Err(error) = std::io::stdin().lock().read_to_end(&mut bytes) {
        eprintln!("conversion worker could not read its input: {error}");
        return 1;
    }
    #[cfg(debug_assertions)]
    if let Some(code) = fault::inject() {
        return code;
    }
    let converting = thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(move || contained(|| convert_bytes(&bytes, extension.as_deref())));
    let result = match converting.map(thread::JoinHandle::join) {
        Ok(Ok(result)) => result,
        // `contained` catches a converter panic, so neither happens.
        Ok(Err(_)) => Err(ImportError::ConverterFailed(
            "the converter panicked".into(),
        )),
        Err(error) => {
            eprintln!("conversion worker could not start its thread: {error}");
            return 1;
        }
    };
    let outcome = match result {
        Ok(converted) => Outcome::Converted(converted),
        Err(error) => Outcome::Failed(error.into()),
    };
    let message_text = match serde_json::to_vec(&Message {
        version: VERSION,
        outcome,
    }) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("conversion worker could not encode its result: {error}");
            return 1;
        }
    };
    let mut stdout = std::io::stdout().lock();
    match stdout
        .write_all(&message_text)
        .and_then(|()| stdout.flush())
    {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("conversion worker could not write its result: {error}");
            1
        }
    }
}

/// Test faults for the supervisor's tests, in debug builds only: the
/// `MDOC_CONVERSION_WORKER_FAULT` variable makes the real worker fail the
/// way a broken converter would, after reading its input.
#[cfg(debug_assertions)]
mod fault {
    use std::io::Write;

    pub(super) const VARIABLE: &str = "MDOC_CONVERSION_WORKER_FAULT";

    /// The exit code to end with, for the faults that end without a
    /// message; the others never return.
    pub(super) fn inject() -> Option<i32> {
        match std::env::var(VARIABLE).ok()?.as_str() {
            // A release worker aborts on a panic; a debug one unwinds out of
            // `main` and exits with 101.
            "panic" => panic!("injected converter panic"),
            "abort" => std::process::abort(),
            "oom" => std::alloc::handle_alloc_error(std::alloc::Layout::new::<[u8; 1 << 20]>()),
            "overflow" => {
                std::hint::black_box(recurse(0));
                Some(1)
            }
            "sleep" => {
                std::thread::sleep(std::time::Duration::from_secs(120));
                Some(1)
            }
            "exit" => Some(3),
            // More output than the supervisor reads.
            "flood" => {
                let chunk = vec![b'x'; 1024 * 1024];
                let mut stdout = std::io::stdout().lock();
                while stdout.write_all(&chunk).is_ok() {}
                Some(1)
            }
            "garbage" => Some(write(b"\x00not a message")),
            "version" => Some(write(br#"{"version":999,"outcome":{}}"#)),
            _ => None,
        }
    }

    fn write(text: &[u8]) -> i32 {
        i32::from(std::io::stdout().write_all(text).is_err())
    }

    /// Recurses until the stack overflows.
    fn recurse(depth: u64) -> u64 {
        let frame = std::hint::black_box([depth; 256]);
        if frame[0] == u64::MAX {
            return 0;
        }
        recurse(depth + 1).wrapping_add(frame[255])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_keep_their_import_error() {
        for error in [
            ImportError::Encrypted,
            ImportError::Unsupported,
            ImportError::Damaged,
            ImportError::ResourceLimit,
            ImportError::Read(std::io::Error::other("gone")),
            ImportError::Convert("odd".into()),
            ImportError::ConverterFailed("panicked".into()),
        ] {
            let text = error.to_string();
            let sent = serde_json::to_vec(&Failure::from(error)).unwrap();
            let back = ImportError::from(serde_json::from_slice::<Failure>(&sent).unwrap());
            assert_eq!(back.to_string(), text);
        }
    }

    #[test]
    fn a_cancelled_import_never_starts_a_worker() {
        let result = convert_with_executable(
            b"text".to_vec(),
            Some("csv"),
            &AtomicBool::new(true),
            Path::new("no-such-executable"),
            Duration::from_secs(5),
        );
        assert!(matches!(result, Err(ImportError::Cancelled)));
    }

    #[test]
    fn a_worker_that_cannot_start_is_a_failed_converter() {
        let result = convert_with_executable(
            b"text".to_vec(),
            Some("csv"),
            &AtomicBool::new(false),
            Path::new("no-such-executable"),
            Duration::from_secs(5),
        );
        assert!(matches!(result, Err(ImportError::ConverterFailed(_))));
    }
}
