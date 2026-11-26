//! A Unix socket in a 0700 folder the user owns: only this user (and root)
//! can reach into the folder to connect. mdoc creates the folder 0700 and
//! refuses to listen unless it is a real folder, this user's, with no
//! group or other bits; the shim checks the same before it connects. The
//! socket itself is 0600. Since an ACL could still open the folder to
//! someone else, each end also checks that the other runs as this user
//! (`getpeereid`, `SO_PEERCRED`). A lock file held while listening keeps
//! two mdocs of one profile from replacing each other's socket; the
//! socket goes when the listener stops.
//!
//! A socket's path has to fit `sun_path`, about 104 bytes on macOS and 108
//! on Linux. A data folder too deep for the usual path gets a short folder
//! of its own instead (#1311), made and checked the same way.
#![allow(unsafe_code)] // geteuid, flock, the peer's credentials (#897), confstr.

use super::{CallError, Calls, Handler};
use std::{
    fs::{self, File},
    io::{self, ErrorKind},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::Arc,
};

/// The socket's path.
pub type Address = PathBuf;

/// `<data>/mdoc/connector/mcp.sock`, under the folder the Vault lives in:
/// the dev profile's `data`, or the system's local data folder. When that
/// path is too long for a socket, `<base>/mdoc-<name>/mcp.sock` instead
/// ([`short_base`], [`short_name`]; #1311). mdoc and the shim each work it
/// out by this one rule, so they agree.
pub fn address(dev_profile: Option<&Path>) -> Option<Address> {
    let data = match dev_profile {
        Some(profile) => profile.join("data"),
        None => dirs::data_local_dir()?,
    };
    Some(address_in(&data, short_base))
}

/// The address of the profile whose data folder is `data`, the short
/// folder going under `base` when it is needed.
fn address_in(data: &Path, base: impl FnOnce() -> PathBuf) -> Address {
    let usual = data.join("mdoc").join("connector").join("mcp.sock");
    if fits(&usual) {
        return usual;
    }
    base()
        .join(format!("mdoc-{}", short_name(data)))
        .join("mcp.sock")
}

/// The short folder's name for `data`: the first 16 hex digits of the
/// SHA-256 of its path, as `printf %s "$data" | shasum -a 256` prints it.
fn short_name(data: &Path) -> String {
    use sha2::{Digest, Sha256};
    use std::os::unix::ffi::OsStrExt;
    let digest = Sha256::digest(data.as_os_str().as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Where short folders go: on macOS the user's own cache folder
/// (`/var/folders/…/C`, which, unlike `$TMPDIR`, the system doesn't clear
/// of files left unused), read from the system rather than the
/// environment, which the AI app may change for the shim. Otherwise, or
/// failing that, `$XDG_RUNTIME_DIR`, else `/tmp`.
fn short_base() -> PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(cache) = darwin_user_cache_dir() {
        return cache;
    }
    match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        Some(runtime) if runtime.is_absolute() => runtime,
        _ => PathBuf::from("/tmp"),
    }
}

/// `confstr(_CS_DARWIN_USER_CACHE_DIR)`: the user's cache folder under
/// `/var/folders`, made by the system and the user's alone.
#[cfg(target_os = "macos")]
fn darwin_user_cache_dir() -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let mut buf = vec![0u8; libc::PATH_MAX as usize];
    // SAFETY: `buf` is valid for `buf.len()` bytes for confstr to fill.
    let len = unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_CACHE_DIR,
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };
    // `len` counts the closing NUL; 0 is a failure, more than the buffer
    // a path cut short.
    if len == 0 || len > buf.len() {
        return None;
    }
    buf.truncate(len - 1);
    let dir = PathBuf::from(std::ffi::OsString::from_vec(buf));
    dir.is_absolute().then_some(dir)
}

pub fn show(address: &Address, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", address.display())
}

