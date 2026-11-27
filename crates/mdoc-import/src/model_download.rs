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
    fetch_with(
        &agent(true, LIMITS),
        url,
        bytes,
        sha,
        path,
        progress,
        LIMITS,
    )
}

/// How long a download waits, and for what (#1288).
#[derive(Clone, Copy)]
struct Limits {
    /// Before the first retry of a dropped connection; each next one waits
    /// twice as long.
    backoff: Duration,
    /// For each read from the network: a stalled connection fails then.
    read: Duration,
    /// Without the file growing, across requests and waits: then its
    /// error shows, whatever retries are left.
    stall: Duration,
}

/// No limit on a whole download, which on a slow line takes as long as it
/// takes (a 600 s cap failed the 556 MB encoder below about 0.93 MB/s,
/// #1288), but one on each step, and a stalled connection shows its error
/// within about a minute rather than holding setup for minutes.
const LIMITS: Limits = Limits {
    backoff: Duration::from_secs(1),
    read: Duration::from_secs(30),
    stall: Duration::from_secs(60),
};

/// A dropped connection is tried again this many times, after
/// `Limits::backoff`, twice that, then four times, before its error shows.
/// A try that grows the file past its longest yet starts the count again.
const RETRIES: u32 = 3;

fn agent(https_only: bool, limits: Limits) -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(https_only)
            .timeout_resolve(Some(Duration::from_secs(30)))
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(limits.read * 2))
            .timeout_per_read(Some(limits.read))
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
    limits: Limits,
) -> Result<(), DownloadError> {
    progress.check()?;
    progress.phase("Verifying");
    if verify(path, bytes, sha).is_ok() {
        return Ok(());
    }
    let parent = path.parent().ok_or(DownloadError::InvalidPath)?;
    std::fs::create_dir_all(parent)?;
    // Another mdoc of this profile may be setting up the same file.
    let _held = hold(&beside(path, ".lock")?, progress)?;
    if verify(path, bytes, sha).is_ok() {
        return Ok(());
    }
    let partial = beside(path, ".partial")?;
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
        let resumed = download(agent, url, bytes, &partial, progress, limits)?;
        progress.phase("Verifying");
        match verify(&partial, bytes, sha) {
            Ok(()) => break,
            // Bytes kept from an earlier response may be another copy's, or
            // damaged: once, from zero.
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

/// `<name><suffix>`, beside `path`.
fn beside(path: &Path, suffix: &str) -> Result<PathBuf, DownloadError> {
    let mut name = path
        .file_name()
        .ok_or(DownloadError::InvalidPath)?
        .to_owned();
    name.push(suffix);
    Ok(path.with_file_name(name))
}

/// An exclusive lock on the file `lock`, held while the returned file
/// lives: another process taking it waits, waking for a cancel. The file
/// stays, empty.
pub fn hold(lock: &Path, progress: &Progress) -> Result<File, DownloadError> {
    let file = owner_only(lock)?;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => wait(Duration::from_millis(200), progress)?,
            Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
        }
    }
}

