use super::*;

/// A temp home with Claude Desktop's and ChatGPT's folders and a built
/// shim.
struct Home {
    dir: tempfile::TempDir,
    places: Places,
}

fn fresh() -> Home {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("Roaming");
    std::fs::create_dir_all(config_dir.join("Claude")).unwrap();
    std::fs::create_dir_all(dir.path().join("User").join(".codex")).unwrap();
    let shim = dir.path().join("mdoc").join(SHIM);
    std::fs::create_dir_all(shim.parent().unwrap()).unwrap();
    std::fs::write(&shim, b"shim").unwrap();
    let places = Places {
        config_dir: Some(config_dir),
        local_dir: Some(dir.path().join("Local")),
        home: Some(dir.path().join("User")),
        shim: Some(shim),
        dev_profile: None,
    };
    Home { dir, places }
}

fn with(host: Host, text: &str) -> Home {
    let home = fresh();
    home.write(host, text);
    home
}

impl Home {
    fn config(&self, host: Host) -> PathBuf {
        self.places.config(host).unwrap()
    }
    fn write(&self, host: Host, text: &str) {
        std::fs::write(self.config(host), text).unwrap();
    }
    fn text(&self, host: Host) -> String {
        std::fs::read_to_string(self.config(host)).unwrap()
    }
    fn json(&self) -> Value {
        serde_json::from_str(&self.text(Host::Claude)).unwrap()
    }
    fn command(&self) -> String {
        (self.places.shim.as_ref().unwrap().to_str().unwrap()).to_owned()
    }
    fn entry(&self) -> Value {
        json!({"command": self.command(), "args": []})
    }
}

/// A Claude config with another server and settings of Claude's own, in
/// an order that isn't alphabetical.
const CLAUDE: &str = r#"{
  "theme": "dark",
  "mcpServers": {
    "zeta": {"command": "zeta-server", "args": ["--port", "7"]},
    "filesystem": {"command": "npx", "args": ["-y", "server-filesystem", "/Users/a"], "env": {"A": "1"}}
  },
  "globalShortcut": "Alt+Space",
  "nested": {"list": [1, 2.5, "three", null, true], "z": {}, "a": []}
}
"#;

/// A Codex config with comments, settings of its own and another server.
const CODEX: &str = r#"# Codex settings, by hand.
model = "gpt-6.2"   # the default
approval_policy = "on-request"

[mcp_servers.docs]
command = "docs-server"
args = ["--port", "8"]   # keep this
env = { TOKEN_FILE = "/x" }

[profiles.work]
model = "o9"
"#;

/// Done when: Add writes exactly the shown entry, pointing at the
/// installed shim, under `mcpServers`, and everything else stays, by
/// value and in order, with the old file kept as the backup.
#[test]
fn claude_add_writes_the_shown_entry_and_keeps_everything_else() {
    let home = with(Host::Claude, CLAUDE);
    assert_eq!(status(&home.places, Host::Claude), Status::NotAdded);
    let change = plan_add(&home.places, Host::Claude).unwrap();
    assert_eq!(change.kind, Kind::Add);
    assert_eq!(change.path, home.config(Host::Claude));
    assert_eq!(change.entry, home.entry());
    assert_eq!(
        change.shown(),
        serde_json::to_string_pretty(&json!({"mcpServers": {"mdoc": home.entry()}})).unwrap()
    );
    // Planning writes nothing.
    assert_eq!(home.text(Host::Claude), CLAUDE);
    apply(&change).unwrap();
    assert_eq!(
        std::fs::read(home.config(Host::Claude)).unwrap(),
        change.written()
    );
    // The file as it was stays beside it.
    let kept = backup(&home.config(Host::Claude));
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), CLAUDE);
    assert_eq!(
        kept.file_name().unwrap(),
        "claude_desktop_config.json.mdoc-backup"
    );
    let mut expected: Value = serde_json::from_str(CLAUDE).unwrap();
    expected["mcpServers"]["mdoc"] = home.entry();
    let written = home.json();
    assert_eq!(written, expected);
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(
        keys(&written),
        ["theme", "mcpServers", "globalShortcut", "nested"]
    );
    assert_eq!(keys(&written["mcpServers"]), ["zeta", "filesystem", "mdoc"]);
    assert_eq!(keys(&written["nested"]), ["list", "z", "a"]);
    assert_eq!(status(&home.places, Host::Claude), Status::Added);
    assert_eq!(
        plan_add(&home.places, Host::Claude),
        Err(Refusal::AlreadyAdded)
    );
}

