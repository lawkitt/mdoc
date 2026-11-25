//! Settings › AI connector (Milestone 2 item 11): the page with the
//! preview off, on, and the consent card for Add to Claude Desktop and to
//! ChatGPT. The apps' folders are fixtures in the render's own profile,
//! never the real ones.
use super::{Driver, Result};
use crate::ai_connector::setup::{Host, Places, SHIM};
use crate::settings_ui::Category;

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
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
            return Err(format!("Add to {} showed no consent card", host.name()).into());
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
