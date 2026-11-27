//! Settings › AI connector in the window: what each button shows and
//! writes, against temp Claude Desktop and ChatGPT folders.
use super::super::{Category, CloseSettings, Event, Panel};
use crate::ai_connector::setup::{Host, Places, SHIM, Status, backup};
use crate::style::Theme;
use gpui::{TestAppContext, VisualTestContext, px, size};
use mdoc_store::settings::{Preferences, Shared, Store};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

/// Temp Claude Desktop and ChatGPT folders, each config holding another
/// server, and an installed shim.
struct Apps {
    _dir: tempfile::TempDir,
    places: Places,
}

const CLAUDE: &str =
    "{\"mcpServers\": {\"filesystem\": {\"command\": \"npx\"}}, \"theme\": \"dark\"}\n";
const CODEX: &str = "# mine\nmodel = \"gpt\"\n\n[mcp_servers.docs]\ncommand = \"docs\"\n";

fn apps(claude: Option<&str>, codex: Option<&str>) -> Apps {
    let dir = tempfile::tempdir().unwrap();
    let claude_folder = dir.path().join("Roaming").join("Claude");
    let codex_folder = dir.path().join("User").join(".codex");
    std::fs::create_dir_all(&claude_folder).unwrap();
    std::fs::create_dir_all(&codex_folder).unwrap();
    let shim = dir.path().join("mdoc").join(SHIM);
    std::fs::create_dir_all(shim.parent().unwrap()).unwrap();
    std::fs::write(&shim, b"shim").unwrap();
    if let Some(text) = claude {
        std::fs::write(claude_folder.join("claude_desktop_config.json"), text).unwrap();
    }
    if let Some(text) = codex {
        std::fs::write(codex_folder.join("config.toml"), text).unwrap();
    }
    Apps {
        places: Places {
            config_dir: Some(dir.path().join("Roaming")),
            local_dir: None,
            home: Some(dir.path().join("User")),
            shim: Some(shim),
            dev_profile: None,
        },
        _dir: dir,
    }
}

impl Apps {
    fn text(&self, host: Host) -> Option<String> {
        std::fs::read_to_string(self.places.config(host).unwrap()).ok()
    }
    fn json(&self) -> Value {
        serde_json::from_str(&self.text(Host::Claude).unwrap()).unwrap()
    }
    fn shim(&self) -> String {
        self.places.shim.as_ref().unwrap().to_str().unwrap().into()
    }
}

/// Settings open on AI connector, with the preview `on` or off, looking
/// in `apps`.
fn page<'a>(
    apps: &Apps,
    on: bool,
    cx: &'a mut TestAppContext,
) -> (gpui::Entity<Panel>, Shared, &'a mut VisualTestContext) {
    let shared = Store::in_memory();
    let mut preferences = Preferences::default();
    preferences.ai_connector.preview = on;
    shared.borrow_mut().current = Some(preferences);
    let (panel, cx) = cx
        .add_window_view(|_, cx| Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx));
    cx.simulate_resize(size(px(1100.), px(2400.)));
    let places = apps.places.clone();
    panel.update_in(cx, |p, window, cx| {
        // Not shown: opening would check models through the global job slot.
        p.open = true;
        p.use_ai_places(places, cx);
        p.change_category(Category::AiConnector, window, cx);
    });
    draw(cx);
    (panel, shared, cx)
}

fn draw(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
    }
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("{id} isn't shown"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    draw(cx);
}

fn shown(cx: &mut VisualTestContext, id: &'static str) -> bool {
    cx.debug_bounds(id).is_some()
}

fn status(panel: &gpui::Entity<Panel>, host: Host, cx: &mut VisualTestContext) -> Option<Status> {
    cx.update(|_, cx| panel.read(cx).ai.status(host).cloned())
}