/// Done when: ChatGPT's Add appends exactly the shown
/// `[mcp_servers.mdoc]` table and leaves every other line, comments
/// included, as it was.
#[test]
fn chatgpt_add_writes_the_shown_table_and_keeps_every_other_line() {
    let home = with(Host::ChatGpt, CODEX);
    assert_eq!(status(&home.places, Host::ChatGpt), Status::NotAdded);
    let change = plan_add(&home.places, Host::ChatGpt).unwrap();
    assert_eq!(change.path, home.dir.path().join("User/.codex/config.toml"));
    assert_eq!(change.entry, home.entry());
    let table = format!(
        "[mcp_servers.mdoc]\ncommand = {}\nargs = []",
        toml_edit::Value::from(home.command().as_str())
    );
    assert_eq!(change.shown(), table);
    assert_eq!(home.text(Host::ChatGpt), CODEX, "planning writes nothing");
    apply(&change).unwrap();
    let written = home.text(Host::ChatGpt);
    assert_eq!(written.as_bytes(), change.written());
    // The new table joins the other server's, and no other line changes.
    assert_eq!(
        written,
        CODEX.replace(
            "\n[profiles.work]",
            &format!("\n{table}\n\n[profiles.work]")
        )
    );
    // Codex reads it as a server like the other.
    let doc: toml_edit::DocumentMut = written.parse().unwrap();
    assert_eq!(
        doc["mcp_servers"]["mdoc"]["command"].as_str(),
        Some(home.command().as_str())
    );
    assert_eq!(
        doc["mcp_servers"]["docs"]["command"].as_str(),
        Some("docs-server")
    );
    assert_eq!(
        std::fs::read_to_string(backup(&home.config(Host::ChatGpt))).unwrap(),
        CODEX
    );
    assert_eq!(status(&home.places, Host::ChatGpt), Status::Added);
}

/// An app installed but never configured: Add makes the file with the
/// entry alone, and there is no backup.
#[test]
fn add_makes_the_file_when_there_is_none() {
    for host in Host::ALL {
        let home = fresh();
        assert!(!home.config(host).exists());
        assert_eq!(status(&home.places, host), Status::NotAdded);
        apply(&plan_add(&home.places, host).unwrap()).unwrap();
        assert_eq!(status(&home.places, host), Status::Added, "{host:?}");
        assert!(!backup(&home.config(host)).exists());
        // An empty file holds nothing either.
        let home = with(host, "  \n");
        apply(&plan_add(&home.places, host).unwrap()).unwrap();
        assert_eq!(status(&home.places, host), Status::Added, "{host:?}");
    }
    let home = fresh();
    apply(&plan_add(&home.places, Host::Claude).unwrap()).unwrap();
    assert_eq!(home.json(), json!({"mcpServers": {"mdoc": home.entry()}}));
}

/// Done when: Remove takes out mdoc's entry alone, and the next change
/// replaces the backup.
#[test]
fn remove_takes_out_only_mdocs_entry() {
    for (host, original) in [(Host::Claude, CLAUDE), (Host::ChatGpt, CODEX)] {
        let home = with(host, original);
        assert_eq!(plan_remove(&home.places, host), Err(Refusal::NotAdded));
        apply(&plan_add(&home.places, host).unwrap()).unwrap();
        let added = std::fs::read(home.config(host)).unwrap();
        let change = plan_remove(&home.places, host).unwrap();
        assert_eq!(change.kind, Kind::Remove);
        assert_eq!(change.entry, home.entry());
        apply(&change).unwrap();
        assert_eq!(status(&home.places, host), Status::NotAdded);
        assert_eq!(std::fs::read(backup(&home.config(host))).unwrap(), added);
        match host {
            Host::Claude => assert_eq!(
                home.json(),
                serde_json::from_str::<Value>(original).unwrap()
            ),
            // Every line of the file as it was, comments included.
            Host::ChatGpt => assert_eq!(home.text(host).trim_end(), original.trim_end()),
        }
    }
    // The last server out leaves an empty mcpServers.
    let home = fresh();
    apply(&plan_add(&home.places, Host::Claude).unwrap()).unwrap();
    apply(&plan_remove(&home.places, Host::Claude).unwrap()).unwrap();
    assert_eq!(home.json(), json!({"mcpServers": {}}));
}

