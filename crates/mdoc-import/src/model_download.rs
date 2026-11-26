//! Explicit bounded artifact acquisition. Progress counts real bytes only:
//! the current file, plus completed downloads against the planned bundle.
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
/// Why a download or its verification failed.
#[derive(Clone, Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("Download cancelled.")]
    Cancelled,
    #[error("Artifact size mismatch. Repair setup.")]
    SizeMismatch,
    #[error("Artifact integrity check failed. Repair setup.")]
    Integrity,
    #[error("network is disabled in headless renders")]
    NetworkDisabled,
    #[error("Invalid artifact path")]
    InvalidPath,
    #[error("Artifact download exceeded its pinned size.")]
    Oversize,
    /// Lookup, connection, timeout and dropped-transfer errors alike.
    #[error("Couldn't reach {host}. Check your internet connection, then Retry.")]
    Unreachable { host: String },
    /// On managed networks this usually means a proxy or firewall.
    #[error(
        "Couldn't make a secure connection to {host}. A proxy or firewall on this network may be blocking it."
    )]
    Tls { host: String },
    #[error("{host} answered the download with HTTP {code}. Retry later.")]
    Status { host: String, code: u16 },
    #[error("Couldn't download from {host}: {message}")]
    Other { host: String, message: String },
    #[error("{0}")]
    Io(Arc<io::Error>),
    #[error("{0}")]
    Persist(Arc<tempfile::PersistError>),
}
impl From<io::Error> for DownloadError {
    fn from(error: io::Error) -> Self {
        Self::Io(Arc::new(error))
    }
}
impl DownloadError {
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::SizeMismatch)
            || matches!(self, Self::Io(error) if error.kind() == io::ErrorKind::NotFound)
    }
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub phase: String,
    pub received: u64,
    pub total: u64,
    /// Bytes still missing when setup started; 0 when nothing was planned.
    pub planned: u64,
    /// Bytes of files downloaded and verified during this setup.
    pub done: u64,
}
impl State {
    pub fn downloading(&self) -> bool {
        self.phase.starts_with("Downloading") && self.total > 0
    }
    /// Bundle bytes received so far, never beyond the plan.
    pub fn overall(&self) -> (u64, u64) {
        if self.planned == 0 {
            return (self.received, self.total);
        }
        ((self.done + self.received).min(self.planned), self.planned)
    }
}
/// Bytes a setup still has to download: model files and a shared runtime.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    pub model: u64,
    pub runtime: u64,
    /// The whole bundle's size, model and runtime, with nothing installed.
    pub full: u64,
    /// An earlier release's ONNX Runtime is installed in place of the pinned
    /// one (`ocr::onnx_runtime_superseded`).
    pub replaces_runtime: bool,
}
impl Pending {
    pub fn total(self) -> u64 {
        self.model + self.runtime
    }
    /// Part of the bundle is already here (a cancelled setup keeps the files
    /// it finished, or another model installed the shared runtime).
    pub fn resumed(self) -> bool {
        self.total() > 0 && self.total() < self.full
    }
    /// The model is installed and only a newer shared runtime is missing,
    /// with an earlier one in its place: setup updates rather than sets up.
    pub fn runtime_update(self) -> bool {
        self.model == 0 && self.runtime > 0 && self.replaces_runtime
    }
    /// How setup cards name the download: "665 MB download" or, after a
    /// cancelled setup, "662 MB left to download".
    pub fn download_label(self) -> String {
        format!(
            "{} MB {}",
            megabytes(self.total()),
            if self.resumed() {
                "left to download"
            } else {
                "download"
            }
        )
    }
}
/// A cheap presence check by size; digests are verified during setup and use.
pub fn missing(path: &Path, bytes: u64) -> bool {
    std::fs::metadata(path).map_or(true, |m| m.len() != bytes)
}
pub fn megabytes(bytes: u64) -> u64 {
    bytes.div_ceil(1_000_000)
}
#[derive(Clone, Default)]
pub struct Progress {
    pub cancel: Arc<AtomicBool>,
    pub state: Arc<Mutex<State>>,
}
impl Progress {
    pub fn check(&self) -> Result<(), DownloadError> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(DownloadError::Cancelled)
        } else {
            Ok(())
        }
    }
    pub fn phase(&self, phase: &str) {
        self.state.lock().unwrap().phase = phase.into();
    }
}
pub fn verify(path: &Path, bytes: u64, sha: &str) -> Result<(), DownloadError> {
    let mut file = File::open(path)?;
    if file.metadata()?.len() != bytes {
        return Err(DownloadError::SizeMismatch);
    }
    let mut digest = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }
    if digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        != sha
    {
        return Err(DownloadError::Integrity);
    }
    Ok(())
}
/// Download `url` to `path` unless a verified copy is there: `bytes` long
/// with SHA-256 `sha`. The file grows as `<name>.partial` beside it, which a
/// failed or cancelled try keeps: the next try, or an automatic retry of a
/// dropped connection, resumes it with an HTTP `Range` request (#1288).
pub fn fetch(
    url: &str,
    bytes: u64,
    sha: &str,
    path: &Path,
    progress: &Progress,
) -> Result<(), DownloadError> {
    if mdoc_store::app_paths::isolated() {
        return Err(DownloadError::NetworkDisabled);
    }
    fetch_with(&agent(true), url, bytes, sha, path, progress, BACKOFF)
}

