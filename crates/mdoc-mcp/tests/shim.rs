//! The protocol seam (#1164 Testing Decisions; Milestone 2 item 7): a test
//! client drives the built `mdoc-mcp` over stdio, in both protocol eras,
//! against a fake app. The fake app is this test's own listener at the dev
//! profile's endpoint, serving `mdoc_connector::serve` over a real connector
//! on a temp Vault with an injected key, which is what mdoc's listener
//! serves. The shim runs from a copy in a temp folder, where it finds no
//! mdoc to start.
//!
//! Done when: no response carries an original or a real name, and a
//! delivery naming a document the app was not served is refused.
//!
//! A release shim on macOS and Windows takes no dev profile, so it would
//! reach the user's own mdoc: these tests run in debug builds there.
#![cfg(any(debug_assertions, not(any(target_os = "macos", target_os = "windows"))))]
use mdoc_connector::Connector;
use mdoc_mcp::ipc::{self, Endpoint, Listening};
use mdoc_pii::{Category, restore::ExportedIdentity};
use mdoc_store::settings::AiConnectorConfig;
use mdoc_vault::{DocumentOrigin, NewDocument, NewProject, SourceKind, Vault};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

/// Every name the fixture knows, as written.
const ORIGINALS: [&str; 4] = ["Anna", "Petrova", "Boris", "Ivanov"];

fn person(alias: &str, original: &str) -> ExportedIdentity {
    ExportedIdentity {
        alias: alias.into(),
        previous: Vec::new(),
        category: Category::Person,
        custom: false,
        originals: vec![Arc::from(original)],
    }
}

/// The fake app: a Vault with a shared Project and its documents, and a
/// listener serving its connector at the profile's endpoint.
struct App {
    vault: Arc<Vault>,
    /// A Ready letter of the shared Project, titled with a real name.
    letter: String,
    /// A Ready memo of the shared Project the AI app only lists.
    memo: String,
    /// A document of the shared Project that writes the name out.
    draft: String,
    /// A Ready document of another Project.
    elsewhere: String,
    /// A Ready document of the shared Project over 60,000 characters,
    /// titled with a real name, with headings naming aliases.
    long: String,
    _listening: Listening,
}

impl App {
    fn start(profile: &Path, vault_dir: &Path) -> Self {
        let vault = Arc::new(Vault::open(vault_dir.join("vault"), &[9; 32]).unwrap());
        let project = |name: &str, alias: &str, original: &str| {
            let project = (vault.create_project(NewProject {
                name: name.into(),
                ..NewProject::default()
            }))
            .unwrap();
            (vault.update_project(&project.id, |p| p.mapping.merge(&[person(alias, original)])))
                .unwrap()
                .id
        };
        let document = |project: &str, title: &str, markdown: &str, ready: bool| {
            let id = (vault.create_document(NewDocument {
                project: Some(project.into()),
                title: title.into(),
                source_kind: SourceKind::Text,
                origin: DocumentOrigin::Ordinary,
                markdown: markdown.into(),
            }))
            .unwrap()
            .id;
            if ready {
                (vault.update_document(&id, |d| {
                    d.scanned = true;
                    d.set_ready(true);
                }))
                .unwrap();
            }
            id
        };
        let shared = project("Petrova v. Ivanov", "PERSON_1", "Anna Petrova");
        let other = project("Ivanov estate", "PERSON_1", "Boris Ivanov");
        let letter = document(
            &shared,
            "Anna Petrova's letter",
            "# Lease\n\nPERSON_1 signed.",
            true,
        );
        let memo = document(&shared, "Memo to Anna Petrova", "PERSON_1 paid.", true);
        let draft = document(&shared, "Draft", "Anna Petrova called.", false);
        let elsewhere = document(&other, "Boris Ivanov", "PERSON_1 wrote.", true);
        let mut text = String::from(
            "# PERSON_1's lease

",
        );
        for section in 1..=40 {
            text.push_str(&format!(
                "## Term {section} for PERSON_1

"
            ));
            text.push_str(&"PERSON_1 pays the rent on time. ".repeat(60));
            text.push_str(
                "

",
            );
        }
        let long = document(&shared, "Anna Petrova's long lease", &text, true);
        let setting = AiConnectorConfig {
            preview: true,
            ..Default::default()
        };
        let connector = Arc::new(Connector::start(vault.clone(), &setting).unwrap());
        connector.share(&shared).unwrap();
        let endpoint = Endpoint::for_profile(Some(profile)).unwrap();
        let serve = connector.clone();
        let handler: ipc::Handler =
            Arc::new(move |line: &[u8]| mdoc_connector::serve::answer_line(&serve, line));
        let listening = ipc::listen(&endpoint, handler).unwrap();
        Self {
            vault,
            letter,
            memo,
            draft,
            elsewhere,
            long,
            _listening: listening,
        }
    }
}

