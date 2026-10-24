#![cfg(all(
    any(target_os = "macos", target_os = "windows"),
    debug_assertions,
    feature = "headless-renders"
))]

use serde_json::Value;
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const SCREENS: &[&str] = &[
    "home",
    "document",
    "more-menu",
    "review",
    "mention-popup",
    "setup-consent",
    "setup-consent-fp32",
    "settings",
    "dictionary",
    "command-palette",
    "find-replace",
    "before-after",
    "open-case",
    "rename-case",
    "case-categories",
    "keychain-explainer",
    "confirm-delete-case",
    "confirm-keep-mapping",
    "confirm-unsaved",
    "confirm-remove-model",
    "model-comparison",
    "conversion-failed",
    "restored-reply",
];

fn png(path: &Path) {
    let mut header = [0; 24];
    std::fs::File::open(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .read_exact(&mut header)
        .unwrap();
    assert_eq!(&header[..8], b"\x89PNG\r\n\x1a\n", "{}", path.display());
    assert_eq!(u32::from_be_bytes(header[16..20].try_into().unwrap()), 2200);
    assert_eq!(u32::from_be_bytes(header[20..24].try_into().unwrap()), 1520);
}

#[cfg(target_os = "macos")]
fn render_command() -> Command {
    // The child must succeed even when the real app profile and update cache
    // are inaccessible. No existing profile file is read, moved or modified.
    let mut policy = String::from("(version 1)(allow default)(deny network*)");
    for path in [
        dirs::data_local_dir().unwrap().join("mdoc"),
        dirs::cache_dir().unwrap().join("io.github.lawkitt.mdoc"),
    ] {
        let escaped = path
            .to_str()
            .unwrap()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        policy.push_str(&format!(
            "(deny file-read* file-write* (subpath \"{escaped}\"))"
        ));
    }
    let mut command = Command::new("/usr/bin/sandbox-exec");
    command.args(["-p", &policy, env!("CARGO_BIN_EXE_mdoc")]);
    command
}

#[cfg(target_os = "windows")]
fn render_command() -> Command {
    // App path redirection and request/key guards are checked by the runner.
    // This does not impose macOS sandbox-exec's OS file/network denial.
    Command::new(env!("CARGO_BIN_EXE_mdoc"))
}

#[cfg(target_os = "windows")]
fn native_windows_present(pid: u32) -> Result<bool, String> {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::*,
    };
    struct Probe {
        pid: u32,
        found: bool,
    }
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> i32 {
        let probe = unsafe { &mut *(data as *mut Probe) };
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
        probe.found |= owner == probe.pid;
        1
    }
    let mut probe = Probe { pid, found: false };
    // Include hidden top-level windows, not just visible/foreground windows.
    if unsafe { EnumWindows(Some(visit), &mut probe as *mut Probe as isize) } == 0 {
        return Err(format!("EnumWindows: {}", std::io::Error::last_os_error()));
    }
    Ok(probe.found)
}

#[test]
fn writes_both_themes_tab_walk_and_frame_costs_without_real_profile_access() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("renders with spaces");
    let mut child = render_command()
        .arg("--headless-renders")
        .arg(&out)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    // WARP rasterizes every 2x frame in software; allow slower hosted Windows
    // CPUs as well as this workstation. This is a hang watchdog, not a budget.
    let timeout = Duration::from_secs(if cfg!(target_os = "windows") {
        1200
    } else {
        300
    });
    while child.try_wait().unwrap().is_none() {
        #[cfg(target_os = "windows")]
        if native_windows_present(child.id()) != Ok(false) {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "native window check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if start.elapsed() > timeout {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "headless run timed out: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value =
        serde_json::from_slice(&std::fs::read(out.join("summary.json")).unwrap()).unwrap();
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(summary["platform"], std::env::consts::OS);
    assert_eq!(
        summary["renderer"],
        if cfg!(target_os = "windows") {
            "headless-directx-warp"
        } else {
            "headless-metal"
        }
    );
    assert_eq!(summary["isolation"]["os_case_keys_disabled"], true);
    assert_eq!(summary["isolation"]["network_requests_disabled"], true);
    let profile = Path::new(summary["profile_directory"].as_str().unwrap());
    for root in ["data_directory", "cache_directory"] {
        assert!(Path::new(summary["isolation"][root].as_str().unwrap()).starts_with(profile));
    }
    assert!(
        !Path::new(summary["profile_directory"].as_str().unwrap()).exists(),
        "temporary profile must be removed"
    );
    let records = summary["screens"].as_array().unwrap();
    assert_eq!(records.len(), SCREENS.len() * 2);
    for theme in ["light", "dark"] {
        for name in SCREENS {
            let matches: Vec<_> = records
                .iter()
                .filter(|r| r["theme"] == theme && r["screen"] == *name)
                .collect();
            assert_eq!(matches.len(), 1, "{theme}/{name}");
            let record = matches[0];
            if *name != "model-comparison" {
                assert_eq!(
                    record["state"]["palette"],
                    *name == "command-palette",
                    "stale palette on {theme}/{name}"
                );
                assert_eq!(
                    record["state"]["find_replace"],
                    *name == "find-replace",
                    "stale find bar on {theme}/{name}"
                );
                assert_eq!(
                    record["state"]["settings"],
                    matches!(*name, "settings" | "dictionary"),
                    "stale settings on {theme}/{name}"
                );
            }
            png(&out.join(theme).join(record["png"].as_str().unwrap()));
            let tabs = record["tab_order"].as_array().unwrap();
            assert!(!tabs.is_empty());
            assert_eq!(record["truncated"], false, "{theme}/{name}");
            let cycle = record["cycle_to"].as_u64().unwrap() as usize;
            assert_eq!(tabs[cycle]["focus_id"], tabs.last().unwrap()["focus_id"]);
            for step in tabs {
                assert!(
                    step["focus_id"].is_string(),
                    "missing focus on {theme}/{name}"
                );
                assert!(!step["key_contexts"].as_str().unwrap().is_empty());
                assert!(step["input_ms"].as_f64().unwrap() >= 0.);
                assert!(step["raster_readback_ms"].as_f64().unwrap() > 0.);
                png(&out.join(theme).join(step["png"].as_str().unwrap()));
            }
            let frames = record["frames"].as_array().unwrap();
            assert_eq!(frames.len(), tabs.len() + 3);
            assert!(
                frames
                    .iter()
                    .all(|f| f["layout_paint_ms"].as_f64().unwrap() > 0.)
            );
        }
    }
    let light = image::open(out.join("light/home.png")).unwrap().to_rgba8();
    let dark = image::open(out.join("dark/home.png")).unwrap().to_rgba8();
    assert_ne!(light, dark, "both themes must actually render");
    assert!(light.pixels().any(|p| p[3] != 0));
    // Existing output is rejected, so a partial/older run cannot pass as new evidence.
    let before = std::fs::read(out.join("summary.json")).unwrap();
    let rejected = render_command()
        .arg("--headless-renders")
        .arg(&out)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert_eq!(std::fs::read(out.join("summary.json")).unwrap(), before);
}
