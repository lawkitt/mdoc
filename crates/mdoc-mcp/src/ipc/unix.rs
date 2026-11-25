//! A Unix socket in a 0700 folder the user owns: only this user (and root)
//! can reach into the folder to connect. mdoc creates the folder 0700 and
//! refuses to listen unless it is a real folder, this user's, with no
//! group or other bits; the shim checks the same before it connects. The
//! socket itself is 0600, and goes when the listener stops.
use super::{CallError, Handler};
use std::{
    fs,
    io::{self, ErrorKind},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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

/// The socket's folder: a real folder, this user's, with no group or
/// other bits.
#[allow(unsafe_code)] // geteuid (#897).
pub(super) fn check_folder(folder: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(folder)?;
    // SAFETY: geteuid has no preconditions and cannot fail.
    let us = unsafe { libc::geteuid() };
    if !meta.file_type().is_dir() || meta.uid() != us || meta.mode() & 0o077 != 0 {
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

pub struct Listener {
    socket: PathBuf,
    stop: Arc<AtomicBool>,
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
    if fs::symlink_metadata(socket).is_ok() {
        // A live socket is another mdoc's; a dead one is left from a crash.
        if UnixStream::connect(socket).is_ok() {
            return Err(io::Error::new(
                ErrorKind::AddrInUse,
                "another mdoc listens there",
            ));
        }
        fs::remove_file(socket)?;
    }
    let listener = UnixListener::bind(socket)?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
    let stop = Arc::new(AtomicBool::new(false));
    let handle = Listener {
        socket: socket.clone(),
        stop: stop.clone(),
    };
    std::thread::Builder::new()
        .name("mdoc-connector-listener".into())
        .spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(stream) = stream else { continue };
                let answer = answer.clone();
                let _ = std::thread::Builder::new()
                    .name("mdoc-connector-call".into())
                    .spawn(move || super::serve(stream, &answer));
            }
        })?;
    Ok(handle)
}

/// Stop: the flag, one connection to wake the accept, and the socket gone.
pub fn stop(listener: Listener) {
    listener.stop.store(true, Ordering::SeqCst);
    let _ = UnixStream::connect(&listener.socket);
    let _ = fs::remove_file(&listener.socket);
}

pub fn connect(socket: &Address) -> Result<UnixStream, CallError> {
    match check_folder(folder(socket)?) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Err(CallError::NotListening),
        Err(_) => return Err(CallError::NotOurs),
        Ok(()) => {}
    }
    UnixStream::connect(socket).map_err(|error| match error.kind() {
        ErrorKind::NotFound | ErrorKind::ConnectionRefused => CallError::NotListening,
        _ => CallError::Io(error),
    })
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