/// The era a client speaks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Era {
    Legacy,
    Modern,
}

/// The shim under test, and every line it wrote.
struct Shim {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    era: Era,
    next: u64,
    seen: Vec<String>,
}

impl Shim {
    /// A copy of the built shim in `folder`, run with `profile`.
    fn start(folder: &Path, profile: &Path, era: Era) -> Self {
        let name = Path::new(env!("CARGO_BIN_EXE_mdoc-mcp"))
            .file_name()
            .unwrap();
        let copy = folder.join(name);
        std::fs::copy(env!("CARGO_BIN_EXE_mdoc-mcp"), &copy).unwrap();
        let mut child = Command::new(&copy)
            .env(ipc::DEV_PROFILE_VAR, profile)
            // Linux builds take no dev profile: the same folder by its
            // data home.
            .env("XDG_DATA_HOME", profile.join("data"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            era,
            next: 0,
            seen: Vec::new(),
        }
    }

    fn write(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Send `method` with `params` in this era: the response.
    fn request(&mut self, method: &str, mut params: Value) -> Value {
        self.next += 1;
        if self.era == Era::Modern {
            params["_meta"] = json!({
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientInfo": {"name": "test-client", "version": "1"},
                "io.modelcontextprotocol/clientCapabilities": {},
            });
        }
        let id = self.next;
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        assert!(line.ends_with('\n'), "one message per line: {line:?}");
        self.seen.push(line.clone());
        let response: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["id"], id);
        response
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let response = self.request("tools/call", json!({"name": tool, "arguments": arguments}));
        response["result"].clone()
    }

    /// Open the session as this era does.
    fn open(&mut self) {
        match self.era {
            Era::Legacy => {
                let init = self.request(
                    "initialize",
                    json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test-client", "version": "1"}}),
                );
                assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
                assert!(
                    init["result"]["instructions"]
                        .as_str()
                        .unwrap()
                        .contains("deliver_reply")
                );
                self.write(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
            }
            Era::Modern => {
                let found = self.request("server/discover", json!({}));
                assert_eq!(found["result"]["resultType"], "complete");
                assert_eq!(found["result"]["supportedVersions"][0], "2026-07-28");
                assert!(
                    found["result"]["instructions"]
                        .as_str()
                        .unwrap()
                        .contains("deliver_reply")
                );
            }
        }
    }

    /// No line the shim wrote holds a name as written.
    fn assert_no_original(&self) {
        for line in &self.seen {
            for original in ORIGINALS {
                assert!(!line.contains(original), "{original} in {line}");
            }
        }
    }
}

impl Drop for Shim {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn text(result: &Value) -> String {
    (result["content"].as_array().unwrap().iter())
        .map(|block| block["text"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

fn deliver(read: &[&str]) -> Value {
    json!({"kind": "reply", "markdown": "PERSON_1 must sign by Friday.", "read": read})
}

/// Everything a model does, in `era`.
fn session(era: Era) {
    let temp = tempfile::tempdir().unwrap();
    let (profile, folder) = (temp.path().join("profile"), temp.path().join("shim"));
    std::fs::create_dir_all(&folder).unwrap();
    let app = App::start(&profile, temp.path());
    let mut shim = Shim::start(&folder, &profile, era);
    shim.open();

    let tools = shim.request("tools/list", json!({}));
    let names: Vec<&str> = (tools["result"]["tools"].as_array().unwrap().iter())
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["list_documents", "get_document", "deliver_reply"]);
    let prompts = shim.request("prompts/list", json!({}));
    assert_eq!(prompts["result"]["prompts"][0]["name"], "restore");
    let restore = shim.request("prompts/get", json!({"name": "restore"}));
    assert!(restore["result"]["messages"][0]["content"]["text"].is_string());

    // Listed by label and state: three of the shared Project, none of the other's.
    let listed = shim.call("list_documents", json!({}));
    let rows = listed["structuredContent"]["documents"]
        .as_array()
        .unwrap()
        .clone();
    let row = |id: &str| rows.iter().find(|row| row["doc_id"] == id).cloned();
    assert_eq!(rows.len(), 4);
    assert_eq!(row(&app.letter).unwrap()["label"], "Lease");
    assert_eq!(row(&app.memo).unwrap()["state"], "ready");
    assert_eq!(row(&app.draft).unwrap()["state"], "needs review");
    assert!(row(&app.elsewhere).is_none());

    // A Ready read hands out aliases; one that needs review, nothing; one
    // of another Project is refused.
    let letter = shim.call("get_document", json!({"doc_id": app.letter}));
    assert_eq!(letter["isError"], false);
    assert!(text(&letter).contains("PERSON_1 signed."));
    let draft = shim.call("get_document", json!({"doc_id": app.draft}));
    assert_eq!(draft["isError"], false);
    assert!(text(&draft).contains("needs review"), "{draft}");
    // A long one is an outline, then its parts.
    let outline = shim.call("get_document", json!({"doc_id": app.long}));
    let outline = text(&outline);
    assert!(outline.contains("is too long to read at once"), "{outline}");
    assert!(outline.contains("Part 2 of"), "{outline}");
    assert!(outline.contains("Term 1 for PERSON_1"), "{outline}");
    let part = shim.call("get_document", json!({"doc_id": app.long, "part": 2}));
    assert_eq!(part["isError"], false);
    assert!(text(&part).starts_with("Part 2 of"), "{part}");
    assert!(text(&part).contains("PERSON_1 pays the rent"));
    let elsewhere = shim.call("get_document", json!({"doc_id": app.elsewhere}));
    assert_eq!(elsewhere["isError"], true);

    // A delivery naming a document not served to this app is refused, and
    // saves nothing: only listed, needing review, another Project's.
    for unserved in [&app.memo, &app.draft, &app.elsewhere] {
        let refused = shim.call("deliver_reply", deliver(&[&app.letter, unserved]));
        assert_eq!(refused["isError"], true, "{refused}");
        assert!(
            text(&refused).contains("Name the documents you read"),
            "{refused}"
        );
    }
    assert!(app.vault.replies().is_empty());

    // One naming what was read lands, restored in mdoc only.
    let delivered = shim.call("deliver_reply", deliver(&[&app.letter]));
    assert_eq!(delivered["isError"], false, "{delivered}");
    assert!(text(&delivered).contains("restored 1 name"), "{delivered}");
    let replies = app.vault.replies();
    assert_eq!(replies.len(), 1);
    let restored = app.vault.restore_reply(&replies[0].id).unwrap();
    assert!(format!("{restored:?}").contains("Anna Petrova"));

    shim.assert_no_original();
}

#[test]
fn a_legacy_client_reads_and_delivers_and_sees_no_original() {
    session(Era::Legacy);
}

#[test]
fn a_modern_client_reads_and_delivers_and_sees_no_original() {
    session(Era::Modern);
}

/// Done when: with nothing listening, a call answers at once that mdoc
/// isn't answering, and a shim with no mdoc beside it starts nothing.
#[test]
fn with_no_mdoc_listening_a_call_says_so() {
    let temp = tempfile::tempdir().unwrap();
    let (profile, folder) = (temp.path().join("profile"), temp.path().join("shim"));
    std::fs::create_dir_all(&folder).unwrap();
    let mut shim = Shim::start(&folder, &profile, Era::Modern);
    let started = Instant::now();
    let result = shim.call("list_documents", json!({}));
    assert_eq!(result["isError"], true);
    assert!(text(&result).contains("mdoc isn't answering"), "{result}");
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// Done when: an unknown protocol version is refused with the supported
/// ones, as the modern era asks.
#[test]
fn an_unknown_protocol_version_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let (profile, folder) = (temp.path().join("profile"), temp.path().join("shim"));
    std::fs::create_dir_all(&folder).unwrap();
    let mut shim = Shim::start(&folder, &profile, Era::Legacy);
    let response = shim.request(
        "tools/list",
        json!({"_meta": {"io.modelcontextprotocol/protocolVersion": "1900-01-01"}}),
    );
    assert_eq!(response["error"]["code"], -32022);
    assert_eq!(response["error"]["data"]["supported"][0], "2026-07-28");
}

/// Done when (#1311): from a profile whose path is too long for the usual
/// socket, the shim reaches mdoc's listener by the same rule.
#[test]
fn a_long_profile_path_is_reached() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("p".repeat(110)).join("profile");
    let folder = temp.path().join("shim");
    std::fs::create_dir_all(&folder).unwrap();
    // On macOS and Linux, a socket in a short folder outside the temp one.
    #[cfg(unix)]
    let socket = Endpoint::for_profile(Some(&profile)).unwrap().to_string();
    #[cfg(unix)]
    assert!(!Path::new(&socket).starts_with(temp.path()), "{socket}");
    let app = App::start(&profile, temp.path());
    let mut shim = Shim::start(&folder, &profile, Era::Modern);
    let listed = shim.call("list_documents", json!({}));
    assert_eq!(listed["isError"], false, "{listed}");
    let rows = listed["structuredContent"]["documents"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    drop((shim, app));
    // The short folder, outside the temp folder: leave nothing there.
    #[cfg(unix)]
    let _ = std::fs::remove_dir_all(Path::new(&socket).parent().unwrap());
}