/// Another copy's entry is shown and replaced, never merged.
#[test]
fn another_mdoc_entry_is_shown_and_replaced() {
    let old = json!({"command": "/old/mdoc-mcp", "args": ["x"], "env": {"K": "v"}});
    let home = with(
        Host::Claude,
        &serde_json::to_string(&json!({"mcpServers": {"mdoc": old, "b": {"command": "b"}}}))
            .unwrap(),
    );
    assert_eq!(status(&home.places, Host::Claude), Status::Other);
    let change = plan_add(&home.places, Host::Claude).unwrap();
    assert_eq!(change.kind, Kind::Replace { old: old.clone() });
    assert_eq!(change.replaced(), Some(shown(Host::Claude, &old)));
    apply(&change).unwrap();
    assert_eq!(
        home.json(),
        json!({"mcpServers": {"mdoc": home.entry(), "b": {"command": "b"}}})
    );
    let home = with(
        Host::ChatGpt,
        "[mcp_servers.mdoc]\ncommand = \"/old/mdoc-mcp\"\nargs = [\"x\"]\nenv = { K = \"v\" }\n",
    );
    assert_eq!(status(&home.places, Host::ChatGpt), Status::Other);
    let change = plan_add(&home.places, Host::ChatGpt).unwrap();
    assert_eq!(change.kind, Kind::Replace { old });
    apply(&change).unwrap();
    assert_eq!(status(&home.places, Host::ChatGpt), Status::Added);
    assert!(!home.text(Host::ChatGpt).contains("/old/"));
}

/// Done when: a file mdoc can't parse, or of another shape, is refused
/// by every button, and left byte for byte, with no backup.
#[test]
fn a_file_mdoc_cannot_parse_is_refused_and_left_alone() {
    for (host, text, refusal) in [
        (Host::Claude, "{\"mcpServers\": {", Refusal::Unparsable),
        (
            Host::Claude,
            "{\"mcpServers\": {}, // a comment\n}",
            Refusal::Unparsable,
        ),
        (
            Host::Claude,
            "\u{feff}{\"mcpServers\": {}}",
            Refusal::Unparsable,
        ),
        (Host::Claude, "[1, 2]", Refusal::Unexpected),
        (Host::Claude, "\"text\"", Refusal::Unexpected),
        (
            Host::Claude,
            "{\"mcpServers\": [\"mdoc\"]}",
            Refusal::Unexpected,
        ),
        (Host::Claude, "{\"mcpServers\": null}", Refusal::Unexpected),
        (Host::ChatGpt, "model = \"gpt\n", Refusal::Unparsable),
        (Host::ChatGpt, "[mcp_servers\n", Refusal::Unparsable),
        (
            Host::ChatGpt,
            "[a]\nx = 1\n[a]\nx = 2\n",
            Refusal::Unparsable,
        ),
        (Host::ChatGpt, "mcp_servers = 3\n", Refusal::Unexpected),
        (
            Host::ChatGpt,
            "mcp_servers = { docs = { command = \"d\" } }\n",
            Refusal::Unexpected,
        ),
        (
            Host::ChatGpt,
            "[mcp_servers]\nmdoc = \"text\"\n",
            Refusal::Unexpected,
        ),
    ] {
        let home = with(host, text);
        assert_eq!(
            status(&home.places, host),
            Status::Unreadable(refusal.clone()),
            "{text}"
        );
        assert_eq!(plan_add(&home.places, host), Err(refusal.clone()), "{text}");
        assert_eq!(plan_remove(&home.places, host), Err(refusal), "{text}");
        assert_eq!(home.text(host), text);
        assert!(!backup(&home.config(host)).exists());
    }
}

