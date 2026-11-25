//! The named pipe, owner-only. Its security descriptor names one ACE, full
//! access for this user's SID, in a protected DACL with this user as owner:
//! no other user, no Administrators or SYSTEM, no AppContainer, and the
//! default mandatory label keeps low-integrity processes from writing.
//! Remote clients are refused, and the first instance must be the name's
//! first, so a process already holding the name makes mdoc not listen. The
//! shim checks that the server's process runs as its user, since a DACL
//! can't stop another user from taking the name first, and lets the server
//! identify it but never impersonate it.
#![allow(unsafe_code)] // The named pipe and token checks through Win32 (#897).

use super::{CallError, Handler};
use std::{
    ffi::OsStr,
    fs::File,
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{FromRawHandle, RawHandle},
    },
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
        GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        },
        EqualSid, GetTokenInformation, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
        TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::{
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
        SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
    },
    System::{
        Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE,
            PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
            WaitNamedPipeW,
        },
        RemoteDesktop::ProcessIdToSessionId,
        Threading::{
            GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

/// The pipe's name.
pub type Address = String;

/// Pipe names are machine-wide: the user and logon session keep two
/// signed-in users (or two sessions) apart, as `single_instance.rs` does,
/// and a dev profile's mdoc listens apart from the user's own.
pub fn address(dev_profile: Option<&Path>) -> Option<Address> {
    let mut session = 0;
    // SAFETY: `session` is a valid u32 for the call to fill.
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };
    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let dev = dev_profile
        .map(|profile| {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::hash::DefaultHasher::new();
            profile.hash(&mut hasher);
            format!("-dev-{:016x}", hasher.finish())
        })
        .unwrap_or_default();
    Some(format!(
        r"\\.\pipe\io.github.lawkitt.mdoc-connector-{user}-{session}{dev}"
    ))
}

pub fn show(address: &Address, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(address)
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain([0]).collect()
}

/// A handle closed on drop.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle was opened here and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// A token's user: its `TOKEN_USER`, whose SID points into the buffer.
struct TokenUserSid(Vec<u64>);

impl TokenUserSid {
    fn of(token: HANDLE) -> io::Result<Self> {
        let mut len = 0;
        // SAFETY: a size query: no buffer, `len` filled.
        unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len) };
        let mut buffer = vec![0u64; (len as usize).div_ceil(8).max(1)];
        // SAFETY: `buffer` holds at least `len` bytes, 8-aligned.
        let ok = unsafe {
            GetTokenInformation(token, TokenUser, buffer.as_mut_ptr().cast(), len, &mut len)
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(buffer))
    }

    fn of_process(process: HANDLE) -> io::Result<Self> {
        let mut token = std::ptr::null_mut();
        // SAFETY: `token` is filled with a handle closed by `Owned`.
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = Owned(token);
        Self::of(token.0)
    }

    fn this_process() -> io::Result<Self> {
        // SAFETY: the pseudo-handle needs no closing.
        Self::of_process(unsafe { GetCurrentProcess() })
    }

    fn sid(&self) -> PSID {
        // SAFETY: the buffer holds a TOKEN_USER written by the system.
        unsafe { (*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    /// Whether `sid`, valid for the call, is this token's user.
    fn is(&self, sid: PSID) -> bool {
        // SAFETY: both SIDs are valid for the call.
        unsafe { EqualSid(self.sid(), sid) != 0 }
    }

    fn string(&self) -> io::Result<String> {
        let mut text = std::ptr::null_mut();
        // SAFETY: the SID is valid; `text` is LocalFree'd below.
        if unsafe { ConvertSidToStringSidW(self.sid(), &mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `text` is a NUL-terminated string the system allocated.
        let len = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
        // SAFETY: `len` units were just read from it.
        let string = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
        // SAFETY: allocated by the call above, freed once.
        unsafe { LocalFree(text.cast()) };
        Ok(string)
    }
}

/// A security descriptor from SDDL, LocalFree'd on drop.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: allocated by ConvertStringSecurityDescriptor…, freed once.
        unsafe { LocalFree(self.0) };
    }
}

/// The SDDL for this user alone: owner, and a protected DACL of one ACE.
pub(super) fn owner_only_sddl() -> io::Result<String> {
    let sid = TokenUserSid::this_process()?.string()?;
    Ok(format!("O:{sid}D:P(A;;GA;;;{sid})"))
}

fn owner_only() -> io::Result<Descriptor> {
    let sddl = wide(&owner_only_sddl()?);
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: `sddl` is NUL-terminated; the descriptor is freed by `Descriptor`.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Descriptor(descriptor))
}

/// One instance of the pipe, owner-only; `first`: the name's first.
fn create(name: &str, first: bool) -> io::Result<HANDLE> {
    let descriptor = owner_only()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let wide = wide(name);
    let flags = PIPE_ACCESS_DUPLEX
        | if first {
            FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            0
        };
    // SAFETY: `wide` and `attributes` (and its descriptor) outlive the call.
    let pipe = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            flags,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            64 * 1024,
            64 * 1024,
            0,
            &attributes,
        )
    };
    if pipe == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(pipe)
}

