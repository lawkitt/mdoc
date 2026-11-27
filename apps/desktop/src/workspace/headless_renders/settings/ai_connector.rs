//! Settings › AI connector (Milestone 2 item 11): the page with the
//! preview off, on, the companion's shortcut in its states (item 15), and
//! the consent card for Add to Claude Desktop and to ChatGPT. The apps' folders are fixtures in the render's own profile,
//! never the real ones.
use crate::ai::connector::setup::{Host, Places, SHIM};
use crate::settings::Category;
use crate::workspace::headless_renders::{Driver, Result};

pub(in crate::workspace::headless_renders) fn scenes(d: &mut Driver) -> Result<()> {
    let root = mdoc_store::app_paths::data_local_dir()
        .ok_or("missing data directory")?
        .join("ai-connector-fixture");
    let write = |path: std::path::PathBuf, text: &str| -> Result<()> {
        std::fs::create_dir_all(path.parent().ok_or("fixture folder")?)
            .map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    };
    // Both apps with another server already set up.
    write(
        root.join("Roaming/Claude/claude_desktop_config.json"),
        "{\n  \"mcpServers\": {\n    \"filesystem\": {\"command\": \"npx\", \"args\": [\"-y\", \
         \"@modelcontextprotocol/server-filesystem\"]}\n  }\n}\n",
    )?;
    write(
        root.join("User/.codex/config.toml"),
        "model = \"gpt\"\n\n[mcp_servers.docs]\ncommand = \"docs-server\"\n",
    )?;
    let shim = root.join("mdoc").join(SHIM);
    write(shim.clone(), "shim")?;
    let places = Places {
        config_dir: Some(root.join("Roaming")),
        local_dir: None,
        home: Some(root.join("User")),
        shim: Some(shim),
        dev_profile: None,
    };
    d.change(|w, window, cx| {
        w.settings.update(cx, |p, cx| {
            p.show_category(Category::AiConnector, window, cx);
            p.render_draft(|p| p.ai_connector.preview = false, cx);
            p.use_ai_places(places.clone(), cx);
        });
    })?;
    d.capture("settings-ai-connector-off")?;
    d.change(|w, _, cx| {
        w.settings.update(cx, |p, cx| {
            p.render_draft(|p| p.ai_connector.preview = true, cx);
            p.use_ai_places(places.clone(), cx);
        });
    })?;
    d.capture("settings-ai-connector")?;
    // The companion's shortcut (item 15): recording, a key that isn't
    // one, a shortcut another app holds, and off. Windows has no row yet,
    // and its renders show the page without it.
    hotkey_scenes(d)?;
    // A start that failed (#1220): another mdoc holds the name.
    d.change(|w, _, cx| {
        w.settings.update(cx, |p, cx| {
            let held = std::io::Error::from(std::io::ErrorKind::AddrInUse);
            p.ai_connector_failed(Some(crate::ai::connector::failed_start(&held)), cx)
        });
    })?;
    d.capture("settings-ai-connector-failed")?;
    d.change(|w, _, cx| {
        w.settings
            .update(cx, |p, cx| p.ai_connector_failed(None, cx))
    })?;
    for (host, name) in [
        (Host::Claude, "settings-ai-connector-consent"),
        (Host::ChatGpt, "settings-ai-connector-consent-chatgpt"),
    ] {
        d.change(|w, window, cx| {
            w.settings
                .update(cx, |p, cx| p.ask_ai_change(host, false, window, cx));
        })?;
        d.settle()?;
        if d.change(|w, _, cx| w.settings.read(cx).ai_consent().is_none())? {
            return Err(format!("Add to {} showed no consent card", host.name()));
        }
        d.capture(name)?;
        d.change(|w, window, cx| {
            w.settings
                .update(cx, |p, cx| p.cancel_ai_change(window, cx));
        })?;
    }
    d.change(|w, window, cx| {
        w.settings.update(cx, |p, cx| {
            p.render_draft(|_| {}, cx);
            p.close(window, cx);
        });
    })?;
    Ok(())
}

fn hotkey_scenes(d: &mut Driver) -> Result<()> {
    use crate::ai::companion::hotkey::Refused;
    use mdoc_store::settings::Hotkey;
    d.change(|w, _, cx| w.settings.update(cx, |p, cx| p.record_hotkey(cx)))?;
    d.capture("settings-ai-hotkey-recording")?;
    d.change(|w, _, cx| {
        let typed = gpui::Keystroke::parse("shift-a").map_err(|e| e.to_string());
        w.settings.update(cx, |p, cx| {
            typed.map(|typed| p.hotkey_keystroke(&typed, cx))
        })
    })??;
    d.capture("settings-ai-hotkey-rejected")?;
    d.change(|w, _, cx| {
        w.settings.update(cx, |p, cx| {
            p.record_hotkey(cx);
            p.hotkey_failed(Some(Refused::Taken.message(Hotkey::COMPANION)), cx)
        })
    })?;
    d.capture("settings-ai-hotkey-failed")?;
    d.change(|w, _, cx| {
        w.settings.update(cx, |p, cx| {
            p.hotkey_failed(None, cx);
            p.render_draft(
                |p| {
                    p.ai_connector.preview = true;
                    p.ai_connector.hotkey_off = true;
                },
                cx,
            )
        })
    })?;
    d.capture("settings-ai-hotkey-off")?;
    d.change(|w, _, cx| {
        w.settings.update(cx, |p, cx| {
            p.render_draft(|p| p.ai_connector.preview = true, cx)
        })
    })?;
    Ok(())
}