/// Done when: with the preview off the page is the switch alone, and the
/// switch saves `ai_connector.preview` and shows the rest at once.
#[gpui::test]
fn the_page_is_the_switch_alone_until_the_preview_is_on(cx: &mut TestAppContext) {
    let apps = apps(Some(CLAUDE), Some(CODEX));
    let (panel, shared, cx) = page(&apps, false, cx);
    assert!(shown(cx, "ai-connector-preview"));
    for id in [
        "ai-claude-add",
        "ai-chatgpt-add",
        "ai-claude-test",
        "ai-copy-path",
        "reset-settings",
    ] {
        assert!(!shown(cx, id), "{id} shows with the preview off");
    }
    // The apps' files aren't even read.
    assert_eq!(status(&panel, Host::Claude, cx), None);
    click(cx, "ai-connector-preview");
    assert!(shared.borrow().snapshot().unwrap().ai_connector.preview);
    assert!(shown(cx, "ai-claude-add") && shown(cx, "ai-chatgpt-add"));
    assert!(shown(cx, "ai-copy-path"));
    assert_eq!(status(&panel, Host::Claude, cx), Some(Status::NotAdded));
    assert_eq!(status(&panel, Host::ChatGpt, cx), Some(Status::NotAdded));
    click(cx, "ai-connector-preview");
    assert!(!shared.borrow().snapshot().unwrap().ai_connector.preview);
    assert!(!shown(cx, "ai-claude-add"));
    assert_eq!(apps.text(Host::Claude).as_deref(), Some(CLAUDE));
    assert_eq!(apps.text(Host::ChatGpt).as_deref(), Some(CODEX));
}

/// Done when: Add shows the file and the exact entry, writes nothing on
/// Cancel, and writes that entry, keeping the rest, on the yes.
#[gpui::test]
fn claude_add_shows_the_entry_and_writes_it_only_on_yes(cx: &mut TestAppContext) {
    let apps = apps(Some(CLAUDE), None);
    let (panel, _, cx) = page(&apps, true, cx);
    click(cx, "ai-claude-add");
    let entry = json!({"command": apps.shim(), "args": []});
    cx.update(|_, cx| {
        let change = (panel.read(cx).ai_consent())
            .expect("the consent card")
            .clone();
        assert_eq!(Some(change.path), apps.places.config(Host::Claude));
        assert_eq!(change.entry, entry);
    });
    assert!(shown(cx, "ai-consent-entry") && shown(cx, "ai-consent-path"));
    assert!(!shown(cx, "ai-claude-add"), "the card replaces the page");
    assert_eq!(
        apps.text(Host::Claude).as_deref(),
        Some(CLAUDE),
        "asking writes nothing"
    );
    click(cx, "ai-consent-cancel");
    cx.update(|_, cx| assert!(panel.read(cx).ai_consent().is_none()));
    assert_eq!(apps.text(Host::Claude).as_deref(), Some(CLAUDE));
    click(cx, "ai-claude-add");
    click(cx, "ai-consent-yes");
    let mut expected: Value = serde_json::from_str(CLAUDE).unwrap();
    expected["mcpServers"]["mdoc"] = entry;
    assert_eq!(apps.json(), expected);
    let config = apps.places.config(Host::Claude).unwrap();
    assert_eq!(std::fs::read_to_string(backup(&config)).unwrap(), CLAUDE);
    assert_eq!(status(&panel, Host::Claude, cx), Some(Status::Added));
    assert!(!shown(cx, "ai-claude-add"));
    assert!(shown(cx, "ai-claude-remove") && shown(cx, "ai-claude-test"));
    // ChatGPT's file is another app's: untouched.
    assert_eq!(apps.text(Host::ChatGpt), None);
}

/// Done when: ChatGPT's Add shows the `[mcp_servers.mdoc]` table and
/// writes it on the yes; its Remove takes it out again.
#[gpui::test]
fn chatgpt_add_and_remove_write_only_on_yes(cx: &mut TestAppContext) {
    let apps = apps(None, Some(CODEX));
    let (panel, _, cx) = page(&apps, true, cx);
    click(cx, "ai-chatgpt-add");
    cx.update(|_, cx| {
        let change = panel.read(cx).ai_consent().expect("the consent card");
        assert_eq!(change.host, Host::ChatGpt);
        assert!(change.shown().starts_with("[mcp_servers.mdoc]\ncommand = "));
    });
    assert_eq!(
        apps.text(Host::ChatGpt).as_deref(),
        Some(CODEX),
        "asking writes nothing"
    );
    click(cx, "ai-consent-yes");
    let written = apps.text(Host::ChatGpt).unwrap();
    assert!(written.starts_with(CODEX), "{written}");
    let doc: toml_edit::DocumentMut = written.parse().unwrap();
    assert_eq!(
        doc["mcp_servers"]["mdoc"]["command"].as_str(),
        Some(apps.shim().as_str())
    );
    assert_eq!(status(&panel, Host::ChatGpt, cx), Some(Status::Added));
    // No link opens a Work thread: the row says what to ask instead.
    assert!(!shown(cx, "ai-chatgpt-test"));
    click(cx, "ai-chatgpt-remove");
    assert!(shown(cx, "ai-consent"));
    assert_eq!(apps.text(Host::ChatGpt).as_deref(), Some(written.as_str()));
    click(cx, "ai-consent-yes");
    assert_eq!(
        apps.text(Host::ChatGpt).unwrap().trim_end(),
        CODEX.trim_end()
    );
    assert_eq!(status(&panel, Host::ChatGpt, cx), Some(Status::NotAdded));
    assert_eq!(apps.text(Host::Claude), None);
}

