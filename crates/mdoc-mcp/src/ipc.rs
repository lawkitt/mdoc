//! The local channel between the shim and mdoc (ADR 0126): an owner-only
//! named pipe on Windows (`windows.rs`), a Unix socket in a 0700 folder on
//! macOS and Linux (`unix.rs`). One connection per call: one request line,
//! one answer line. mdoc listens only while "AI connector (preview)" is on.
use std::{
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError, RwLock, TryLockError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as sys;
#[cfg(unix)]
pub(crate) use unix::euid;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as sys;

/// mdoc-store's `app_paths::DEV_PROFILE_VAR`, which the app's tests pin to
/// this: a debug build's profile folder.
pub const DEV_PROFILE_VAR: &str = "MDOC_DEV_PROFILE";

/// mdoc-store's `app_paths::DEV_PROFILE_HONOURED`, pinned the same way.
pub const DEV_PROFILE_HONOURED: bool = cfg!(all(
    debug_assertions,
    any(target_os = "macos", target_os = "windows")
));

/// The dev profile this process names, read as mdoc reads it: debug
/// builds on macOS and Windows, a relative folder taken from the working
/// directory.
pub fn dev_profile_from_env() -> Option<PathBuf> {
    if !DEV_PROFILE_HONOURED {
        return None;
    }
    let value = std::env::var_os(DEV_PROFILE_VAR).filter(|value| !value.is_empty())?;
    Some(std::env::current_dir().unwrap_or_default().join(value))
}

/// Where one profile's mdoc listens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint(sys::Address);

impl Endpoint {
    /// The endpoint of the profile in `dev_profile`, or of the user's own
    /// profile; `None` when the system names no data folder.
    pub fn for_profile(dev_profile: Option<&Path>) -> Option<Self> {
        sys::address(dev_profile).map(Self)
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        sys::show(&self.0, f)
    }
}

/// Why a call reached no mdoc.
#[derive(Debug)]
pub enum CallError {
    /// Nothing listens there: mdoc isn't running, its connector is off,
    /// or its Vault isn't open.
    NotListening,
    /// Something listens that isn't this user's: the shim says nothing to it.
    NotOurs,
    Io(io::Error),
}

impl From<io::Error> for CallError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Send one request line to the mdoc at `endpoint`: its answer line.
pub fn call(endpoint: &Endpoint, request: &[u8]) -> Result<Vec<u8>, CallError> {
    let mut stream = sys::connect(&endpoint.0)?;
    write_line(&mut stream, request)?;
    Ok(read_line(&mut io::BufReader::new(stream))?)
}

/// Answers each request line with `answer`'s line.
pub type Handler = std::sync::Arc<dyn Fn(&[u8]) -> Vec<u8> + Send + Sync>;

/// A listener; dropping it stops it. Once the drop returns, nothing more
/// is written to any connection it accepted (#1220): a call still being
/// answered then gets no answer. The drop never waits for an answer
/// being made, only for one being written, which it cuts short.
pub struct Listening(Option<sys::Listener>);

impl Drop for Listening {
    fn drop(&mut self) {
        if let Some(listener) = self.0.take() {
            sys::stop(listener);
        }
    }
}

/// Listen at `endpoint`, owner-only, each connection on its own thread.
/// Fails when another process holds the endpoint or its folder isn't
/// this user's alone.
pub fn listen(endpoint: &Endpoint, answer: Handler) -> io::Result<Listening> {
    sys::listen(&endpoint.0, answer).map(|listener| Listening(Some(listener)))
}

/// What a listener shares with the calls it serves (#1220): whether it
/// has stopped, the gate each answer is written under, and how to cut
/// short the pipe or socket I/O under way.
struct Calls {
    stopped: AtomicBool,
    /// Held shared while an answer is written; [`Calls::stop`] takes it
    /// alone once, so no write outlasts the stop.
    gate: RwLock<()>,
    /// The I/O under way, by call: never a handler's own (Vault) I/O.
    cuts: Mutex<Vec<(u64, Arc<sys::Cut>)>>,
    next: AtomicU64,
}

impl Calls {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            stopped: AtomicBool::new(false),
            gate: RwLock::new(()),
            cuts: Mutex::new(Vec::new()),
            next: AtomicU64::new(0),
        })
    }

    fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Run `io` on the pipe or socket, where a stop can cut it.
    fn cuttable<T>(&self, cut: &Arc<sys::Cut>, io: impl FnOnce() -> T) -> T {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        lock(&self.cuts).push((id, cut.clone()));
        let done = io();
        // Under the lock a stop cuts with: once off the list, this call's
        // thread is never cut again (its handler's I/O is the Vault's).
        lock(&self.cuts).retain(|(at, _)| *at != id);
        done
    }

    /// No answer is written from now on; one being written is cut and
    /// waited for. An answer still being made is not waited for: it finds
    /// the stop when it is done, and writes nothing.
    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        loop {
            for (_, cut) in lock(&self.cuts).iter() {
                cut.cut();
            }
            match self.gate.try_write() {
                Ok(_) | Err(TryLockError::Poisoned(_)) => return,
                // A write began after the cut: cut again.
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Serve one connection: one request line, one answer line, written only
/// while the listener runs.
fn serve(stream: impl Read + Write, cut: sys::Cut, calls: &Calls, answer: &Handler) {
    let cut = Arc::new(cut);
    let mut reader = io::BufReader::new(stream);
    let Ok(request) = calls.cuttable(&cut, || read_line(&mut reader)) else {
        return;
    };
    if calls.stopped() {
        return;
    }
    let answer = answer(&request);
    let _open = calls.gate.read().unwrap_or_else(PoisonError::into_inner);
    if calls.stopped() {
        return;
    }
    let _ = calls.cuttable(&cut, || write_line(reader.get_mut(), &answer));
}

/// One line, at most [`crate::wire::MAX_LINE`] bytes, without its line
/// end; `None` at the end of the stream.
pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let read = (reader.by_ref())
        .take(crate::wire::MAX_LINE + 1)
        .read_until(b'\n', &mut line)?;
    if read == 0 {
        return Ok(None);
    }
    if line.last() == Some(&b'\n') {
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
    } else if line.len() as u64 > crate::wire::MAX_LINE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
    }
    Ok(Some(line))
}

