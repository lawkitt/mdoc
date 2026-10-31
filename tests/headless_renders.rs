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

/// One screen name per line. `merge=union` in `.gitattributes` keeps both
/// sides when parallel branches add screens, so the list never conflicts.
fn screens() -> Vec<&'static str> {
    // A union merge can repeat a line both sides added; one copy counts.
    let mut screens: Vec<_> = include_str!("headless_screens.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    screens.sort_unstable();
    screens.dedup();
    screens
}

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
    // CI retains the same contract-tested run as an artifact. Local runs keep
    // the temporary output (including the spaces-in-path check) by default.
    let out = std::env::var_os("MDOC_HEADLESS_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().join("renders with spaces"));
    let mut child = render_command()
        .arg("--headless-renders")
        .arg(&out)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    // WARP rasterizes every 2x frame in software; allow slower hosted Windows
    // CPUs as well as this workstation. Hosted macOS runners vary too (one
    // took over 300 s). This is a hang watchdog, not a budget.
    let timeout = Duration::from_secs(if cfg!(target_os = "windows") {
        1200
    } else {
        600
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
    let screens = screens();
    for record in records {
        let name = record["screen"].as_str().unwrap();
        assert!(
            screens.contains(&name),
            "{name} is not in tests/headless_screens.txt"
        );
    }
    assert_eq!(records.len(), screens.len() * 2);
    for theme in ["light", "dark"] {
        for name in &screens {
            let matches: Vec<_> = records
                .iter()
                .filter(|r| r["theme"] == theme && r["screen"] == *name)
                .collect();
            assert_eq!(matches.len(), 1, "{theme}/{name}");
            let record = matches[0];
            if *name == "merge-hint" {
                assert_eq!(
                    record["state"]["merge_hints"], 1,
                    "{theme}: live merge hint"
                );
            }
            // The shelf (ADR 0084): only ready rows copy or drag and show a
            // preview, of prose without Markdown syntax or an original;
            // locked cases hide closed members and ask to unlock.
            if name.starts_with("shelf") && *name != "shelf-note" {
                let state = &record["state"];
                let rows = state["rows"].as_array().unwrap();
                let states: Vec<_> = rows.iter().map(|r| r["state"].clone()).collect();
                match *name {
                    "shelf" | "shelf-copied" => assert_eq!(
                        states,
                        ["ready", "ready", "2 left to review", "Changed since review"],
                        "{theme}/{name}"
                    ),
                    "shelf-empty" => assert!(rows.is_empty(), "{theme}/{name}"),
                    "shelf-locked" => assert_eq!(rows.len(), 3, "{theme}/{name}"),
                    _ => {}
                }
                for row in rows {
                    let title = row["title"].as_str().unwrap();
                    assert!(!title.ends_with(".md"), "{theme}/{name}: {title}");
                    match row["preview"].as_str() {
                        Some(preview) => {
                            assert_eq!(row["state"], "ready", "{theme}/{name}");
                            for hidden in ["**", "_notice_", "#", "Page 1", "Anna", "Northwind"] {
                                assert!(!preview.contains(hidden), "{theme}/{name}: {preview}");
                            }
                        }
                        None => assert_ne!(row["state"], "ready", "{theme}/{name}"),
                    }
                }
                assert_eq!(
                    state["copied"],
                    if *name == "shelf-copied" {
                        serde_json::json!(0)
                    } else {
                        Value::Null
                    },
                    "{theme}/{name}"
                );
                assert_eq!(
                    state["clipboard_locked"],
                    *name == "shelf-locked",
                    "{theme}/{name}"
                );
            }
            if *name == "shelf-note" {
                assert_eq!(
                    record["state"]["title"], "mdoc is still in the menu bar.",
                    "{theme}"
                );
            }
            if *name != "model-comparison" && !name.starts_with("shelf") {
                if matches!(
                    *name,
                    "home" | "document" | "navigation-disabled" | "navigation-enabled"
                ) {
                    let enabled = matches!(*name, "document" | "navigation-enabled");
                    assert_eq!(record["state"]["back_enabled"], enabled, "{theme}/{name}");
                    assert_eq!(record["state"]["forward_enabled"], false, "{theme}/{name}");
                    assert_eq!(
                        record["state"]["back_label"],
                        if enabled { "Back to Home" } else { "Back" }
                    );
                }
                if *name == "navigation-middle" {
                    assert_eq!(record["state"]["back_enabled"], true, "{theme}/{name}");
                    assert_eq!(record["state"]["forward_enabled"], true, "{theme}/{name}");
                    assert_eq!(record["state"]["back_label"], "Back to Home");
                    assert_eq!(record["state"]["forward_label"], "Forward to Untitled.md");
                }
                assert_eq!(
                    record["state"]["palette"],
                    *name == "command-palette",
                    "stale palette on {theme}/{name}"
                );
                assert_eq!(
                    record["state"]["shortcuts"],
                    *name == "keyboard-shortcuts",
                    "stale shortcuts sheet on {theme}/{name}"
                );
                assert_eq!(
                    record["state"]["find_replace"],
                    *name == "find-replace",
                    "stale find bar on {theme}/{name}"
                );
                // ADR 0078: the Reply view marks the one invented alias.
                assert_eq!(
                    record["state"]["reply_view"],
                    *name == "reply-view",
                    "{theme}/{name}"
                );
                assert_eq!(
                    record["state"]["reply_unresolved"],
                    if matches!(*name, "reply-view" | "share-reply") {
                        Value::from(1)
                    } else {
                        Value::Null
                    },
                    "{theme}/{name}"
                );
                if matches!(*name, "reply-view" | "share-reply") {
                    assert!(
                        record["tab_order"].as_array().unwrap().iter().any(|step| {
                            step["key_contexts"]
                                .as_str()
                                .is_some_and(|contexts| contexts.contains("ReplyBox"))
                        }),
                        "reply box missing from Tab walk on {theme}/{name}"
                    );
                }
                assert_eq!(
                    record["state"]["ocr_consent"],
                    *name == "ocr-consent",
                    "{theme}/{name}"
                );
                assert_eq!(
                    record["state"]["decision_history"],
                    *name == "decision-history"
                );
                if *name == "decision-history" {
                    // Apply, Undo replacement, then Keep, travelled back
                    // one: counts only, never labels.
                    let decisions = &record["state"]["decisions"];
                    assert_eq!(decisions["done"], 2);
                    assert_eq!(decisions["undone"], 1);
                    assert_eq!(decisions["past_limit"], 0);
                }
                assert_eq!(
                    record["state"]["settings"],
                    name.starts_with("settings-") || *name == "dictionary",
                    "stale settings on {theme}/{name}"
                );
                assert_eq!(
                    record["state"]["case_page"],
                    matches!(
                        *name,
                        "case-page" | "case-page-locked" | "rename-case" | "case-categories"
                    ),
                    "{theme}/{name}"
                );
                assert_eq!(
                    record["state"]["case_page_locked"],
                    *name == "case-page-locked",
                    "{theme}/{name}"
                );
                if *name == "case-page" {
                    assert!(record["state"]["case_page_documents"].as_u64().unwrap() >= 3);
                    assert!(record["state"]["case_page_names"].as_u64().unwrap() > 0);
                    let members = record["state"]["case_page_members"].as_array().unwrap();
                    assert_eq!(members[0]["open"], true, "{theme}: joined fixture is open");
                    let mentions = members[0]["mentions"].as_array().unwrap();
                    assert_eq!(
                        mentions.len(),
                        record["state"]["case_page_names"].as_u64().unwrap() as usize
                    );
                    assert!(
                        mentions.iter().all(|count| count.as_u64().unwrap() > 0),
                        "{theme}: every fixture name has live mentions"
                    );
                    assert_eq!(members[1]["open"], false);
                    assert!(members[1]["mentions"].is_null());
                }
                // ADR 0075: the primary button and the 20-file cap.
                let (primary, ticked) = match *name {
                    "folder-sheet" => (Value::from("Open as case"), Value::from(4)),
                    "folder-sheet-over-cap" => (Value::from("Open as case"), Value::from(20)),
                    "folder-sheet-add-to-case" => {
                        (Value::from("Add to “Public fixture”"), Value::from(4))
                    }
                    _ => (Value::Null, Value::Null),
                };
                assert_eq!(record["state"]["folder_sheet"], primary, "{theme}/{name}");
                assert_eq!(
                    record["state"]["folder_sheet_ticked"], ticked,
                    "{theme}/{name}"
                );
                let expected_category = match *name {
                    "settings-general" => Some("General"),
                    "settings-detection" | "settings-detection-categories" => Some("Detection"),
                    "settings-recognition" => Some("Recognition"),
                    "settings-spelling" | "dictionary" => Some("Spelling"),
                    "settings-cases" => Some("Cases"),
                    "settings-about" => Some("About"),
                    _ => None,
                };
                if let Some(category) = expected_category {
                    assert_eq!(
                        record["state"]["settings_category"], category,
                        "wrong settings category on {theme}/{name}"
                    );
                }
                assert_eq!(
                    record["state"]["dictionary"],
                    *name == "dictionary",
                    "stale dictionary on {theme}/{name}"
                );
                if *name == "settings-general" {
                    assert_eq!(record["state"]["theme_choice"], theme);
                    assert_eq!(
                        record["state"]["settings_theme_choice"],
                        if theme == "light" { "Light" } else { "Dark" }
                    );
                    assert_eq!(
                        record["state"]["appearance_in_view"], true,
                        "all three Appearance segments must be in view in {theme}"
                    );
                }
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
            assert_eq!(frames.len(), tabs.len() * 4 + 3);
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
    // ADR 0074: actual app controls, not a focus-handle assertion. Focus adds
    // only an accent outline; selected/on-state fills and labels stay intact.
    let rings = summary["focus_rings"].as_array().unwrap();
    assert_eq!(rings.len(), 16);
    for theme in ["light", "dark"] {
        for control in [
            "toggle-on",
            "sidebar-row",
            "settings-row",
            "beside-selected-tab",
            "comments-toggle",
            "decision-row",
            "navigation-back",
            "navigation-forward",
        ] {
            let pair = rings
                .iter()
                .find(|r| r["theme"] == theme && r["control"] == control)
                .unwrap();
            let dir = out.join(theme);
            let rest = image::open(dir.join(pair["rest"].as_str().unwrap()))
                .unwrap()
                .to_rgba8();
            let focused = image::open(dir.join(pair["focused"].as_str().unwrap()))
                .unwrap()
                .to_rgba8();
            let changed: Vec<_> = rest
                .enumerate_pixels()
                .filter_map(|(x, y, p)| (*p != *focused.get_pixel(x, y)).then_some((x, y)))
                .collect();
            assert!(!changed.is_empty(), "no visible focus: {theme}/{control}");
            let left = changed.iter().map(|(x, _)| *x).min().unwrap();
            let right = changed.iter().map(|(x, _)| *x).max().unwrap();
            let top = changed.iter().map(|(_, y)| *y).min().unwrap();
            let bottom = changed.iter().map(|(_, y)| *y).max().unwrap();
            assert!(
                right - left > 24 && bottom - top > 24,
                "ring bounds: {theme}/{control}"
            );
            assert!(
                changed.iter().all(|&(x, y)| {
                    x < left + 12 || x > right - 12 || y < top + 12 || y > bottom - 12
                }),
                "focus changed the fill or label: {theme}/{control}"
            );
            // At 2x, the two 2 px strokes occupy eight pixels across either
            // midline. Both sides must survive clipping and neighbouring fills.
            for count in [
                changed
                    .iter()
                    .filter(|&&(x, _)| x == (left + right) / 2)
                    .count(),
                changed
                    .iter()
                    .filter(|&&(_, y)| y == (top + bottom) / 2)
                    .count(),
            ] {
                assert!(
                    (8..=10).contains(&count),
                    "incomplete 2 px ring ({count} pixels): {theme}/{control}"
                );
            }
            let accent = if theme == "light" {
                [0x33, 0x6b, 0x5c]
            } else {
                [0x82, 0xb9, 0xa7]
            };
            assert!(
                changed
                    .iter()
                    .filter(|&&(x, y)| {
                        focused.get_pixel(x, y).0[..3]
                            .iter()
                            .zip(accent)
                            .all(|(&a, b)| a.abs_diff(b) <= 2)
                    })
                    .count()
                    > 50,
                "ring is not the accent: {theme}/{control}"
            );
        }
    }
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
