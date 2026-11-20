// Every mdoc process keeps itself out of Windows Error Reporting: `main`
// sets SEM_NOGPFAULTERRORBOX before any mode runs. The probe flag exists in
// debug builds only, so this test does too.
#![cfg(all(windows, debug_assertions))]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

const SEM_NOGPFAULTERRORBOX: u32 = 0x0002;
const CREATE_DEFAULT_ERROR_MODE: u32 = 0x0400_0000;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// mdoc sets the flag itself: the child starts from the default error mode,
/// not this test runner's.
#[test]
fn mdoc_starts_with_windows_error_reporting_off() {
    let output = Command::new(Path::new(env!("CARGO_BIN_EXE_mdoc")))
        .arg("--mdoc-error-mode")
        .creation_flags(CREATE_DEFAULT_ERROR_MODE | CREATE_NO_WINDOW)
        .output()
        .expect("start mdoc");
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8_lossy(&output.stdout);
    let mode = u32::from_str_radix(text.trim().trim_start_matches("0x"), 16)
        .unwrap_or_else(|_| panic!("not an error mode: {text:?}"));
    assert_ne!(mode & SEM_NOGPFAULTERRORBOX, 0, "error mode {mode:#x}");
}