/// Bring `partial` to `bytes` bytes from `url`, resuming what it holds and
/// retrying a dropped connection: whether the whole holds bytes of an
/// earlier response, from this try or one before.
fn download(
    agent: &ureq::Agent,
    url: &str,
    bytes: u64,
    partial: &Path,
    progress: &Progress,
    limits: Limits,
) -> Result<bool, DownloadError> {
    let length = || match std::fs::metadata(partial) {
        Ok(meta) => Ok(meta.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    };
    let mut failures = 0;
    // The longest the file has been, and when it last grew past that.
    let (mut longest, mut grew) = (0, std::time::Instant::now());
    loop {
        progress.check()?;
        let have = length()?;
        if have > bytes {
            std::fs::remove_file(partial)?;
            continue;
        }
        if have == bytes {
            return Ok(true);
        }
        longest = longest.max(have);
        let error = match attempt(agent, url, bytes, have, partial, progress)? {
            Attempt::Done { kept } => return Ok(kept),
            Attempt::Failed(error) => error,
        };
        // Only a file that grows past its longest counts as progress: a
        // server that restarts it, or refuses the part asked for, can't
        // keep this going.
        let now = length()?;
        if now > longest {
            (longest, grew) = (now, std::time::Instant::now());
            failures = 0;
        }
        failures += 1;
        if failures > RETRIES || grew.elapsed() >= limits.stall {
            return Err(error);
        }
        wait(limits.backoff * 2u32.pow(failures - 1), progress)?;
    }
}

/// How one request went.
enum Attempt {
    /// The file is whole; `kept` when it holds bytes of an earlier response.
    Done { kept: bool },
    /// The connection failed or ended early, or the server couldn't send
    /// the part asked for; what the file holds is kept.
    Failed(DownloadError),
}

/// One request for what `partial` lacks: the rest with a `Range` request
/// when it holds `have` bytes. A server answering with the whole file (200)
/// starts it again, never appends; one refusing the part (416), or sending
/// another, starts it again on the next request.
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
            return Ok(Attempt::Failed(unreachable(url)));
        }
        Err(error) => {
            return match download_error(url, error) {
                error @ DownloadError::Unreachable { .. } => Ok(Attempt::Failed(error)),
                error => Err(error),
            };
        }
    };
    let start = match response.status().as_u16() {
        206 if starts_at(&response, have, bytes) => have,
        // Not the part asked for: nothing of it can be used.
        206 => {
            std::fs::remove_file(partial).or_else(ignore_missing)?;
            return Ok(Attempt::Failed(unreachable(url)));
        }
        _ => 0,
    };
    let mut file = owner_only(partial)?;
    file.set_len(start)?;
    file.seek(io::SeekFrom::Start(start))?;
    progress.state.lock().unwrap().received = start;
    let mut input = response.body_mut().as_reader();
    let mut buf = [0; 65536];
    let mut total = start;
    loop {
        progress.check()?;
        // A transfer that drops midway is a network failure like any other.
        let Ok(n) = input.read(&mut buf) else {
            file.sync_all()?;
            return Ok(Attempt::Failed(unreachable(url)));
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
    file.sync_all()?;
    if total < bytes {
        return Ok(Attempt::Failed(unreachable(url)));
    }
    Ok(Attempt::Done { kept: start > 0 })
}
/// `path` opened to write, made owner-only like the temporary file the
/// partial replaces; nothing in it is cut.
fn owner_only(path: &Path) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(false).write(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
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
    /// How a test server answers a `Range` request.
    #[derive(Clone, Copy, PartialEq)]
    enum Ranges {
        /// With the part asked for (206).
        Serve,
        /// With the whole file (200).
        Ignore,
        /// With 416.
        Refuse,
    }

    /// A local server for one file, answering each request in turn as
    /// `ranges` says, and cutting it after the bytes `cuts` names for it;
    /// with `hang`, a cut request then waits, its connection open. It
    /// records each request's range start.
    struct Server {
        url: String,
        asked: Arc<Mutex<Vec<Option<u64>>>>,
    }

    fn serve(body: Vec<u8>, ranges: Ranges, cuts: Vec<Option<usize>>, hang: bool) -> Server {
        use std::io::{BufRead, BufReader};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model.onnx", listener.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(Vec::new()));
        let log = asked.clone();
        let body = Arc::new(body);
        std::thread::spawn(move || {
            for (index, stream) in listener.incoming().enumerate() {
                let Ok(mut stream) = stream else { return };
                let (log, body, cuts) = (log.clone(), body.clone(), cuts.clone());
                // Each on its own thread, so a hanging one holds up no other.
                std::thread::spawn(move || {
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
                    if ranges == Ranges::Refuse && from.is_some() {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                        return;
                    }
                    let start = match (ranges, from) {
                        (Ranges::Serve, Some(from)) => from as usize,
                        _ => 0,
                    };
                    let head = if start > 0 {
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
                    let cut = cuts.get(index).copied().flatten();
                    let _ = stream.write_all(&rest[..cut.unwrap_or(rest.len()).min(rest.len())]);
                    let _ = stream.flush();
                    if hang && cut.is_some() {
                        std::thread::sleep(Duration::from_secs(5));
                    }
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                });
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

    /// The real limits, but retries a moment apart.
    const QUICK: Limits = Limits {
        backoff: Duration::from_millis(1),
        ..LIMITS
    };

    fn get_with(
        server: &Server,
        body: &[u8],
        path: &Path,
        limits: Limits,
    ) -> Result<(), DownloadError> {
        let (bytes, sha) = (body.len() as u64, sha_of(body));
        let agent = agent(false, limits);
        fetch_with(
            &agent,
            &server.url,
            bytes,
            &sha,
            path,
            &Progress::default(),
            limits,
        )
    }

    fn get(server: &Server, body: &[u8], path: &Path) -> Result<(), DownloadError> {
        get_with(server, body, path, QUICK)
    }

    fn partial(path: &Path) -> PathBuf {
        beside(path, ".partial").unwrap()
    }

    /// Done when (#1288): a connection dropped halfway is retried, the
    /// retry asks for the rest only, and the whole passes its digest.
    #[test]
    fn a_dropped_download_resumes_where_it_stopped() {
        let body = body();
        let server = serve(body.clone(), Ranges::Serve, vec![Some(120_000)], false);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        get(&server, &body, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(*server.asked.lock().unwrap(), [None, Some(120_000)]);
        assert!(!partial(&path).exists());
    }

    /// #1288: a partial file left by an earlier try is resumed on the next.
    #[test]
    fn a_partial_left_by_an_earlier_try_is_resumed() {
        let body = body();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        // A third of it, then three retries that bring nothing.
        let cuts = vec![Some(100_000), Some(0), Some(0), Some(0)];
        let failing = serve(body.clone(), Ranges::Serve, cuts, false);
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
        assert_eq!(std::fs::metadata(partial(&path)).unwrap().len(), 100_000);
        let server = serve(body.clone(), Ranges::Serve, Vec::new(), false);
        get(&server, &body, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(*server.asked.lock().unwrap(), [Some(100_000)]);
    }

    /// #1288: a server that ignores `Range` and sends the whole file starts
    /// the file again; nothing is appended.
    #[test]
    fn a_server_ignoring_range_restarts_cleanly() {
        let body = body();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        std::fs::write(partial(&path), &body[..150_000]).unwrap();
        let server = serve(body.clone(), Ranges::Ignore, vec![Some(50_000)], false);
        get(&server, &body, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(*server.asked.lock().unwrap(), [Some(150_000), Some(50_000)]);
    }

    /// Review of #1324: a server that cuts each whole file at the same
    /// place, and ignores or refuses the rest, can't keep setup retrying:
    /// the file never grows past its longest.
    #[test]
    fn a_server_that_never_gets_further_gives_up() {
        let body = body();
        for ranges in [Ranges::Ignore, Ranges::Refuse] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("model.onnx");
            let server = serve(body.clone(), ranges, vec![Some(120_000); 20], false);
            let error = get(&server, &body, &path).unwrap_err();
            assert!(
                matches!(error, DownloadError::Unreachable { .. }),
                "{error}"
            );
            let asked = server.asked.lock().unwrap().len();
            assert!(asked <= 6, "{asked} requests");
        }
    }

    /// #1288: a partial whose bytes are wrong (damaged, or another copy's)
    /// fails the digest, is deleted, and the file downloads once from zero.
    #[test]
    fn a_corrupt_partial_restarts_from_zero_once() {
        let body = body();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        std::fs::write(partial(&path), vec![0xAA; 150_000]).unwrap();
        let server = serve(body.clone(), Ranges::Serve, Vec::new(), false);
        get(&server, &body, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(*server.asked.lock().unwrap(), [Some(150_000), None]);
        // Wrong from zero too: the digest error, the partial gone.
        let other = dir.path().join("other.onnx");
        std::fs::write(partial(&other), vec![0xAA; 150_000]).unwrap();
        let wrong = serve(vec![0xBB; body.len()], Ranges::Serve, Vec::new(), false);
        let error = get(&wrong, &body, &other).unwrap_err();
        assert!(matches!(error, DownloadError::Integrity), "{error}");
        assert_eq!(*wrong.asked.lock().unwrap(), [Some(150_000), None]);
        assert!(!partial(&other).exists());
        assert!(!other.exists());
    }

    /// Review of #1324: a whole file sent again over a partial (Range
    /// ignored) holds no bytes of before: a wrong digest fails at once,
    /// with no second download.
    #[test]
    fn a_wrong_file_sent_whole_over_a_partial_is_not_downloaded_twice() {
        let body = body();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        std::fs::write(partial(&path), &body[..150_000]).unwrap();
        let wrong = serve(vec![0xBB; body.len()], Ranges::Ignore, Vec::new(), false);
        let error = get(&wrong, &body, &path).unwrap_err();
        assert!(matches!(error, DownloadError::Integrity), "{error}");
        assert_eq!(*wrong.asked.lock().unwrap(), [Some(150_000)]);
    }

    /// #1288: a connection that keeps failing without bringing anything is
    /// tried three more times, then shows its error.
    #[test]
    fn a_connection_that_brings_nothing_gives_up_after_three_retries() {
        let body = body();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        let server = serve(body.clone(), Ranges::Serve, vec![Some(0); 8], false);
        let error = get(&server, &body, &path).unwrap_err();
        assert!(
            matches!(error, DownloadError::Unreachable { .. }),
            "{error}"
        );
        assert_eq!(server.asked.lock().unwrap().len(), 4);
    }

    /// #1288 (review of #1324): a connection that stalls, open but sending
    /// nothing, shows its error once the file hasn't grown for the stall
    /// limit, whatever retries are left, and keeps what came.
    #[test]
    fn a_stalled_connection_gives_up_within_the_stall_limit() {
        let body = body();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        let server = serve(
            body.clone(),
            Ranges::Serve,
            vec![Some(50_000), Some(0), Some(0), Some(0)],
            true,
        );
        let limits = Limits {
            backoff: Duration::from_millis(1),
            read: Duration::from_millis(300),
            stall: Duration::from_millis(500),
        };
        let started = std::time::Instant::now();
        let error = get_with(&server, &body, &path, limits).unwrap_err();
        assert!(
            matches!(error, DownloadError::Unreachable { .. }),
            "{error}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        // The first stall, then retries until the limit.
        let asked = server.asked.lock().unwrap().clone();
        assert_eq!(asked[..2], [None, Some(50_000)]);
        assert!(asked.len() <= 3, "{asked:?}");
        assert_eq!(std::fs::metadata(partial(&path)).unwrap().len(), 50_000);
    }

    /// Review of #1324: a second process setting up the same file waits
    /// for the first's lock, and a cancel ends the wait.
    #[test]
    fn the_lock_waits_for_its_holder_and_a_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join("model.onnx.lock");
        let first = hold(&lock, &Progress::default()).unwrap();
        let progress = Progress::default();
        let waiting = {
            let (lock, progress) = (lock.clone(), progress.clone());
            std::thread::spawn(move || hold(&lock, &progress).map(drop))
        };
        std::thread::sleep(Duration::from_millis(300));
        assert!(!waiting.is_finished(), "took a held lock");
        progress.cancel.store(true, Ordering::Relaxed);
        assert!(matches!(
            waiting.join().unwrap(),
            Err(DownloadError::Cancelled)
        ));
        let next = {
            let lock = lock.clone();
            std::thread::spawn(move || hold(&lock, &Progress::default()).map(drop))
        };
        std::thread::sleep(Duration::from_millis(300));
        drop(first);
        next.join().unwrap().unwrap();
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
            &agent(true, QUICK),
            "https://127.0.0.1:1/model.onnx",
            1,
            "00",
            &dir.path().join("model.onnx"),
            &Progress::default(),
            QUICK,
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