/// Done when: Remove shows the entry and takes out only mdoc's.
#[gpui::test]
fn claude_remove_asks_and_takes_out_only_mdocs_entry(cx: &mut TestAppContext) {
    let apps = apps(Some(CLAUDE), None);
    let (_, _, cx) = page(&apps, true, cx);
    click(cx, "ai-claude-add");
    click(cx, "ai-consent-yes");
    click(cx, "ai-claude-remove");
    assert!(shown(cx, "ai-consent"));
    assert!(
        apps.json()["mcpServers"].get("mdoc").is_some(),
        "asking writes nothing"
    );
    click(cx, "ai-consent-yes");
    assert_eq!(apps.json(), serde_json::from_str::<Value>(CLAUDE).unwrap());
    assert!(shown(cx, "ai-claude-add"));
}

/// Done when: a file mdoc can't parse is refused on the page: no button
/// offers to change it, and it stays as it was.
#[gpui::test]
fn a_file_mdoc_cannot_parse_offers_no_change(cx: &mut TestAppContext) {
    let broken_json = "{\"mcpServers\": {\"a\": ";
    let broken_toml = "[mcp_servers.docs\ncommand = 1\n";
    let apps = apps(Some(broken_json), Some(broken_toml));
    let (panel, _, cx) = page(&apps, true, cx);
    for host in Host::ALL {
        assert!(matches!(
            status(&panel, host, cx),
            Some(Status::Unreadable(_))
        ));
    }
    for id in [
        "ai-claude-add",
        "ai-claude-remove",
        "ai-claude-test",
        "ai-chatgpt-add",
        "ai-chatgpt-remove",
    ] {
        assert!(!shown(cx, id), "{id} offered for an unparsable file");
    }
    // A plan made anyway is refused and says so.
    for host in Host::ALL {
        panel.update_in(cx, |p, window, cx| p.ask_ai_change(host, false, window, cx));
        draw(cx);
        assert!(shown(cx, "ai-connector-error"));
        cx.update(|_, cx| assert!(panel.read(cx).ai_consent().is_none()));
    }
    assert_eq!(apps.text(Host::Claude).as_deref(), Some(broken_json));
    assert_eq!(apps.text(Host::ChatGpt).as_deref(), Some(broken_toml));
}

/// A file changed between the card and the yes is not written.
#[gpui::test]
fn a_yes_to_a_file_that_changed_since_writes_nothing(cx: &mut TestAppContext) {
    let apps = apps(Some(CLAUDE), None);
    let (_, _, cx) = page(&apps, true, cx);
    click(cx, "ai-claude-add");
    std::fs::write(apps.places.config(Host::Claude).unwrap(), "{}").unwrap();
    click(cx, "ai-consent-yes");
    assert_eq!(apps.text(Host::Claude).as_deref(), Some("{}"));
    assert!(shown(cx, "ai-connector-error"));
}

/// Esc on the card cancels it and leaves Settings open.
#[gpui::test]
fn esc_on_the_card_cancels_it(cx: &mut TestAppContext) {
    let apps = apps(None, None);
    let (panel, _, cx) = page(&apps, true, cx);
    click(cx, "ai-claude-add");
    cx.dispatch_action(CloseSettings);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let panel = panel.read(cx);
        assert!(panel.ai_consent().is_none());
        assert!(panel.open);
    });
    assert_eq!(apps.text(Host::Claude), None);
}