/// This process's effective user.
pub(crate) fn euid() -> libc::uid_t {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

/// Whether `uid` is this process's user.
pub(super) fn is_us(uid: libc::uid_t) -> bool {
    uid == euid()
}

/// The user at the other end of `stream`.
pub(super) fn peer(stream: &UnixStream) -> io::Result<libc::uid_t> {
    let fd = stream.as_raw_fd();
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let mut cred = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut len = size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: `cred` and `len` are valid for the call to fill.
        let read = unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&raw mut cred).cast(),
                &mut len,
            )
        };
        if read != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(cred.uid)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let (mut uid, mut gid) = (0, 0);
        // SAFETY: `uid` and `gid` are valid for the call to fill.
        if unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(uid)
    }
}

/// The socket's folder: a real folder, this user's, with no group or
/// other bits.
pub(super) fn check_folder(folder: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(folder)?;
    if !meta.file_type().is_dir() || !is_us(meta.uid()) || meta.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            format!("{} is not this user's alone", folder.display()),
        ));
    }
    Ok(())
}

fn folder(socket: &Path) -> io::Result<&Path> {
    socket
        .parent()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "no folder"))
}

/// The profile's listener lock, taken: `None` when another mdoc holds it.
pub(super) fn lock(folder: &Path) -> io::Result<Option<File>> {
    let file = (File::options().create(true).truncate(false).write(true))
        .mode(0o600)
        .open(folder.join("mcp.lock"))?;
    // SAFETY: a valid descriptor; flock only locks it.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let error = io::Error::last_os_error();
        return match error.kind() {
            ErrorKind::WouldBlock => Ok(None),
            _ => Err(error),
        };
    }
    Ok(Some(file))
}

/// A served connection.
pub type Stream = UnixStream;