/// A dropped connection is tried again this many times, after
/// [`BACKOFF`], twice that, then four times, before its error shows. A try
/// that brought new bytes starts the count again: each makes progress.
const RETRIES: u32 = 3;
const BACKOFF: Duration = Duration::from_secs(1);

/// No limit on a whole download, which on a slow line takes as long as it
/// takes (a 600 s cap failed the 556 MB encoder below about 0.9 MB/s, #1288),
/// but one on each step, so a stalled connection fails within half a minute
/// and is retried rather than holding setup for minutes.
fn agent(https_only: bool) -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(https_only)
            .timeout_resolve(Some(Duration::from_secs(30)))
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .timeout_per_read(Some(Duration::from_secs(30)))
            .build(),
    )
}

fn fetch_with(
    agent: &ureq::Agent,
    url: &str,
    bytes: u64,
    sha: &str,
    path: &Path,
    progress: &Progress,
    backoff: Duration,
) -> Result<(), DownloadError> {
    progress.check()?;
    progress.phase("Verifying");
    if verify(path, bytes, sha).is_ok() {
        return Ok(());
    }
    let parent = path.parent().ok_or(DownloadError::InvalidPath)?;
    std::fs::create_dir_all(parent)?;
    let partial = partial_of(path)?;
    let mut restarted = false;
    loop {
        {
            let mut state = progress.state.lock().unwrap();
            state.phase = format!(
                "Downloading {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            state.received = 0;
            state.total = bytes;
        }
        let resumed = download(agent, url, bytes, &partial, progress, backoff)?;
        progress.phase("Verifying");
        match verify(&partial, bytes, sha) {
            Ok(()) => break,
            // Bytes kept from before may be another copy's, or damaged:
            // once, from zero.
            Err(DownloadError::Integrity) if resumed && !restarted => {
                std::fs::remove_file(&partial)?;
                restarted = true;
            }
            Err(error) => {
                let _ = std::fs::remove_file(&partial);
                return Err(error);
            }
        }
    }
    progress.check()?;
    std::fs::rename(&partial, path)?;
    let mut state = progress.state.lock().unwrap();
    state.done += bytes;
    state.received = 0;
    Ok(())
}

/// `<name>.partial`, beside `path`.
fn partial_of(path: &Path) -> Result<PathBuf, DownloadError> {
    let mut name = path
        .file_name()
        .ok_or(DownloadError::InvalidPath)?
        .to_owned();
    name.push(".partial");
    Ok(path.with_file_name(name))
}

/// Bring `partial` to `bytes` bytes from `url`, resuming what it holds and
/// retrying a dropped connection: whether any of it came from an earlier
/// try or connection.
fn download(
    agent: &ureq::Agent,
    url: &str,
    bytes: u64,
    partial: &Path,
    progress: &Progress,
    backoff: Duration,
) -> Result<bool, DownloadError> {
    let mut resumed = false;
    let mut failures = 0;
    loop {
        progress.check()?;
        let have = match std::fs::metadata(partial) {
            Ok(meta) => meta.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        if have > bytes {
            std::fs::remove_file(partial)?;
            continue;
        }
        resumed |= have > 0;
        if have == bytes {
            return Ok(resumed);
        }
        match attempt(agent, url, bytes, have, partial, progress)? {
            Attempt::Done => return Ok(resumed),
            Attempt::Restart => resumed = false,
            Attempt::Dropped { progressed, error } => {
                if progressed {
                    failures = 0;
                }
                failures += 1;
                if failures > RETRIES {
                    return Err(error);
                }
                wait(backoff * 2u32.pow(failures - 1), progress)?;
            }
        }
    }
}

/// How one request went.
enum Attempt {
    /// The file is whole.
    Done,
    /// The server can't serve the part asked for: from zero, at once.
    Restart,
    /// The connection failed or ended early; what it brought is kept.
    Dropped {
        progressed: bool,
        error: DownloadError,
    },
}

/// One request for what `partial` lacks: the rest with a `Range` request
/// when it holds `have` bytes. A server answering with the whole file (200)
/// starts it again, never appends.
fn attempt(
    agent: &ureq::Agent,
    url: &str,
    bytes: u64,
    have: u64,
    partial: &Path,
    progress: &Progress,
) -> Result<Attempt, DownloadError> {
    let mut request = agent.get(url);
    if have > 0 {
        request = request.header("Range", format!("bytes={have}-"));
    }
    let mut response = match request.call() {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(416)) if have > 0 => {
            std::fs::remove_file(partial)?;
            return Ok(Attempt::Restart);
        }
        Err(error) => {
            return match download_error(url, error) {
                error @ DownloadError::Unreachable { .. } => Ok(Attempt::Dropped {
                    progressed: false,
                    error,
                }),
                error => Err(error),
            };
        }
    };
    let start = match response.status().as_u16() {
        206 if starts_at(&response, have, bytes) => have,
        // Not the part asked for: nothing of it can be used.
        206 if have > 0 => {
            std::fs::remove_file(partial).or_else(ignore_missing)?;
            return Ok(Attempt::Restart);
        }
        206 => {
            return Ok(Attempt::Dropped {
                progressed: false,
                error: unreachable(url),
            });
        }
        _ => 0,
    };
    let mut file = open_partial(partial)?;
    file.set_len(start)?;
    file.seek(io::SeekFrom::Start(start))?;
    progress.state.lock().unwrap().received = start;
    let mut input = response.body_mut().as_reader();
    let mut buf = [0; 65536];
    let mut total = start;
    let dropped = |total: u64, file: &File| -> Result<Attempt, DownloadError> {
        file.sync_all()?;
        Ok(Attempt::Dropped {
            progressed: total > have,
            error: unreachable(url),
        })
    };
    loop {
        progress.check()?;
        // A transfer that drops midway is a network failure like any other.
        let Ok(n) = input.read(&mut buf) else {
            return dropped(total, &file);
        };
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > bytes {
            drop(file);
            std::fs::remove_file(partial)?;
            return Err(DownloadError::Oversize);
        }
        file.write_all(&buf[..n])?;
        progress.state.lock().unwrap().received = total;
    }
    if total < bytes {
        return dropped(total, &file);
    }
    file.sync_all()?;
    Ok(Attempt::Done)
}

/// The partial file, made owner-only like the temporary file it replaces.
fn open_partial(partial: &Path) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(false).write(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(partial)
}

fn ignore_missing(error: io::Error) -> io::Result<()> {
    match error.kind() {
        io::ErrorKind::NotFound => Ok(()),
        _ => Err(error),
    }
}

/// A 206's `Content-Range` is `bytes <have>-<bytes - 1>/<bytes>` (the
/// length may be `*`).
fn starts_at(response: &ureq::http::Response<ureq::Body>, have: u64, bytes: u64) -> bool {
    let Some(range) = (response.headers().get("content-range")).and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let Some((span, length)) = range
        .trim()
        .strip_prefix("bytes ")
        .and_then(|r| r.split_once('/'))
    else {
        return false;
    };
    let length_ok = length == "*" || length.parse() == Ok(bytes);
    span.split_once('-')
        .is_some_and(|(first, last)| first.parse() == Ok(have) && last.parse() == Ok(bytes - 1))
        && length_ok
}

/// Sleep `time`, waking for a cancel.
fn wait(time: Duration, progress: &Progress) -> Result<(), DownloadError> {
    let until = std::time::Instant::now() + time;
    loop {
        progress.check()?;
        let now = std::time::Instant::now();
        if now >= until {
            return Ok(());
        }
        std::thread::sleep((until - now).min(Duration::from_millis(50)));
    }
}
/// What a failed download means to the user. Lookup, connection, timeout and
/// dropped-transfer errors all mean "no usable network"; the raw resolver text
/// ("failed to lookup address information …") helps nobody. TLS failures are
/// kept apart: on managed networks they usually mean a proxy or firewall.
pub fn download_error(url: &str, error: ureq::Error) -> DownloadError {
    use ureq::Error;
    let host = host(url);
    match error {
        Error::Io(_)
        | Error::Timeout(_)
        | Error::HostNotFound
        | Error::ConnectionFailed
        | Error::BodyStalled => DownloadError::Unreachable { host },
        Error::Tls(_) | Error::Rustls(_) => DownloadError::Tls { host },
        Error::StatusCode(code) => DownloadError::Status { host, code },
        other => DownloadError::Other {
            host,
            message: other.to_string(),
        },
    }
}
pub fn unreachable(url: &str) -> DownloadError {
    DownloadError::Unreachable { host: host(url) }
}
fn host(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| "the download server".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A local server for one file: each request in turn, answered with
    /// the part a `Range` asks for when `ranges`, else the whole file, and
    /// cut after the bytes `cuts` names for that request. It records each
    /// request's range start.
    struct Server {
        url: String,
        asked: Arc<Mutex<Vec<Option<u64>>>>,
    }

    fn serve(body: Vec<u8>, ranges: bool, cuts: Vec<Option<usize>>) -> Server {
        use std::io::{BufRead, BufReader};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model.onnx", listener.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(Vec::new()));
        let log = asked.clone();
        std::thread::spawn(move || {
            for (index, stream) in listener.incoming().enumerate() {
                let Ok(mut stream) = stream else { return };
                let mut from = None;
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(range) = lower.strip_prefix("range: bytes=") {
                        from = range.trim().trim_end_matches('-').parse::<u64>().ok();
                    }
                }
                log.lock().unwrap().push(from);
                let start = if ranges {
                    from.unwrap_or(0) as usize
                } else {
                    0
                };
                let head = if ranges && from.is_some() {
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{}\r\n",
                        body.len() - 1,
                        body.len()
                    )
                } else {
                    "HTTP/1.1 200 OK\r\n".into()
                };
                let rest = &body[start..];
                let _ = write!(
                    stream,
                    "{head}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    rest.len()
                );
                let cut = cuts.get(index).copied().flatten().unwrap_or(rest.len());
                let _ = stream.write_all(&rest[..cut.min(rest.len())]);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        });
        Server { url, asked }
    }

    fn body() -> Vec<u8> {
        (0..300_000u32).map(|i| (i * 31 % 251) as u8).collect()
    }

    fn sha_of(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn get(server: &Server, body: &[u8], path: &Path) -> Result<(), DownloadError> {
        let wait = Duration::from_millis(1);
        let (bytes, sha) = (body.len() as u64, sha_of(body));
        fetch_with(
            &agent(false),
            &server.url,
            bytes,
            &sha,
            path,
            &Progress::default(),
            wait,
        )
    }

    /// Done when (#1288): a connection dropped halfway is retried, the
    /// retry asks for the rest only, and the whole passes its digest.
    #[test]
    fn a_dropped_download_resumes_where_it_stopped() {
        let body = body();
        let server = serve(body.clone(), true, vec![Some(120_000)]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        get(&server, &body, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(*server.asked.lock().unwrap(), [None, Some(120_000)]);
        assert!(!partial_of(&path).unwrap().exists());
    }

    /// #1288: a partial file left by an earlier try is resumed on the next.
    #[test]
    fn a_partial_left_by_an_earlier_try_is_resumed() {
        let body = body();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        // A third of it, then three retries that bring nothing.
        let failing = serve(
            body.clone(),
            true,
            vec![Some(100_000), Some(0), Some(0), Some(0)],
        );
        let error = get(&failing, &body, &path).unwrap_err();
        assert!(
            matches!(error, DownloadError::Unreachable { .. }),
            "{error}"
        );
        assert_eq!(
            *failing.asked.lock().unwrap(),
            [None, Some(100_000), Some(100_000), Some(100_000)]
        );
        assert!(!path.exists());
        let kept = std::fs::metadata(partial_of(&path).unwrap()).unwrap().len();
        assert_eq!(kept, 100_000);
        let server = serve(body.clone(), true, Vec::new());
        get(&server, &body, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(*server.asked.lock().unwrap(), [Some(kept)]);
    }

    /// #1288: a server that ignores `Range` and sends the whole file starts
    /// the file again; nothing is appended.
    #[test]
    fn a_server_ignoring_range_restarts_cleanly() {
        let body = body();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        std::fs::write(partial_of(&path).unwrap(), &body[..150_000]).unwrap();
        let server = serve(body.clone(), false, vec![Some(50_000)]);
        get(&server, &body, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(*server.asked.lock().unwrap(), [Some(150_000), Some(50_000)]);
    }

    /// #1288: a partial whose bytes are wrong (damaged, or another copy's)
    /// fails the digest, is deleted, and the file downloads once from zero.
    #[test]
    fn a_corrupt_partial_restarts_from_zero_once() {
        let body = body();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        std::fs::write(partial_of(&path).unwrap(), vec![0xAA; 150_000]).unwrap();
        let server = serve(body.clone(), true, Vec::new());
        get(&server, &body, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(*server.asked.lock().unwrap(), [Some(150_000), None]);
        // Wrong from zero too: the digest error, the partial gone.
        let other = dir.path().join("other.onnx");
        std::fs::write(partial_of(&other).unwrap(), vec![0xAA; 150_000]).unwrap();
        let wrong = serve(vec![0xBB; body.len()], true, Vec::new());
        let error = get(&wrong, &body, &other).unwrap_err();
        assert!(matches!(error, DownloadError::Integrity), "{error}");
        assert_eq!(*wrong.asked.lock().unwrap(), [Some(150_000), None]);
        assert!(!partial_of(&other).unwrap().exists());
        assert!(!other.exists());
    }

    /// #1288: a connection that keeps failing without bringing anything is
    /// tried three more times, then shows its error.
    #[test]
    fn a_connection_that_brings_nothing_gives_up_after_three_retries() {
        let body = body();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        let server = serve(body.clone(), true, vec![Some(0); 8]);
        let error = get(&server, &body, &path).unwrap_err();
        assert!(
            matches!(error, DownloadError::Unreachable { .. }),
            "{error}"
        );
        assert_eq!(server.asked.lock().unwrap().len(), 4);
    }
    #[test]
    fn download_failures_read_as_actions_not_resolver_text() {
        let url = "https://github.com/owner/repo/releases/download/v1/model.onnx";
        let lookup = std::io::Error::other(
            "failed to lookup address information: nodename nor servname provided, or not known",
        );
        let offline = "Couldn't reach github.com. Check your internet connection, then Retry.";
        for error in [
            ureq::Error::Io(lookup),
            ureq::Error::HostNotFound,
            ureq::Error::ConnectionFailed,
            ureq::Error::BodyStalled,
        ] {
            assert_eq!(download_error(url, error).to_string(), offline);
        }
        assert!(
            download_error(url, ureq::Error::Tls("bad certificate"))
                .to_string()
                .contains("proxy or firewall")
        );
        assert_eq!(
            download_error(url, ureq::Error::StatusCode(503)).to_string(),
            "github.com answered the download with HTTP 503. Retry later."
        );
        // A refused connection goes through the real fetch path, its
        // retries a moment apart.
        let dir = tempfile::tempdir().unwrap();
        let error = fetch_with(
            &agent(true),
            "https://127.0.0.1:1/model.onnx",
            1,
            "00",
            &dir.path().join("model.onnx"),
            &Progress::default(),
            Duration::from_millis(1),
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Couldn't reach 127.0.0.1. Check your internet connection, then Retry."
        );
    }
    #[test]
    fn verified_files_are_reused_offline_and_cancel_keeps_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model");
        std::fs::write(&path, b"abc").unwrap();
        let sha = Sha256::digest(b"abc")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let progress = Progress::default();
        fetch("https://invalid.invalid/model", 3, &sha, &path, &progress).unwrap();
        progress.cancel.store(true, Ordering::Relaxed);
        assert!(fetch("https://invalid.invalid/model", 3, &sha, &path, &progress).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"abc");
        assert_eq!(progress.state.lock().unwrap().done, 0);
    }
    #[test]
    fn overall_progress_counts_completed_files_within_the_plan() {
        let mut state = State {
            phase: "Downloading model.onnx".into(),
            received: 20,
            total: 50,
            planned: 100,
            done: 40,
        };
        assert!(state.downloading());
        assert_eq!(state.overall(), (60, 100));
        state.done = 90;
        assert_eq!(state.overall(), (100, 100));
        state.planned = 0;
        assert_eq!(state.overall(), (20, 50));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        assert!(missing(&path, 3));
        std::fs::write(&path, b"abc").unwrap();
        assert!(!missing(&path, 3));
        assert!(missing(&path, 4));
    }
}