/// The yes covers what the user saw: a file changed after planning (the
/// app saved it, say) is not written.
#[test]
fn a_file_changed_after_the_card_is_not_written() {
    for (host, original) in [(Host::Claude, CLAUDE), (Host::ChatGpt, CODEX)] {
        let home = with(host, original);
        let change = plan_add(&home.places, host).unwrap();
        let edited = original.replace("dark", "light").replace("o9", "o10");
        home.write(host, &edited);
        assert_eq!(apply(&change), Err(Refusal::Changed));
        assert_eq!(home.text(host), edited);
        assert!(!backup(&home.config(host)).exists());
        // A file that appeared after a plan made for none, too.
        let home = fresh();
        let change = plan_add(&home.places, host).unwrap();
        home.write(host, "\n");
        assert_eq!(apply(&change), Err(Refusal::Changed));
        assert_eq!(home.text(host), "\n");
    }
}

/// No app folder, no shim, or an app on its disk image: no plan, and
/// nothing is made.
#[test]
fn nothing_is_planned_without_the_app_or_an_installed_shim() {
    let home = fresh();
    let mut places = home.places.clone();
    places.config_dir = Some(home.dir.path().join("Elsewhere"));
    places.home = Some(home.dir.path().join("Elsewhere"));
    for host in Host::ALL {
        assert_eq!(status(&places, host), Status::NotInstalled);
        assert_eq!(plan_add(&places, host), Err(Refusal::NotInstalled));
        assert_eq!(plan_remove(&places, host), Err(Refusal::NotInstalled));
    }
    assert!(!home.dir.path().join("Elsewhere").exists());
    let mut places = home.places.clone();
    places.shim = None;
    assert_eq!(plan_add(&places, Host::Claude), Err(Refusal::NoShim));
    for path in [
        "/Volumes/mdoc/mdoc.app/Contents/MacOS/mdoc-mcp",
        "/private/var/folders/x/AppTranslocation/1/d/mdoc.app/Contents/MacOS/mdoc-mcp",
    ] {
        places.shim = Some(PathBuf::from(path));
        assert_eq!(
            plan_add(&places, Host::ChatGpt),
            Err(Refusal::NotInPlace),
            "{path}"
        );
    }
    for host in Host::ALL {
        assert!(!home.config(host).exists());
    }
}

/// A Microsoft Store Claude reads its virtualised copy: that one is
/// written, and the roaming one is left alone.
#[test]
fn a_store_claude_gets_its_own_copy() {
    let home = fresh();
    let store =
        (home.dir.path()).join("Local/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude");
    let roaming = home
        .dir
        .path()
        .join("Roaming/Claude/claude_desktop_config.json");
    // A package folder without Claude's own folder isn't used.
    std::fs::create_dir_all(store.parent().unwrap()).unwrap();
    assert_eq!(home.config(Host::Claude), roaming);
    std::fs::create_dir_all(&store).unwrap();
    assert_eq!(
        home.config(Host::Claude),
        store.join("claude_desktop_config.json")
    );
    apply(&plan_add(&home.places, Host::Claude).unwrap()).unwrap();
    assert!(store.join("claude_desktop_config.json").is_file());
    assert!(!roaming.exists());
}

/// A debug build's dev profile goes with the entry, so the AI app's
/// shim reaches the same profile; without one there is no `env`.
#[test]
fn a_dev_profile_goes_with_the_entry() {
    let home = fresh();
    let mut places = home.places.clone();
    let profile = home.dir.path().join("profile");
    places.dev_profile = Some(profile.clone());
    let var = mdoc_store::app_paths::DEV_PROFILE_VAR;
    let change = plan_add(&places, Host::Claude).unwrap();
    assert_eq!(change.entry["env"][var], profile.to_str().unwrap());
    apply(&plan_add(&places, Host::ChatGpt).unwrap()).unwrap();
    let doc: toml_edit::DocumentMut = home.text(Host::ChatGpt).parse().unwrap();
    assert_eq!(
        doc["mcp_servers"]["mdoc"]["env"][var].as_str(),
        profile.to_str()
    );
    assert_eq!(status(&places, Host::ChatGpt), Status::Added);
    let plain = plan_add(&home.places, Host::Claude).unwrap();
    assert!(plain.entry.get("env").is_none());
}

