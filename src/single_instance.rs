//! One mdoc per user session on Windows. macOS gets this from Launch
//! Services; Windows starts a new process for every launch and "Open with".
//!
//! The first process owns a named pipe. A later launch sends its paths
//! through it and exits; the first process opens them and comes to the front.
//! If the pipe cannot be reached, the later launch simply runs on its own.
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

/// What `claim` decided for this process.
pub enum Launch {
    /// Another mdoc took the paths; exit now.
    Forwarded,
    /// This is the first mdoc: paths from later launches arrive here. An empty
    /// list asks only to bring the window to the front.
    Primary(Option<Receiver<Vec<PathBuf>>>),
}

#[cfg(windows)]
pub fn claim(paths: &[PathBuf]) -> Launch {
    windows::claim(&windows::pipe_name(), paths)
}

#[cfg(not(windows))]
pub fn claim(_: &[PathBuf]) -> Launch {
    Launch::Primary(None)
}

#[cfg(windows)]
mod windows {
    use super::Launch;
    use std::ffi::{OsStr, OsString};
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{
        ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
    };
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

    /// Paths beyond this are dropped; a launch never sends anywhere near it.
    const MAX_MESSAGE: u64 = 1 << 20;
    const ERROR_FILE_NOT_FOUND: i32 = 2;

    /// Pipe names are machine-wide: the user and logon session keep two
    /// signed-in users (or two sessions) from reaching each other's mdoc.
    pub fn pipe_name() -> String {
        let mut session = 0;
        unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };
        let user: String = std::env::var("USERNAME")
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
            .collect();
        format!(r"\\.\pipe\io.github.lawkitt.mdoc-{user}-{session}")
    }

    pub fn claim(name: &str, paths: &[PathBuf]) -> Launch {
        // Two tries: a launch racing another one may find the pipe gone
        // between "nobody serves it" and "someone just created it".
        for _ in 0..2 {
            match send(name, paths) {
                Ok(()) => return Launch::Forwarded,
                Err(error) if error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND) => {}
                // The owner exists but cannot be reached: run on our own.
                Err(_) => return Launch::Primary(None),
            }
            if let Some(pipe) = create(name, true) {
                return Launch::Primary(Some(serve(name.to_string(), pipe)));
            }
        }
        Launch::Primary(None)
    }

    /// Hand `paths` to the running mdoc.
    fn send(name: &str, paths: &[PathBuf]) -> std::io::Result<()> {
        let mut pipe = loop {
            match std::fs::OpenOptions::new().write(true).open(name) {
                Ok(pipe) => break pipe,
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    let wide = wide(OsStr::new(name));
                    if unsafe { WaitNamedPipeW(wide.as_ptr(), 2000) } == 0 {
                        return Err(error);
                    }
                }
                Err(error) => return Err(error),
            }
        };
        // Let the running mdoc take the foreground this launch was given.
        unsafe { AllowSetForegroundWindow(ASFW_ANY) };
        pipe.write_all(&encode(paths))?;
        pipe.flush()
    }

    fn create(name: &str, first: bool) -> Option<HANDLE> {
        let wide = wide(OsStr::new(name));
        let flags = PIPE_ACCESS_INBOUND
            | if first {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                0
            };
        let pipe = unsafe {
            CreateNamedPipeW(
                wide.as_ptr(),
                flags,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                0,
                64 * 1024,
                0,
                std::ptr::null(),
            )
        };
        (pipe != INVALID_HANDLE_VALUE).then_some(pipe)
    }

    /// Accept launches for the life of the process.
    fn serve(name: String, first: HANDLE) -> mpsc::Receiver<Vec<PathBuf>> {
        let (tx, rx) = mpsc::channel();
        let pipe = first as usize;
        std::thread::Builder::new()
            .name("mdoc-single-instance".into())
            .spawn(move || {
                let mut pipe = pipe as HANDLE;
                loop {
                    // A launch that connected, wrote and closed before this
                    // call reports NO_DATA; what it wrote is still readable.
                    let connected = unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) } != 0
                        || matches!(
                            unsafe { GetLastError() },
                            ERROR_PIPE_CONNECTED | ERROR_NO_DATA
                        );
                    // The next instance exists before this one closes, so the
                    // name never disappears and no launch misses the owner.
                    let next = create(&name, false);
                    let mut client = unsafe { File::from_raw_handle(pipe as RawHandle) };
                    let mut bytes = Vec::new();
                    if connected
                        && (&mut client)
                            .take(MAX_MESSAGE)
                            .read_to_end(&mut bytes)
                            .is_ok()
                        && tx.send(decode(&bytes)).is_err()
                    {
                        return;
                    }
                    drop(client);
                    match next {
                        Some(next) => pipe = next,
                        None => return,
                    }
                    // A failing pipe must not spin.
                    if !connected {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            })
            .expect("could not start the single-instance listener");
        rx
    }

    fn wide(text: &OsStr) -> Vec<u16> {
        text.encode_wide().chain([0]).collect()
    }

    /// UTF-16 paths, each ending in a NUL, little-endian: lossless for any
    /// Windows path. Relative paths are resolved here, against this launch's
    /// working directory.
    fn encode(paths: &[PathBuf]) -> Vec<u8> {
        paths
            .iter()
            .map(|path| std::path::absolute(path).unwrap_or_else(|_| path.clone()))
            .flat_map(|path| wide(path.as_os_str()))
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    fn decode(bytes: &[u8]) -> Vec<PathBuf> {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        units
            .split(|&unit| unit == 0)
            .filter(|path| !path.is_empty())
            .map(|path| PathBuf::from(OsString::from_wide(path)))
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn paths_round_trip_with_cyrillic_and_spaces() {
            let paths = vec![
                PathBuf::from(r"C:\Дела\Мировое соглашение.docx"),
                PathBuf::from(r"C:\a b\c.md"),
            ];
            assert_eq!(decode(&encode(&paths)), paths);
            assert!(decode(&[]).is_empty());
        }

        #[test]
        fn a_second_launch_forwards_its_paths_to_the_first() {
            let name = format!(r"\\.\pipe\mdoc-test-{}", std::process::id());
            let Launch::Primary(Some(rx)) = claim(&name, &[]) else {
                panic!("the first launch owns the pipe");
            };
            for round in 0..3 {
                let path = PathBuf::from(format!(r"C:\cases\{round}.pdf"));
                assert!(matches!(
                    claim(&name, std::slice::from_ref(&path)),
                    Launch::Forwarded
                ));
                let got = rx.recv_timeout(Duration::from_secs(5)).unwrap();
                assert_eq!(got, vec![path]);
            }
            // A bare launch only asks for the window; quick ones in a row
            // close their end before the listener reaches them.
            for _ in 0..20 {
                assert!(matches!(claim(&name, &[]), Launch::Forwarded));
            }
            for _ in 0..20 {
                assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap().is_empty());
            }
        }
    }
}
