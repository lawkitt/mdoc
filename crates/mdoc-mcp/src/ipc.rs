//! The local channel between the shim and mdoc (ADR 0126): an owner-only
//! named pipe on Windows (`windows.rs`), a Unix socket in a 0700 folder on
//! macOS and Linux (`unix.rs`). One connection per call: one request line,
//! one answer line. mdoc listens only while "AI connector (preview)" is on.
use std::{
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as sys;
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

/// A listener; dropping it stops it.
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

/// Serve one connection: one request line, one answer line.
fn serve(stream: impl Read + Write, answer: &Handler) {
    let mut reader = io::BufReader::new(stream);
    let Ok(request) = read_line(&mut reader) else {
        return;
    };
    let _ = write_line(reader.get_mut(), &answer(&request));
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
    use std::sync::Arc;

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

    #[test]
    fn a_second_listener_on_the_same_endpoint_is_refused() {
        let profile = scratch();
        let endpoint = Endpoint::for_profile(Some(profile.path())).unwrap();
        let echo: Handler = Arc::new(|request: &[u8]| request.to_vec());
        let _first = listen(&endpoint, echo.clone()).unwrap();
        assert!(listen(&endpoint, echo).is_err());
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
