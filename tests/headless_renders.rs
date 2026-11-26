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

/// A PNG of the window at the screen's viewport, at the test window's 2x.
fn png(path: &Path, viewport: &Value) {
    let mut header = [0; 24];
    std::fs::File::open(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .read_exact(&mut header)
        .unwrap();
    assert_eq!(&header[..8], b"\x89PNG\r\n\x1a\n", "{}", path.display());
    let size = |i: usize| (viewport[i].as_f64().unwrap() * 2.).round() as u32;
    assert_eq!(
        u32::from_be_bytes(header[16..20].try_into().unwrap()),
        size(0)
    );
    assert_eq!(
        u32::from_be_bytes(header[20..24].try_into().unwrap()),
        size(1)
    );
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
#[allow(unsafe_code)] // EnumWindows through Win32 (#897).
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
        // SAFETY: `data` is the `&mut Probe` passed to EnumWindows below,
        // which outlives the enumeration and is used by nothing else meanwhile.
        let probe = unsafe { &mut *(data as *mut Probe) };
        let mut owner = 0;
        // SAFETY: `hwnd` comes from EnumWindows; `owner` is a valid u32 to fill.
        unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
        probe.found |= owner == probe.pid;
        1
    }
    let mut probe = Probe { pid, found: false };
    // Include hidden top-level windows, not just visible/foreground windows.
    // SAFETY: `visit` matches WNDENUMPROC and reads `probe` only during this
    // call.
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
    // CPUs as well as this workstation (one queue run took 1177 s, #1243).
    // Hosted macOS runners vary too (one took over 300 s). This is a hang
    // watchdog, not a budget.
    let timeout = Duration::from_secs(if cfg!(target_os = "windows") {
        1500
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
    assert_eq!(summary["isolation"]["os_keys_disabled"], true);
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
            // Below 900 points the sidebar collapses by itself; expanded by
            // the user it stays; a drag widens it up to 320 (ADR 0027).
            match *name {
                "narrow-minimum" => assert_eq!(record["state"]["sidebar_expanded"], false),
                "narrow-sidebar-names" | "narrow-minimum-sidebar" => {
                    assert_eq!(record["state"]["sidebar_expanded"], true, "{theme}/{name}");
                    assert_eq!(record["state"]["sidebar_width"], 240.0, "{theme}/{name}");
                }
                "sidebar-wide" => {
                    assert_eq!(record["state"]["sidebar_width"], 320.0, "{theme}/{name}")
                }
                _ => {}
            }
            assert_eq!(
                record["state"]["scanning"] == true,
                *name == "review-scanning",
                "{theme}/{name}: only this screen shows a running scan"
            );
            if *name == "merge-hint" {
                assert_eq!(
                    record["state"]["merge_hints"], 1,
                    "{theme}: live merge hint"
                );
            }
            // The shelf (ADR 0084): only ready rows copy or drag and show a
            // preview, of prose without Markdown syntax or an original;
            // a locked Vault lists open tabs only and asks to unlock.
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
                    record["state"]["title"],
                    if cfg!(windows) {
                        "mdoc is still in the system tray."
                    } else {
                        "mdoc is still in the menu bar."
                    },
                    "{theme}"
                );
            }
            // The companion (ADR 0128): what is shared is always named, and
            // the restored reply shows only with the pointer over it.
            if name.starts_with("companion") {
                let state = &record["state"];
                assert_eq!(state["companion"], true, "{theme}/{name}");
                let shared = !matches!(*name, "companion-nothing-shared" | "companion-empty");
                assert_eq!(state["shared"].is_string(), shared, "{theme}/{name}");
                assert_eq!(
                    state["reply_shown"],
                    *name == "companion-reply-shown",
                    "{theme}/{name}"
                );
                assert_eq!(
                    state["review"].is_string(),
                    *name == "companion-review",
                    "{theme}/{name}"
                );
            }
            if *name != "model-comparison"
                && !name.starts_with("shelf")
                && !name.starts_with("companion")
            {
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
                    assert_eq!(record["state"]["forward_label"], "Forward to Meeting notes");
                }
                assert_eq!(
                    record["state"]["palette"],
                    matches!(
                        *name,
                        "command-palette" | "palette-search-title" | "palette-search-project"
                    ),
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
                if *name == "import-comments-note" {
                    // No rendering shows them: the left-out line counts the
                    // comments, and no notice does.
                    assert!(record["state"]["import_note"].is_null(), "{theme}/{name}");
                    assert_eq!(
                        record["state"]["left_out"], "2 comments not shown.",
                        "{theme}/{name}"
                    );
                }
                // The rail in a wide pane, markers in a 640×480 window.
                if let Some(rail) = match *name {
                    "comments-rail" | "comments-resolved" | "comments-hidden-text" => Some(true),
                    "comments-markers" => Some(false),
                    _ => None,
                } {
                    let state = &record["state"]["comment_rail"];
                    assert_eq!(state["shown"], true, "{theme}/{name}");
                    assert_eq!(state["rail"], rail, "{theme}/{name}");
                    let toggles = record["state"]["source_toggles"].as_array().unwrap();
                    // Five threads, one with a reply: every comment counts (#964).
                    assert_eq!(toggles[0]["label"], "💬 6", "{theme}/{name}");
                    assert_eq!(
                        toggles[1],
                        serde_json::json!({
                            "label": "Hidden text (1)",
                            "on": *name == "comments-hidden-text"
                        }),
                        "{theme}/{name}"
                    );
                }
                // A PDF's annotations: the same rail and markers, no
                // hidden-text control; off, the plain page.
                if let Some(rail) = match *name {
                    "pdf-comments-rail" | "pdf-comments-resolved" | "pdf-comments-rotated" => {
                        Some(true)
                    }
                    "pdf-comments-markers" => Some(false),
                    _ => None,
                } {
                    let state = &record["state"]["comment_rail"];
                    assert_eq!(state["shown"], true, "{theme}/{name}");
                    assert_eq!(state["rail"], rail, "{theme}/{name}");
                }
                if name.starts_with("pdf-comments-") {
                    assert_eq!(
                        record["state"]["source_toggles"],
                        serde_json::json!([{
                            "label": "💬 10",
                            "on": *name != "pdf-comments-hidden"
                        }]),
                        "{theme}/{name}"
                    );
                }
                if *name == "pdf-comments-hidden" {
                    assert_eq!(record["state"]["comment_rail"]["shown"], false);
                }
                if *name == "import-hidden-note" {
                    // Left out, not a partial import: the left-out line.
                    assert!(record["state"]["import_note"].is_null(), "{theme}/{name}");
                    assert_eq!(
                        record["state"]["left_out"], "6 hidden passages not imported.",
                        "{theme}/{name}"
                    );
                }
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
                    record["state"]["project_page"],
                    matches!(
                        *name,
                        "project-page"
                            | "project-page-replies"
                            | "project-page-names"
                            | "project-page-menu"
                            | "project-page-lists"
                            | "rename-project"
                            | "project-categories"
                            | "new-project-page"
                            | "ai-share-prompt"
                            | "ai-project-shared"
                            | "ai-delivery-badge"
                            | "project-page-activity"
                            | "ai-project-rail"
                            | "project-ask-menu"
                    ),
                    "{theme}/{name}"
                );
                // The move toast says where the fixture went, with Undo.
                assert_eq!(
                    record["state"]["move_toast"],
                    if *name == "project-move-toast" {
                        Value::from("“Services Agreement” moved to “Public fixture”.")
                    } else {
                        Value::Null
                    },
                    "{theme}/{name}"
                );
                if *name == "project-page" {
                    assert_eq!(record["state"]["project_page_documents"], 2);
                    assert!(record["state"]["project_page_names"].as_u64().unwrap() > 0);
                    let members = record["state"]["project_page_members"].as_array().unwrap();
                    // By title: "Services Agreement", then "Witness statement".
                    assert_eq!(members[0]["open"], true, "{theme}: moved fixture is open");
                    let mentions = members[0]["mentions"].as_array().unwrap();
                    assert_eq!(
                        mentions.len(),
                        record["state"]["project_page_names"].as_u64().unwrap() as usize
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
                    "folder-sheet" => (Value::from("Open as New Project"), Value::from(4)),
                    "folder-sheet-over-cap" => {
                        (Value::from("Open as New Project"), Value::from(20))
                    }
                    "folder-sheet-add-to-project" => {
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
                    "settings-general" | "settings-general-off" => Some("General"),
                    "settings-detection"
                    | "settings-detection-categories"
                    | "settings-models-installed"
                    | "settings-detection-end" => Some("Detection"),
                    "settings-recognition" => Some("Recognition"),
                    "settings-spelling" | "dictionary" => Some("Spelling"),
                    "settings-vault" | "settings-vault-recovery-key" => Some("Vault"),
                    "settings-about" => Some("About"),
                    "settings-ai-connector-off"
                    | "settings-ai-connector"
                    | "settings-ai-connector-failed"
                    | "settings-ai-connector-consent"
                    | "settings-ai-connector-consent-chatgpt" => Some("AI connector"),
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
                // ADR 0076: a page scrolled to its end fades only above.
                if *name == "settings-detection-end" {
                    assert_eq!(
                        record["state"]["settings_scroll_cues"],
                        serde_json::json!([true, false]),
                        "{theme}/{name}"
                    );
                }
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
            let viewport = &record["viewport"];
            let expected = match *name {
                // The window's minimum size (#303, ADR 0027).
                "applied-alias-selected" | "narrow-minimum" | "narrow-minimum-sidebar" => {
                    [720., 480.]
                }
                // Under the 1100 points other screens use, still over 900.
                "narrow-sidebar-names" | "sidebar-wide" => [1000., 700.],
                // Wide enough for the comment rail beside the page; and the
                // 640×480 window the comments spec names for markers.
                "comments-rail"
                | "comments-resolved"
                | "comments-hidden-text"
                | "pdf-comments-rail"
                | "pdf-comments-resolved"
                | "pdf-comments-rotated"
                | "pdf-comments-hidden"
                | "pdf-unlocked" => [1500., 900.],
                "comments-markers" | "pdf-comments-markers" => [640., 480.],
                _ => [1100., 760.],
            };
            assert_eq!(viewport, &serde_json::json!(expected), "{theme}/{name}");
            png(
                &out.join(theme).join(record["png"].as_str().unwrap()),
                viewport,
            );
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
                png(
                    &out.join(theme).join(step["png"].as_str().unwrap()),
                    viewport,
                );
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
    assert_eq!(rings.len(), 18);
    for theme in ["light", "dark"] {
        for control in [
            "toggle-on",
            "sidebar-row",
            "settings-row",
            "settings-appearance",
            "beside-selected-tab",
            "comment-card",
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