/// Test connection opens a new Claude chat with the first prompt filled
/// in, and the page says so.
#[gpui::test]
fn test_connection_opens_a_prefilled_claude_chat(cx: &mut TestAppContext) {
    let apps = apps(None, None);
    let (panel, _, cx) = page(&apps, true, cx);
    assert!(!shown(cx, "ai-claude-test"), "nothing to test before Add");
    click(cx, "ai-claude-add");
    click(cx, "ai-consent-yes");
    click(cx, "ai-claude-test");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("claude://claude.ai/new?q=List%20my%20documents%20in%20mdoc.")
    );
    cx.update(|_, cx| assert!(panel.read(cx).ai.tested));
}

/// Copy path copies the installed shim's path, for any other AI app.
#[gpui::test]
fn copy_path_copies_the_installed_shim(cx: &mut TestAppContext) {
    let apps = apps(None, None);
    let (_, _, cx) = page(&apps, true, cx);
    click(cx, "ai-copy-path");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(apps.shim())
    );
    assert_eq!(apps.text(Host::Claude), None);
}

/// Without a shim beside mdoc, Add can't be chosen.
#[gpui::test]
fn without_an_installed_shim_add_does_nothing(cx: &mut TestAppContext) {
    let mut apps = apps(None, None);
    apps.places.shim = None;
    let (panel, _, cx) = page(&apps, true, cx);
    click(cx, "ai-claude-add");
    click(cx, "ai-chatgpt-add");
    cx.update(|_, cx| assert!(panel.read(cx).ai_consent().is_none()));
    assert!(!shown(cx, "ai-copy-path"));
    assert_eq!(apps.text(Host::Claude), None);
    assert_eq!(apps.text(Host::ChatGpt), None);
}

/// A switch save that fails puts the switch back on, and the apps' rows
/// with it.
#[gpui::test]
fn a_failed_switch_save_shows_the_rows_again(cx: &mut TestAppContext) {
    let apps = apps(Some(CLAUDE), None);
    let (panel, shared, cx) = page(&apps, true, cx);
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("not-a-directory");
    std::fs::write(&blocked, b"blocked").unwrap();
    shared.borrow_mut().path = Some(blocked.join("settings.json"));
    click(cx, "ai-connector-preview");
    assert!(shared.borrow().snapshot().unwrap().ai_connector.preview);
    assert_eq!(status(&panel, Host::Claude, cx), Some(Status::NotAdded));
    assert!(shown(cx, "ai-claude-add"));
}

/// #1220: a failed start shows under the switch while it is on, and not
/// with the switch off.
#[gpui::test]
fn a_failed_start_shows_under_the_switch(cx: &mut TestAppContext) {
    let apps = apps(Some(CLAUDE), Some(CODEX));
    let (panel, _, cx) = page(&apps, true, cx);
    assert!(!shown(cx, "ai-connector-failed"));
    let why = "Couldn't start the connector: another copy of mdoc is using it.";
    panel.update(cx, |p, cx| p.ai_connector_failed(Some(why.into()), cx));
    draw(cx);
    assert!(shown(cx, "ai-connector-failed"));
    assert!(shown(cx, "ai-claude-add"), "the rest of the page stays");
    click(cx, "ai-connector-preview");
    assert!(!shown(cx, "ai-connector-failed"));
}

/// #1220: switched off, the page tells the window at once, before its
/// save lands, so the connector stops while the switch shows off; a save
/// that fails turns it back on.
#[gpui::test]
fn off_is_told_before_its_save_lands(cx: &mut TestAppContext) {
    use std::{cell::RefCell, time::Duration};
    let apps = apps(Some(CLAUDE), None);
    let (panel, shared, cx) = page(&apps, true, cx);
    let told = Rc::new(RefCell::new(Vec::new()));
    let _told = cx.update(|_, cx| {
        let told = told.clone();
        cx.subscribe(&panel, move |_, event: &Event, _| {
            if let Event::AiConnectorShown(on) = event {
                told.borrow_mut().push(*on);
            }
        })
    });
    panel.update(cx, |p, cx| {
        let gate = cx.background_executor().timer(Duration::from_secs(1));
        p.save_gate = Some(cx.background_executor().spawn(gate));
    });
    click(cx, "ai-connector-preview");
    assert_eq!(*told.borrow(), [false]);
    assert!(
        shared.borrow().snapshot().unwrap().ai_connector.preview,
        "the save is still held"
    );
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert!(!shared.borrow().snapshot().unwrap().ai_connector.preview);
    // On again saves; then an off whose save fails is on again.
    click(cx, "ai-connector-preview");
    assert!(shared.borrow().snapshot().unwrap().ai_connector.preview);
    assert_eq!(*told.borrow(), [false]);
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("not-a-directory");
    std::fs::write(&blocked, b"blocked").unwrap();
    shared.borrow_mut().path = Some(blocked.join("settings.json"));
    click(cx, "ai-connector-preview");
    assert_eq!(*told.borrow(), [false, false, true]);
    panel.update(cx, |p, _| assert!(p.draft.ai_connector.preview));
}

