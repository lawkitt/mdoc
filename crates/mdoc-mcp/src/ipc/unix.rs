//! A Unix socket in a 0700 folder the user owns: only this user (and root)
//! can reach into the folder to connect. mdoc creates the folder 0700 and
//! refuses to listen unless it is a real folder, this user's, with no
//! group or other bits; the shim checks the same before it connects. The
//! socket itself is 0600. Since an ACL could still open the folder to
//! someone else, each end also checks that the other runs as this user
//! (`getpeereid`, `SO_PEERCRED`). A lock file held while listening keeps
//! two mdocs of one profile from replacing each other's socket; the
//! socket goes when the listener stops.
#![allow(unsafe_code)] // geteuid, flock and the peer's credentials (#897).

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
/// the dev profile's `data`, or the system's local data folder.
pub fn address(dev_profile: Option<&Path>) -> Option<Address> {
    let data = match dev_profile {
        Some(profile) => profile.join("data"),
        None => dirs::data_local_dir()?,
    };
    Some(data.join("mdoc").join("connector").join("mcp.sock"))
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

/// How a stop cuts a call's socket I/O short: the connection, shut down
/// both ways, which ends a blocking read or write.
pub struct Cut(UnixStream);

impl Cut {
    fn of(stream: &UnixStream) -> io::Result<Self> {
        stream.try_clone().map(Self)
    }

    pub fn cut(&self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}

pub struct Listener {
    socket: PathBuf,
    calls: Arc<Calls>,
    /// Held while listening; released once the socket is gone.
    _lock: File,
}

pub fn listen(socket: &Address, answer: Handler) -> io::Result<Listener> {
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
                // No way to cut it short: not served.
                let Ok(cut) = Cut::of(&stream) else { continue };
                let (answer, calls) = (answer.clone(), calls.clone());
                let _ = std::thread::Builder::new()
                    .name("mdoc-connector-call".into())
                    .spawn(move || super::serve(stream, cut, &calls, &answer));
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