pub struct Listener {
    name: String,
    stop: Arc<AtomicBool>,
}

pub fn listen(name: &Address, answer: Handler) -> io::Result<Listener> {
    let first = create(name, true)? as usize;
    let stop = Arc::new(AtomicBool::new(false));
    let listener = Listener {
        name: name.clone(),
        stop: stop.clone(),
    };
    let name = name.clone();
    std::thread::Builder::new()
        .name("mdoc-connector-listener".into())
        .spawn(move || {
            let mut pipe = first as HANDLE;
            loop {
                // SAFETY: `pipe` is an instance this listener created and
                // owns; no OVERLAPPED means a blocking connect.
                let accepted = unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) } != 0;
                // A client that connected and closed first is still served.
                let connected = accepted || {
                    // SAFETY: reads this thread's last error.
                    let error = unsafe { GetLastError() };
                    matches!(error, ERROR_PIPE_CONNECTED | ERROR_NO_DATA)
                };
                // SAFETY: owned here; the File closes it once.
                let client = unsafe { File::from_raw_handle(pipe as RawHandle) };
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                // The next instance exists before this one is handed off.
                let next = create(&name, false);
                if connected {
                    let answer = answer.clone();
                    let _ = std::thread::Builder::new()
                        .name("mdoc-connector-call".into())
                        .spawn(move || super::serve(client, &answer));
                } else {
                    drop(client);
                    // A failing pipe must not spin.
                    std::thread::sleep(Duration::from_millis(100));
                }
                match next {
                    Ok(next) => pipe = next,
                    Err(error) => {
                        eprintln!("mdoc: the AI connector stopped listening: {error}");
                        return;
                    }
                }
            }
        })?;
    Ok(listener)
}

/// Stop: the flag, then one connection to wake the blocking connect.
pub fn stop(listener: Listener) {
    listener.stop.store(true, Ordering::SeqCst);
    let _ = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&listener.name);
}

/// Open the pipe, then check that its server runs as this user.
pub fn connect(name: &Address) -> Result<File, CallError> {
    let wide = wide(name);
    let pipe = loop {
        // SAFETY: `wide` is NUL-terminated and outlives the call;
        // SECURITY_IDENTIFICATION lets the server identify the shim but not
        // impersonate it.
        let pipe = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                std::ptr::null_mut(),
            )
        };
        if pipe != INVALID_HANDLE_VALUE {
            break pipe;
        }
        // SAFETY: reads this thread's last error.
        match unsafe { GetLastError() } {
            ERROR_FILE_NOT_FOUND => return Err(CallError::NotListening),
            // SAFETY: `wide` is NUL-terminated and outlives the call.
            ERROR_PIPE_BUSY if unsafe { WaitNamedPipeW(wide.as_ptr(), 5000) } != 0 => {}
            _ => return Err(io::Error::last_os_error().into()),
        }
    };
    // SAFETY: opened above; the File closes it once.
    let file = unsafe { File::from_raw_handle(pipe as RawHandle) };
    let mut server = 0;
    // SAFETY: `pipe` is a connected pipe client handle.
    if unsafe { GetNamedPipeServerProcessId(pipe, &mut server) } == 0 || !runs_as_us(server) {
        return Err(CallError::NotOurs);
    }
    Ok(file)
}