/// "Open deliveries when they arrive" shows only with the preview on, is
/// off by default, and saves `ai_connector.open_deliveries`.
#[gpui::test]
fn open_deliveries_shows_with_the_preview_and_saves(cx: &mut TestAppContext) {
    let apps = apps(None, None);
    let (panel, shared, cx) = page(&apps, false, cx);
    assert!(
        !shown(cx, "ai-open-deliveries"),
        "shown with the preview off"
    );
    click(cx, "ai-connector-preview");
    assert!(shown(cx, "ai-open-deliveries"));
    let saved = |shared: &Shared| shared.borrow().snapshot().unwrap().ai_connector;
    assert!(!saved(&shared).open_deliveries, "off by default");
    click(cx, "ai-open-deliveries");
    assert!(saved(&shared).open_deliveries);
    assert!(saved(&shared).preview, "the preview stays on");
    panel.update(cx, |p, _| assert!(p.draft.ai_connector.open_deliveries));
    click(cx, "ai-open-deliveries");
    assert!(!saved(&shared).open_deliveries);
    // Off with the preview, it stays as saved.
    click(cx, "ai-open-deliveries");
    click(cx, "ai-connector-preview");
    assert!(!shown(cx, "ai-open-deliveries"));
    assert!(saved(&shared).open_deliveries);
}

/// A failed save of "Open deliveries when they arrive" puts the switch
/// back and says so.
#[gpui::test]
fn a_failed_open_deliveries_save_restores_it(cx: &mut TestAppContext) {
    let apps = apps(None, None);
    let (panel, shared, cx) = page(&apps, true, cx);
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("not-a-directory");
    std::fs::write(&blocked, b"blocked").unwrap();
    shared.borrow_mut().path = Some(blocked.join("settings.json"));
    click(cx, "ai-open-deliveries");
    assert!(
        !shared
            .borrow()
            .snapshot()
            .unwrap()
            .ai_connector
            .open_deliveries
    );
    panel.update(cx, |p, _| {
        assert!(
            !p.draft.ai_connector.open_deliveries,
            "the switch is back off"
        );
        assert!(p.draft.ai_connector.preview);
    });
    assert!(shown(cx, "settings-error"));
    assert!(shown(cx, "ai-open-deliveries"));
}

/// Done when (item 15): "Companion shortcut" shows with the preview on, at
/// ⌥M and on; its recorder saves the next shortcut pressed, before any
/// binding takes it, and Esc stops it without a change.
#[cfg(target_os = "macos")]
#[gpui::test]
fn the_shortcut_is_option_m_and_records_another(cx: &mut TestAppContext) {
    use mdoc_store::settings::Hotkey;
    let apps = apps(None, None);
    let (panel, shared, cx) = page(&apps, false, cx);
    assert!(!shown(cx, "ai-hotkey-record"), "shown with the preview off");
    click(cx, "ai-connector-preview");
    assert!(shown(cx, "ai-hotkey-record") && shown(cx, "ai-hotkey"));
    let saved = |shared: &Shared| shared.borrow().snapshot().unwrap().ai_connector;
    assert_eq!(saved(&shared).hotkey, Hotkey::COMPANION);
    assert!(!saved(&shared).hotkey_off);
    click(cx, "ai-hotkey-record");
    assert!(shown(cx, "ai-hotkey-hint"), "it says what to press");
    // A key without ⌘, ⌃ or ⌥ isn't one: it says why and keeps waiting.
    cx.simulate_keystrokes("shift-a");
    draw(cx);
    panel.update(cx, |p, _| {
        assert!(p.ai.recording.is_some());
        assert!(
            (p.ai.hotkey_hint.as_deref().unwrap()).starts_with("Hold ⌘, ⌃ or ⌥"),
            "{:?}",
            p.ai.hotkey_hint
        );
    });
    cx.simulate_keystrokes("ctrl-shift-space");
    draw(cx);
    let changed = Hotkey::new(true, false, true, false, "Space").unwrap();
    assert_eq!(saved(&shared).hotkey, changed);
    assert!(!shown(cx, "ai-hotkey-hint"));
    panel.update(cx, |p, _| assert!(p.ai.recording.is_none()));
    // Esc stops recording and changes nothing; Settings' own Esc binding
    // never sees it.
    click(cx, "ai-hotkey-record");
    cx.simulate_keystrokes("escape");
    draw(cx);
    panel.update(cx, |p, _| {
        assert!(p.ai.recording.is_none());
        assert!(p.open, "Esc didn't close Settings");
    });
    assert_eq!(saved(&shared).hotkey, changed);
    // Leaving the page stops it too.
    click(cx, "ai-hotkey-record");
    panel.update_in(cx, |p, window, cx| {
        p.change_category(Category::General, window, cx)
    });
    panel.update(cx, |p, _| assert!(p.ai.recording.is_none()));
}

