//! Settings › AI connector in the window: what each button shows and
//! writes, against temp Claude Desktop and ChatGPT folders.
use super::super::{Category, CloseSettings, Panel};
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