/// Test connection opens a new Claude chat with the first prompt filled
/// in, percent-encoded (item 1's spike): byte for byte the link it opened
/// before it used `crate::ai::links` (encoding is tested there).
#[test]
fn test_connection_prefills_a_new_claude_chat() {
    assert_eq!(
        claude_test_link(),
        "claude://claude.ai/new?q=List%20my%20documents%20in%20mdoc."
    );
}

/// A config that is a link (into a dotfiles repository, say) is refused:
/// replacing it would break the link.
#[test]
fn a_linked_config_is_refused() {
    let home = fresh();
    let real = home.dir.path().join("dotfiles-config.toml");
    std::fs::write(&real, CODEX).unwrap();
    let link = home.dir.path().join("User/.codex/config.toml");
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&real, &link);
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(&real, &link);
    if made.is_err() {
        // Windows without Developer Mode can't make one; Linux and macOS run it.
        eprintln!("skipped: this account can't make a symbolic link");
        return;
    }
    assert_eq!(
        status(&home.places, Host::ChatGpt),
        Status::Unreadable(Refusal::Linked)
    );
    assert_eq!(plan_add(&home.places, Host::ChatGpt), Err(Refusal::Linked));
    assert_eq!(
        plan_remove(&home.places, Host::ChatGpt),
        Err(Refusal::Linked)
    );
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_to_string(&real).unwrap(), CODEX);
}

/// A file planned unlinked but linked by the yes is not replaced either.
#[test]
fn a_config_linked_after_the_card_is_not_replaced() {
    let home = with(Host::ChatGpt, CODEX);
    let change = plan_add(&home.places, Host::ChatGpt).unwrap();
    let real = home.dir.path().join("dotfiles-config.toml");
    std::fs::write(&real, CODEX).unwrap();
    let link = home.config(Host::ChatGpt);
    std::fs::remove_file(&link).unwrap();
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&real, &link);
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(&real, &link);
    if made.is_err() {
        eprintln!("skipped: this account can't make a symbolic link");
        return;
    }
    assert_eq!(apply(&change), Err(Refusal::Linked));
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

/// A Windows config with CRLF lines keeps them; only mdoc's table is new.
#[test]
fn a_crlf_codex_config_keeps_its_line_ends() {
    let original = CODEX.replace('\n', "\r\n");
    let home = with(Host::ChatGpt, &original);
    apply(&plan_add(&home.places, Host::ChatGpt).unwrap()).unwrap();
    let written = home.text(Host::ChatGpt);
    assert!(!written.replace("\r\n", "").contains('\n'), "{written:?}");
    assert!(written.starts_with(&original[..original.find("[profiles").unwrap()]));
    apply(&plan_remove(&home.places, Host::ChatGpt).unwrap()).unwrap();
    assert_eq!(home.text(Host::ChatGpt).trim_end(), original.trim_end());
}

/// Update keeps the comments above mdoc's table and its place; Remove
/// shows those comments on the card, since they go with it.
#[test]
fn comments_above_mdocs_table_are_kept_or_shown() {
    let text = "model = \"gpt\"\n\n# mdoc, added by hand\n[mcp_servers.mdoc]\ncommand = \"/old/mdoc-mcp\"\n\n[mcp_servers.docs]\ncommand = \"docs\"\n";
    let home = with(Host::ChatGpt, text);
    let change = plan_remove(&home.places, Host::ChatGpt).unwrap();
    assert!(
        change
            .shown()
            .starts_with("# mdoc, added by hand\n[mcp_servers.mdoc]\n"),
        "{}",
        change.shown()
    );
    apply(&plan_add(&home.places, Host::ChatGpt).unwrap()).unwrap();
    let written = home.text(Host::ChatGpt);
    let comment = written
        .find("# mdoc, added by hand\n[mcp_servers.mdoc]\n")
        .unwrap();
    assert!(
        comment < written.find("[mcp_servers.docs]").unwrap(),
        "{written}"
    );
    assert!(!written.contains("/old/"));
}