/// Done when (item 15): the switch turns the shortcut off and on again,
/// keeping the recorded one, and off the recorder records nothing.
#[cfg(target_os = "macos")]
#[gpui::test]
fn the_switch_turns_the_shortcut_off_and_keeps_it(cx: &mut TestAppContext) {
    use mdoc_store::settings::Hotkey;
    let apps = apps(None, None);
    let (panel, shared, cx) = page(&apps, true, cx);
    let saved = |shared: &Shared| shared.borrow().snapshot().unwrap().ai_connector;
    click(cx, "ai-hotkey");
    assert!(saved(&shared).hotkey_off);
    click(cx, "ai-hotkey-record");
    panel.update(cx, |p, _| {
        assert!(p.ai.recording.is_none(), "off: no recorder")
    });
    click(cx, "ai-hotkey");
    assert!(!saved(&shared).hotkey_off);
    assert_eq!(saved(&shared).hotkey, Hotkey::COMPANION);
    // Review of #1268: either switch going off while it records stops the
    // recorder, so the next key reaches the app again.
    for switch in ["ai-hotkey", "ai-connector-preview"] {
        click(cx, "ai-hotkey-record");
        panel.update(cx, |p, _| assert!(p.ai.recording.is_some()));
        click(cx, switch);
        panel.update(cx, |p, _| assert!(p.ai.recording.is_none(), "{switch}"));
        cx.simulate_keystrokes("ctrl-shift-space");
        assert_eq!(saved(&shared).hotkey, Hotkey::COMPANION, "{switch}");
        click(cx, switch);
    }
}

/// Done when (item 15): a shortcut that couldn't be registered shows
/// under the row, and not while the shortcut is off.
#[cfg(target_os = "macos")]
#[gpui::test]
fn a_failed_registration_shows_under_the_row(cx: &mut TestAppContext) {
    use mdoc_store::settings::Hotkey;
    let apps = apps(None, None);
    let (panel, _, cx) = page(&apps, true, cx);
    assert!(!shown(cx, "ai-hotkey-failed"));
    let why = crate::companion::hotkey::Refused::Taken.message(Hotkey::COMPANION);
    panel.update(cx, |p, cx| p.hotkey_failed(Some(why), cx));
    draw(cx);
    assert!(shown(cx, "ai-hotkey-failed"));
    assert!(shown(cx, "ai-hotkey-record"), "the row stays");
    click(cx, "ai-hotkey");
    assert!(!shown(cx, "ai-hotkey-failed"));
}

/// While the recorder waits, the registered shortcut lets go, so pressing
/// it again records it; it comes back when recording stops.
#[cfg(target_os = "macos")]
#[gpui::test]
fn recording_pauses_the_registered_shortcut(cx: &mut TestAppContext) {
    use crate::companion::hotkey::{self, Recorded};
    use mdoc_store::settings::Hotkey;
    let apps = apps(None, None);
    let (_, _, cx) = page(&apps, true, cx);
    let recorded = cx.update(|_, cx| {
        let recorded = Recorded::install(cx);
        hotkey::set(Some(Hotkey::COMPANION), cx);
        recorded
    });
    click(cx, "ai-hotkey-record");
    assert_eq!(recorded.state().registered, None);
    cx.simulate_keystrokes("escape");
    assert_eq!(recorded.state().registered, Some(Hotkey::COMPANION));
}