/// Whether process `pid` runs as this process's user; `false` when that
/// can't be read.
pub(super) fn runs_as_us(pid: u32) -> bool {
    // SAFETY: no pointers; a null handle is checked.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let process = Owned(process);
    match (
        TokenUserSid::of_process(process.0),
        TokenUserSid::this_process(),
    ) {
        (Ok(theirs), Ok(ours)) => ours.is(theirs.sid()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::{
        Foundation::ERROR_ACCESS_DENIED,
        Security::{
            ACCESS_ALLOWED_ACE, ACL, Authorization::GetSecurityInfo,
            Authorization::SE_KERNEL_OBJECT, CreateRestrictedToken, DACL_SECURITY_INFORMATION,
            DISABLE_MAX_PRIVILEGE, GetAce, GetSecurityDescriptorControl, ImpersonateLoggedOnUser,
            OWNER_SECURITY_INFORMATION, RevertToSelf, SE_DACL_PROTECTED, SID_AND_ATTRIBUTES,
            TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_IMPERSONATE,
        },
    };

    fn test_name(tag: &str) -> String {
        format!(
            r"\\.\pipe\mdoc-connector-test-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        )
    }

    /// Done when: the pipe's DACL, read back from the pipe, is protected
    /// and holds one ACE allowing this user and no one else, and its owner
    /// is this user.
    #[test]
    fn the_pipe_grants_its_owner_alone() {
        let name = test_name("dacl");
        let pipe = Owned(create(&name, true).unwrap());
        let mut owner = std::ptr::null_mut();
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: out-pointers for the call; the descriptor is freed below.
        let error = unsafe {
            GetSecurityInfo(
                pipe.0,
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        assert_eq!(error, 0);
        let descriptor = Descriptor(descriptor);
        let (mut control, mut revision) = (0, 0);
        // SAFETY: the descriptor is valid until `descriptor` drops.
        let read =
            unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) };
        assert_ne!(read, 0);
        assert_ne!(control & SE_DACL_PROTECTED, 0, "the DACL is not protected");
        let us = TokenUserSid::this_process().unwrap();
        // SAFETY: both SIDs are valid.
        let ours = unsafe { EqualSid(owner, us.sid()) };
        assert_ne!(ours, 0, "the owner is not us");
        assert!(!dacl.is_null(), "a null DACL lets everyone in");
        // SAFETY: `dacl` points into `descriptor`.
        let count = unsafe { (*dacl).AceCount };
        assert_eq!(count, 1, "the DACL holds other entries");
        let mut ace = std::ptr::null_mut();
        // SAFETY: index 0 of an ACL with one entry.
        let got = unsafe { GetAce(dacl, 0, &mut ace) };
        assert_ne!(got, 0);
        let ace = ace.cast::<ACCESS_ALLOWED_ACE>();
        // SAFETY: the entry is an ACCESS_ALLOWED_ACE (checked by its type
        // first); its SID starts at `SidStart`.
        unsafe {
            assert_eq!((*ace).Header.AceType, 0, "not an allow entry");
            let sid = std::ptr::addr_of_mut!((*ace).SidStart).cast();
            assert_ne!(EqualSid(sid, us.sid()), 0, "the entry is not for us");
        }
    }

    /// Done when: a token of this user with its user SID made deny-only,
    /// as another account's would be, can't open the pipe even to read,
    /// which the default DACL grants Everyone.
    #[test]
    fn a_token_without_the_owner_is_refused() {
        let name = test_name("restricted");
        let _pipe = Owned(create(&name, true).unwrap());
        let us = TokenUserSid::this_process().unwrap();
        let mut token = std::ptr::null_mut();
        let access = TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY | TOKEN_IMPERSONATE;
        // SAFETY: the pseudo-handle and an out-pointer closed by `Owned`.
        let opened = unsafe { OpenProcessToken(GetCurrentProcess(), access, &mut token) };
        assert_ne!(opened, 0);
        let token = Owned(token);
        let disable = SID_AND_ATTRIBUTES {
            Sid: us.sid(),
            Attributes: 0,
        };
        let mut restricted = std::ptr::null_mut();
        // SAFETY: one SID to disable, valid for the call; the new token is
        // closed by `Owned`.
        let ok = unsafe {
            CreateRestrictedToken(
                token.0,
                DISABLE_MAX_PRIVILEGE,
                1,
                &disable,
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                &mut restricted,
            )
        };
        assert_ne!(ok, 0, "{}", io::Error::last_os_error());
        let restricted = Owned(restricted);
        let wide = wide(&name);
        // SAFETY: impersonation on this test thread only, reverted below
        // before anything else runs on it.
        let error = unsafe {
            assert_ne!(ImpersonateLoggedOnUser(restricted.0), 0);
            let pipe = CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            let error = GetLastError();
            assert_ne!(RevertToSelf(), 0);
            if pipe != INVALID_HANDLE_VALUE {
                CloseHandle(pipe);
                0
            } else {
                error
            }
        };
        assert_eq!(error, ERROR_ACCESS_DENIED, "the restricted token got in");
    }

    /// Done when: the shim's server check refuses a process of another
    /// account (System, pid 4) and accepts its own.
    #[test]
    fn a_server_of_another_account_is_not_ours() {
        assert!(!runs_as_us(4));
        assert!(runs_as_us(std::process::id()));
    }

    /// Done when: the user check tells another account's SID (System's)
    /// from this user's.
    #[test]
    fn another_accounts_sid_is_not_ours() {
        use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
        let us = TokenUserSid::this_process().unwrap();
        let system = wide("S-1-5-18");
        let mut sid = std::ptr::null_mut();
        // SAFETY: `system` is NUL-terminated; `sid` is LocalFree'd below.
        let converted = unsafe { ConvertStringSidToSidW(system.as_ptr(), &mut sid) };
        assert_ne!(converted, 0);
        assert!(!us.is(sid), "System's SID taken for ours");
        assert!(us.is(us.sid()));
        // SAFETY: allocated by the call above, freed once.
        unsafe { LocalFree(sid) };
    }

    /// Done when: a name already held makes mdoc not listen.
    #[test]
    fn a_name_held_by_another_listener_is_refused() {
        let name = test_name("held");
        let _squatter = Owned(create(&name, false).unwrap());
        let answer: Handler = Arc::new(|request: &[u8]| request.to_vec());
        assert!(listen(&name, answer).is_err());
    }
}