fn read_line(reader: &mut impl BufRead) -> io::Result<Vec<u8>> {
    Ok(read_message(reader)?.unwrap_or_default())
}

fn write_line(writer: &mut impl Write, line: &[u8]) -> io::Result<()> {
    let mut bytes = Vec::with_capacity(line.len() + 1);
    bytes.extend_from_slice(line);
    bytes.push(b'\n');
    writer.write_all(&bytes)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_call_gets_its_answer_and_stops_with_the_listener() {
        let profile = scratch();
        let endpoint = Endpoint::for_profile(Some(profile.path())).unwrap();
        assert!(matches!(
            call(&endpoint, b"before"),
            Err(CallError::NotListening)
        ));
        let echo: Handler = Arc::new(|request: &[u8]| [b"re: ", request].concat());
        let listening = listen(&endpoint, echo).unwrap();
        for round in 0..5 {
            let request = format!("call {round}");
            let answer = call(&endpoint, request.as_bytes()).unwrap();
            assert_eq!(answer, format!("re: call {round}").into_bytes());
        }
        drop(listening);
        let mut stopped = false;
        for _ in 0..50 {
            if matches!(call(&endpoint, b"after"), Err(CallError::NotListening)) {
                stopped = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            stopped,
            "the endpoint still answers after the listener stopped"
        );
    }

    /// A handler held at a hook until released, telling when it is
    /// entered; it answers `answer`.
    fn held(answer: Vec<u8>) -> (Handler, mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (enter, entered) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        let released = Mutex::new(released);
        let handler: Handler = Arc::new(move |_: &[u8]| {
            let _ = enter.send(());
            let _ = lock(&released).recv();
            answer.clone()
        });
        (handler, entered, release)
    }

    const SOON: Duration = Duration::from_secs(2);

    /// #1220: a call whose answer is still being made when the listener
    /// stops gets nothing written, and the stop doesn't wait for it.
    #[test]
    fn an_answer_made_across_the_stop_is_not_written() {
        let profile = scratch();
        let endpoint = Endpoint::for_profile(Some(profile.path())).unwrap();
        let (handler, entered, release) = held(b"the text".to_vec());
        let listening = listen(&endpoint, handler).unwrap();
        let caller = {
            let endpoint = endpoint.clone();
            std::thread::spawn(move || call(&endpoint, b"read"))
        };
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        let stopping = std::time::Instant::now();
        drop(listening);
        assert!(stopping.elapsed() < SOON, "the stop waited for the answer");
        release.send(()).unwrap();
        // The call ends with nothing: an empty line or a broken pipe.
        if let Ok(answer) = caller.join().unwrap() {
            assert!(answer.is_empty(), "written after the stop: {answer:?}");
        }
    }

    /// #1220: an answer being written to a caller that doesn't read is cut
    /// short by the stop, which returns rather than waiting on it.
    #[test]
    fn a_stuck_write_is_cut_by_the_stop() {
        let profile = scratch();
        let endpoint = Endpoint::for_profile(Some(profile.path())).unwrap();
        let big = vec![b'x'; 8 << 20];
        let (handler, entered, release) = held(big);
        let listening = listen(&endpoint, handler).unwrap();
        let mut stream = sys::connect(&endpoint.0).unwrap();
        write_line(&mut stream, b"read").unwrap();
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        release.send(()).unwrap();
        // Long enough for the write to fill the pipe and block.
        std::thread::sleep(Duration::from_millis(300));
        let stopping = std::time::Instant::now();
        drop(listening);
        assert!(stopping.elapsed() < SOON, "the stop waited on the write");
        drop(stream);
    }

    #[test]
    fn a_second_listener_on_the_same_endpoint_is_refused() {
        let profile = scratch();
        let endpoint = Endpoint::for_profile(Some(profile.path())).unwrap();
        let echo: Handler = Arc::new(|request: &[u8]| request.to_vec());
        let _first = listen(&endpoint, echo.clone()).unwrap();
        // Told apart, so Settings can say another mdoc holds it (#1220).
        let refused = listen(&endpoint, echo).err().unwrap();
        assert_eq!(refused.kind(), io::ErrorKind::AddrInUse, "{refused}");
        assert_eq!(
            call(&endpoint, b"still the first").unwrap(),
            b"still the first"
        );
    }

    #[test]
    fn profiles_listen_apart() {
        let (one, two) = (scratch(), scratch());
        let one = Endpoint::for_profile(Some(one.path())).unwrap();
        let two = Endpoint::for_profile(Some(two.path())).unwrap();
        assert_ne!(one, two);
        assert_ne!(Endpoint::for_profile(None), Some(one));
    }

    #[test]
    fn a_line_over_the_limit_is_refused() {
        let long = vec![b'x'; crate::wire::MAX_LINE as usize + 1];
        assert!(read_line(&mut io::Cursor::new(long)).is_err());
        let line = read_line(&mut io::Cursor::new(b"one\ntwo".to_vec())).unwrap();
        assert_eq!(line, b"one");
    }
}