/// A stop shuts the connection down both ways: the caller sees its end
/// at once, and a blocking read or write on it ends, now and later.
pub fn close(stream: &Stream) {
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// Nothing more to cut: [`close`] ended the call's I/O for good.
pub struct Cut;

impl Cut {
    pub fn cut(&self) {}
}

pub struct Listener {
    socket: PathBuf,
    calls: Arc<Calls>,
    /// Held while listening; released once the socket is gone.
    _lock: File,
}

/// Whether the system takes `socket` as a socket's path: `sun_path`
/// holds it with its closing NUL (#1311).
fn fits(socket: &Path) -> bool {
    let room = size_of::<libc::sockaddr_un>() - std::mem::offset_of!(libc::sockaddr_un, sun_path);
    socket.as_os_str().len() < room
}

pub fn listen(socket: &Address, answer: Handler) -> io::Result<Listener> {
    if !fits(socket) {
        // Told apart, so Settings can say so plainly (#1311).
        return Err(io::Error::new(
            ErrorKind::InvalidFilename,
            format!("{} is too long for a socket's path", socket.display()),
        ));
    }
    let folder = folder(socket)?;
    if let Some(parent) = folder.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::DirBuilder::new().mode(0o700).create(folder) {
        Err(error) if error.kind() != ErrorKind::AlreadyExists => return Err(error),
        _ => {}
    }
    check_folder(folder)?;
    let Some(held) = lock(folder)? else {
        return Err(io::Error::new(
            ErrorKind::AddrInUse,
            "another mdoc listens there",
        ));
    };
    // Whoever listens holds the lock: a socket left now is from a crash.
    if fs::symlink_metadata(socket).is_ok() {
        fs::remove_file(socket)?;
    }
    let listener = UnixListener::bind(socket)?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
    let calls = Calls::new();
    let handle = Listener {
        socket: socket.clone(),
        calls: calls.clone(),
        _lock: held,
    };
    std::thread::Builder::new()
        .name("mdoc-connector-listener".into())
        .spawn(move || {
            for stream in listener.incoming() {
                if calls.stopped() {
                    return;
                }
                let Ok(stream) = stream else { continue };
                if !peer(&stream).is_ok_and(is_us) {
                    continue;
                }
                let (answer, calls) = (answer.clone(), calls.clone());
                let _ = std::thread::Builder::new()
                    .name("mdoc-connector-call".into())
                    .spawn(move || super::serve(stream, Cut, &calls, &answer));
            }
        })?;
    Ok(handle)
}

/// Stop: no answer written from now on, one connection to wake the
/// accept, the socket gone, then the lock.
pub fn stop(listener: Listener) {
    listener.calls.stop();
    let _ = UnixStream::connect(&listener.socket);
    let _ = fs::remove_file(&listener.socket);
}

pub fn connect(socket: &Address) -> Result<UnixStream, CallError> {
    match check_folder(folder(socket)?) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Err(CallError::NotListening),
        Err(_) => return Err(CallError::NotOurs),
        Ok(()) => {}
    }
    let stream = UnixStream::connect(socket).map_err(|error| match error.kind() {
        ErrorKind::NotFound | ErrorKind::ConnectionRefused => CallError::NotListening,
        _ => CallError::Io(error),
    })?;
    if !peer(&stream).is_ok_and(is_us) {
        return Err(CallError::NotOurs);
    }
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handler() -> Handler {
        Arc::new(|request: &[u8]| request.to_vec())
    }

    /// Done when: mdoc makes the socket's folder 0700 and the socket 0600.
    #[test]
    fn the_folder_is_the_users_alone() {
        let profile = tempfile::tempdir().unwrap();
        let socket = address(Some(profile.path())).unwrap();
        let _listening = listen(&socket, handler()).unwrap();
        let folder = fs::metadata(socket.parent().unwrap()).unwrap();
        assert_eq!(folder.mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(&socket).unwrap().mode() & 0o777, 0o600);
    }

    /// Done when: a folder others can enter, or a symlink, is refused by
    /// the listener and the shim alike.
    #[test]
    fn a_folder_others_can_reach_is_refused() {
        let profile = tempfile::tempdir().unwrap();
        let socket = address(Some(profile.path())).unwrap();
        let folder = socket.parent().unwrap();
        let listening = listen(&socket, handler()).unwrap();
        fs::set_permissions(folder, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(connect(&socket), Err(CallError::NotOurs)));
        drop(listening);
        assert!(listen(&socket, handler()).is_err());

        let elsewhere = tempfile::tempdir().unwrap();
        fs::set_permissions(elsewhere.path(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::remove_dir_all(folder).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), folder).unwrap();
        assert!(listen(&socket, handler()).is_err());
        assert!(matches!(connect(&socket), Err(CallError::NotOurs)));
    }

    /// Done when: the peer check takes this user and no other, and reads
    /// this user at the other end of a pair.
    #[test]
    fn a_peer_of_another_user_is_refused() {
        assert!(is_us(euid()));
        assert!(!is_us(euid().wrapping_add(1)));
        let (one, _two) = UnixStream::pair().unwrap();
        assert_eq!(peer(&one).unwrap(), euid());
    }

    /// Done when: while another mdoc of the profile holds the lock, a
    /// listener refuses and leaves that mdoc's socket alone.
    #[test]
    fn a_held_lock_keeps_the_socket() {
        let profile = tempfile::tempdir().unwrap();
        let socket = address(Some(profile.path())).unwrap();
        let folder = socket.parent().unwrap();
        fs::create_dir_all(folder).unwrap();
        fs::set_permissions(folder, fs::Permissions::from_mode(0o700)).unwrap();
        let _other = (lock(folder).unwrap()).expect("the lock is free");
        let _theirs = UnixListener::bind(&socket).unwrap();
        assert!(listen(&socket, handler()).is_err());
        assert!(socket.exists(), "the other mdoc's socket was removed");
    }

    /// A data folder whose usual socket path is too long: over 110 bytes.
    fn long_data(profile: &Path) -> PathBuf {
        let data = profile.join("p".repeat(110)).join("data");
        assert!(data.as_os_str().len() > 110);
        data
    }

    /// Done when (#1311): a data folder too deep for the usual socket
    /// path listens in its short folder, and a call is answered there.
    #[test]
    fn a_long_data_folder_listens_in_its_short_folder() {
        let profile = tempfile::tempdir().unwrap();
        let data = long_data(profile.path());
        let socket = address_in(&data, short_base);
        assert!(socket.starts_with(short_base()), "{}", socket.display());
        assert!(fits(&socket), "{}", socket.display());
        let folder = socket.parent().unwrap().to_path_buf();
        let listening = listen(&socket, handler()).unwrap();
        assert_eq!(fs::metadata(&folder).unwrap().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(&socket).unwrap().mode() & 0o777, 0o600);
        let endpoint = super::super::Endpoint(socket);
        assert_eq!(super::super::call(&endpoint, b"hi").unwrap(), b"hi");
        drop(listening);
        let _ = fs::remove_dir_all(folder);
    }

    /// #1311: a data folder that fits keeps the usual path, and the short
    /// folder's name is the SHA-256 a script works out.
    #[test]
    fn a_short_data_folder_keeps_the_usual_path() {
        let profile = tempfile::tempdir().unwrap();
        let data = profile.path().join("data");
        assert_eq!(
            address(Some(profile.path())).unwrap(),
            data.join("mdoc").join("connector").join("mcp.sock")
        );
        // printf %s abc | shasum -a 256
        assert_eq!(short_name(Path::new("abc")), "ba7816bf8f01cfea");
        let (one, two) = (
            long_data(profile.path()),
            profile.path().join("q".repeat(120)),
        );
        let base = || PathBuf::from("/b");
        assert_ne!(address_in(&one, base), address_in(&two, base));
    }

    /// #1311: when even the short folder's path is too long, the listener
    /// refuses before making anything, told apart from other failures and
    /// without std's "SUN_LEN".
    #[test]
    fn a_base_too_long_is_refused_plainly() {
        let profile = tempfile::tempdir().unwrap();
        let base = profile.path().join("b".repeat(120));
        let socket = address_in(&long_data(profile.path()), || base.clone());
        assert!(socket.starts_with(&base));
        let refused = listen(&socket, handler()).err().unwrap();
        assert_eq!(refused.kind(), ErrorKind::InvalidFilename, "{refused}");
        assert!(!refused.to_string().contains("SUN_LEN"), "{refused}");
        assert!(!base.exists(), "made a folder it can't use");
    }

    /// #1311: a short folder made beforehand that others can enter, or a
    /// symlink in its place, is refused by the listener and the shim.
    #[test]
    fn a_short_folder_others_can_reach_is_refused() {
        let profile = tempfile::tempdir().unwrap();
        let base = tempfile::tempdir().unwrap();
        let socket = address_in(&long_data(profile.path()), || base.path().to_path_buf());
        let folder = socket.parent().unwrap();
        fs::DirBuilder::new().mode(0o755).create(folder).unwrap();
        fs::set_permissions(folder, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(listen(&socket, handler()).is_err());
        assert!(matches!(connect(&socket), Err(CallError::NotOurs)));

        let elsewhere = tempfile::tempdir().unwrap();
        fs::set_permissions(elsewhere.path(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::remove_dir(folder).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), folder).unwrap();
        assert!(listen(&socket, handler()).is_err());
        assert!(matches!(connect(&socket), Err(CallError::NotOurs)));
    }

    /// Done when: a socket left by a crash is replaced.
    #[test]
    fn a_dead_socket_is_replaced() {
        let profile = tempfile::tempdir().unwrap();
        let socket = address(Some(profile.path())).unwrap();
        let folder = socket.parent().unwrap();
        fs::create_dir_all(folder).unwrap();
        fs::set_permissions(folder, fs::Permissions::from_mode(0o700)).unwrap();
        drop(UnixListener::bind(&socket).unwrap());
        assert!(socket.exists());
        let _listening = listen(&socket, handler()).unwrap();
        assert_eq!(
            super::super::call(&super::super::Endpoint(socket), b"hi").unwrap(),
            b"hi"
        );
    }
}
