//! Settings › AI connector's setup (Milestone 2 item 11, #1164 Decision
//! 20): mdoc adds its shim to an AI app's own config file, only after
//! showing the user the exact entry and getting a yes, and takes it out
//! again. Two apps:
//! - **Claude Desktop:** `mcpServers.mdoc` in `claude_desktop_config.json`.
//! - **ChatGPT:** `[mcp_servers.mdoc]` in `~/.codex/config.toml`, which the
//!   ChatGPT desktop app shares with Codex; only its Work threads call
//!   local servers (item 2's spike).
//!
//! Nothing here runs on its own: a [`Change`] is planned for the consent
//! card, and [`apply`] writes it only if the file is still what the user
//! saw. Everything else in the file is kept (JSON by value and order, TOML
//! with its comments and layout), and the file as it was stays beside it as
//! `<file>.mdoc-backup` until the next change. A file mdoc can't parse is
//! refused, never replaced.
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

pub(crate) use crate::ai::text::FIRST_PROMPT;

/// The key mdoc adds under the app's servers, as the app lists it.
pub(crate) const SERVER: &str = "mdoc";
/// The backup's suffix: `claude_desktop_config.json.mdoc-backup`, the
/// file before mdoc's last change (the maintainer, on #1091).
const BACKUP: &str = ".mdoc-backup";
/// The shim's file beside mdoc's.
pub(crate) const SHIM: &str = if cfg!(windows) {
    "mdoc-mcp.exe"
} else {
    "mdoc-mcp"
};

/// An AI app mdoc can add itself to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Host {
    Claude,
    ChatGpt,
}

impl Host {
    pub(crate) const ALL: [Self; 2] = [Self::Claude, Self::ChatGpt];

    /// The app's name, as the page says it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Desktop",
            Self::ChatGpt => "ChatGPT",
        }
    }

    /// Where the servers sit in the file, as the consent card says it.
    pub(crate) fn servers(self) -> &'static str {
        match self {
            Self::Claude => "mcpServers",
            Self::ChatGpt => "mcp_servers",
        }
    }

    fn file(self) -> &'static str {
        match self {
            Self::Claude => "claude_desktop_config.json",
            Self::ChatGpt => "config.toml",
        }
    }
}

/// Where the setup looks: this app's own places, or a test's or render's
/// temp folders. Nothing else is ever read or written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Places {
    /// Application Support on macOS, the roaming `%APPDATA%` on Windows.
    pub config_dir: Option<PathBuf>,
    /// `%LOCALAPPDATA%` on Windows, where a Microsoft Store Claude keeps
    /// its own copy of the config.
    pub local_dir: Option<PathBuf>,
    /// The user's home folder (`%USERPROFILE%` on Windows), which holds
    /// ChatGPT's and Codex's `.codex` folder.
    pub home: Option<PathBuf>,
    /// The installed `mdoc-mcp`, beside the running mdoc.
    pub shim: Option<PathBuf>,
    /// A debug build's dev profile, which the shim must reach too.
    pub dev_profile: Option<PathBuf>,
}

impl Places {
    /// This app's places. Never in tests or headless renders, which inject
    /// their own.
    pub(crate) fn of_this_app() -> Self {
        let dev_profile = (mdoc_store::app_paths::DEV_PROFILE_HONOURED)
            .then(mdoc_store::app_paths::dev_profile)
            .flatten()
            .map(Path::to_path_buf);
        Self {
            config_dir: dirs::config_dir(),
            local_dir: if cfg!(windows) {
                dirs::data_local_dir()
            } else {
                None
            },
            home: dirs::home_dir(),
            shim: installed_shim(),
            dev_profile,
        }
    }

    /// The app's folder, when it is installed.
    pub(crate) fn folder(&self, host: Host) -> Option<PathBuf> {
        match host {
            Host::Claude => {
                // A Microsoft Store (MSIX) Claude reads a virtualised copy
                // of its roaming folder, so that one wins when it exists.
                if let Some(folder) = self.local_dir.as_deref().and_then(store_folder) {
                    return Some(folder);
                }
                let folder = self.config_dir.as_ref()?.join("Claude");
                folder.is_dir().then_some(folder)
            }
            Host::ChatGpt => {
                let folder = self.home.as_ref()?.join(".codex");
                folder.is_dir().then_some(folder)
            }
        }
    }

    /// The app's config file, when the app is installed.
    pub(crate) fn config(&self, host: Host) -> Option<PathBuf> {
        self.folder(host).map(|folder| folder.join(host.file()))
    }

    /// The entry for this shim, or why there is none.
    pub(crate) fn entry(&self) -> Result<Value, Refusal> {
        let shim = self.shim.as_deref().ok_or(Refusal::NoShim)?;
        if !runs_in_place(shim) {
            return Err(Refusal::NotInPlace);
        }
        let command = shim.to_str().ok_or(Refusal::NoShim)?;
        let mut entry = Map::new();
        entry.insert("command".into(), command.into());
        entry.insert("args".into(), json!([]));
        if let Some(profile) = &self.dev_profile {
            let profile = profile.to_str().ok_or(Refusal::NoShim)?;
            entry.insert(
                "env".into(),
                json!({ mdoc_store::app_paths::DEV_PROFILE_VAR: profile }),
            );
        }
        Ok(Value::Object(entry))
    }
}

/// The `Claude_*` package's roaming folder under `local`, if a Microsoft
/// Store Claude made one.
fn store_folder(local: &Path) -> Option<PathBuf> {
    let mut packages: Vec<PathBuf> = std::fs::read_dir(local.join("Packages"))
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.len() > 7 && name.is_char_boundary(7) && name[..7].eq_ignore_ascii_case("Claude_")
        })
        .map(|entry| {
            entry
                .path()
                .join("LocalCache")
                .join("Roaming")
                .join("Claude")
        })
        .filter(|folder| folder.is_dir())
        .collect();
    packages.sort();
    packages.into_iter().next()
}

/// The `mdoc-mcp` beside the running mdoc.
fn installed_shim() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = dunce::canonicalize(&exe).unwrap_or(exe);
    let shim = exe.with_file_name(SHIM);
    shim.is_file().then_some(shim)
}

/// A macOS app run from its disk image or from a translocated copy has a
/// path that goes away: the AI app couldn't start the shim later.
fn runs_in_place(shim: &Path) -> bool {
    let path = shim.to_string_lossy();
    !(path.contains("/AppTranslocation/") || path.starts_with("/Volumes/"))
}

/// Why mdoc won't change an AI app's config, or couldn't. Each one writes
/// nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// No folder of the app's.
    NotInstalled,
    /// No `mdoc-mcp` beside mdoc.
    NoShim,
    /// mdoc runs from its disk image or a translocated copy.
    NotInPlace,
    /// The file isn't JSON, or TOML.
    Unparsable,
    /// It parses, but not in the shape mdoc expects.
    Unexpected,
    /// The entry is there already, as mdoc would write it.
    AlreadyAdded,
    /// There is no entry to remove.
    NotAdded,
    /// The file changed after the user saw the change.
    Changed,
    /// The file is a link to another file: replacing it would break the
    /// link.
    Linked,
    /// Reading or writing failed.
    Io { writing: bool, error: String },
}

/// Where an AI app stands with mdoc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    NotInstalled,
    NotAdded,
    /// The entry is there, as mdoc would write it now.
    Added,
    /// An `mdoc` entry is there, but another one (another copy of the
    /// shim, or edited by hand).
    Other,
    /// The file can't be read or parsed: mdoc won't change it.
    Unreadable(Refusal),
}

/// The parsed file.
enum Doc {
    Json(Map<String, Value>),
    Toml(toml_edit::DocumentMut),
}

/// What the file holds now.
struct Snapshot {
    /// Its bytes, `None` when there is no file yet.
    bytes: Option<Vec<u8>>,
    doc: Doc,
    /// The TOML file's lines end in CRLF: the written file keeps them.
    crlf: bool,
}

impl Snapshot {
    fn read(path: &Path, host: Host) -> Result<Self, Refusal> {
        if is_link(path) {
            return Err(Refusal::Linked);
        }
        let bytes = match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(Refusal::Io {
                    writing: false,
                    error: error.to_string(),
                });
            }
        };
        // No file, or an empty one, holds nothing to keep.
        let text = bytes.as_deref().unwrap_or_default();
        let empty = text.iter().all(u8::is_ascii_whitespace);
        let mut crlf = false;
        let doc = match host {
            Host::Claude => Doc::Json(if empty {
                Map::new()
            } else {
                let config = match serde_json::from_slice::<Value>(text) {
                    Ok(Value::Object(config)) => config,
                    Ok(_) => return Err(Refusal::Unexpected),
                    Err(_) => return Err(Refusal::Unparsable),
                };
                match config.get("mcpServers") {
                    None | Some(Value::Object(_)) => config,
                    Some(_) => return Err(Refusal::Unexpected),
                }
            }),
            Host::ChatGpt => {
                let text = std::str::from_utf8(text).map_err(|_| Refusal::Unparsable)?;
                crlf = text.contains("\r\n");
                let doc: toml_edit::DocumentMut = text.parse().map_err(|_| Refusal::Unparsable)?;
                // A table of tables mdoc can add one to, written as
                // `[mcp_servers.<name>]` sections.
                match doc.get("mcp_servers") {
                    None => {}
                    Some(toml_edit::Item::Table(servers)) => {
                        if let Some(entry) = servers.get(SERVER)
                            && !entry.is_table_like()
                        {
                            return Err(Refusal::Unexpected);
                        }
                    }
                    Some(_) => return Err(Refusal::Unexpected),
                }
                Doc::Toml(doc)
            }
        };
        Ok(Self { bytes, doc, crlf })
    }

    /// The comment lines written just above mdoc's own TOML table: they go
    /// with it on Remove, so the card shows them.
    fn entry_comments(&self) -> Option<String> {
        let Doc::Toml(doc) = &self.doc else {
            return None;
        };
        let table = doc.get("mcp_servers")?.get(SERVER)?.as_table()?;
        let prefix = table.decor().prefix()?.as_str()?;
        let comments: Vec<&str> = prefix
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('#'))
            .collect();
        (!comments.is_empty()).then(|| comments.join("\n"))
    }

    /// The `mdoc` entry, as JSON.
    fn entry(&self) -> Option<Value> {
        match &self.doc {
            Doc::Json(config) => config.get("mcpServers")?.get(SERVER).cloned(),
            Doc::Toml(doc) => Some(json_of_item(doc.get("mcp_servers")?.get(SERVER)?)),
        }
    }

    /// The file's bytes with the `mdoc` entry set to `entry`, or taken out.
    fn with(self, entry: Option<&Value>) -> Vec<u8> {
        match self.doc {
            Doc::Json(mut config) => {
                let servers = config
                    .entry("mcpServers")
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Value::Object(servers) = servers {
                    match entry {
                        Some(entry) => {
                            servers.insert(SERVER.into(), entry.clone());
                        }
                        None => {
                            servers.shift_remove(SERVER);
                        }
                    }
                }
                let mut bytes = serde_json::to_vec_pretty(&Value::Object(config))
                    .expect("a JSON value serializes");
                bytes.push(b'\n');
                bytes
            }
            Doc::Toml(mut doc) => {
                let servers = doc.entry("mcp_servers").or_insert_with(|| {
                    let mut servers = toml_edit::Table::new();
                    servers.set_implicit(true);
                    toml_edit::Item::Table(servers)
                });
                if let toml_edit::Item::Table(servers) = servers {
                    match entry {
                        Some(entry) => {
                            let mut table = toml_table(entry);
                            // A replaced table keeps its place and the
                            // comments above it.
                            if let Some(old) = servers.get(SERVER).and_then(|item| item.as_table())
                            {
                                *table.decor_mut() = old.decor().clone();
                                if let Some(position) = old.position() {
                                    table.set_position(Some(position));
                                }
                            }
                            servers.insert(SERVER, toml_edit::Item::Table(table));
                        }
                        None => {
                            servers.remove(SERVER);
                        }
                    }
                }
                let text = doc.to_string();
                if self.crlf {
                    text.replace("\r\n", "\n")
                        .replace('\n', "\r\n")
                        .into_bytes()
                } else {
                    text.into_bytes()
                }
            }
        }
    }
}

/// A JSON object as a `[mcp_servers.mdoc]` table: strings, arrays and an
/// inline `env` table.
fn toml_table(entry: &Value) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    if let Value::Object(fields) = entry {
        for (key, value) in fields {
            table.insert(key, toml_edit::Item::Value(toml_value(value)));
        }
    }
    table
}

fn toml_value(value: &Value) -> toml_edit::Value {
    match value {
        Value::String(text) => text.as_str().into(),
        Value::Bool(on) => (*on).into(),
        Value::Number(number) => match number.as_i64() {
            Some(integer) => integer.into(),
            None => number.as_f64().unwrap_or_default().into(),
        },
        Value::Array(items) => toml_edit::Value::Array(items.iter().map(toml_value).collect()),
        Value::Object(fields) => {
            let mut table = toml_edit::InlineTable::new();
            for (key, value) in fields {
                table.insert(key, toml_value(value));
            }
            toml_edit::Value::InlineTable(table)
        }
        Value::Null => "".into(),
    }
}

/// A TOML entry as JSON, to compare and show it as the JSON one is.
fn json_of_item(item: &toml_edit::Item) -> Value {
    match item {
        toml_edit::Item::Value(value) => json_of_value(value),
        toml_edit::Item::Table(table) => Value::Object(
            table
                .iter()
                .map(|(key, item)| (key.to_owned(), json_of_item(item)))
                .collect(),
        ),
        toml_edit::Item::ArrayOfTables(tables) => Value::Array(
            tables
                .iter()
                .map(|table| json_of_item(&toml_edit::Item::Table(table.clone())))
                .collect(),
        ),
        toml_edit::Item::None => Value::Null,
    }
}

fn json_of_value(value: &toml_edit::Value) -> Value {
    match value {
        toml_edit::Value::String(text) => text.value().as_str().into(),
        toml_edit::Value::Integer(integer) => (*integer.value()).into(),
        toml_edit::Value::Float(float) => json!(*float.value()),
        toml_edit::Value::Boolean(on) => (*on.value()).into(),
        toml_edit::Value::Datetime(when) => when.value().to_string().into(),
        toml_edit::Value::Array(items) => Value::Array(items.iter().map(json_of_value).collect()),
        toml_edit::Value::InlineTable(table) => Value::Object(
            table
                .iter()
                .map(|(key, value)| (key.to_owned(), json_of_value(value)))
                .collect(),
        ),
    }
}

/// Where an AI app stands, read from its file now.
pub(crate) fn status(places: &Places, host: Host) -> Status {
    let Some(path) = places.config(host) else {
        return Status::NotInstalled;
    };
    match Snapshot::read(&path, host) {
        Err(refusal) => Status::Unreadable(refusal),
        Ok(snapshot) => match snapshot.entry() {
            None => Status::NotAdded,
            Some(entry) if places.entry() == Ok(entry.clone()) => Status::Added,
            Some(_) => Status::Other,
        },
    }
}

/// What a change does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Add,
    /// Replaces another `mdoc` entry, shown as `old`.
    Replace {
        old: Value,
    },
    Remove,
}

/// One change to an AI app's config, planned for the consent card: what it
/// shows is what [`apply`] writes, and only over the bytes it was planned
/// from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change {
    pub host: Host,
    pub kind: Kind,
    pub path: PathBuf,
    /// The entry added, or removed.
    pub entry: Value,
    /// Comment lines Remove takes out with the entry: those just above
    /// its TOML table.
    comments: Option<String>,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
}

impl Change {
    /// The entry as the card shows it: as it reads in the file, with the
    /// comments that go with it.
    pub(crate) fn shown(&self) -> String {
        match &self.comments {
            Some(comments) => format!("{comments}\n{}", shown(self.host, &self.entry)),
            None => shown(self.host, &self.entry),
        }
    }

    /// The other entry a replace takes out, as the card shows it.
    pub(crate) fn replaced(&self) -> Option<String> {
        match &self.kind {
            Kind::Replace { old } => Some(shown(self.host, old)),
            _ => None,
        }
    }

    /// Whether [`apply`] keeps a backup: only of a file that was there.
    pub(crate) fn backs_up(&self) -> bool {
        self.before.is_some()
    }

    /// The backup's file name, as the card names it.
    pub(crate) fn backup_name(&self) -> String {
        backup(&self.path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    /// The whole file as [`apply`] writes it.
    #[cfg(test)]
    pub(crate) fn written(&self) -> &[u8] {
        &self.after
    }
}

/// `entry` under the app's servers, as it reads in its file.
fn shown(host: Host, entry: &Value) -> String {
    match host {
        Host::Claude => serde_json::to_string_pretty(&json!({ "mcpServers": { SERVER: entry } }))
            .expect("a JSON value serializes"),
        Host::ChatGpt => {
            let mut servers = toml_edit::Table::new();
            servers.set_implicit(true);
            servers.insert(SERVER, toml_edit::Item::Table(toml_table(entry)));
            let mut doc = toml_edit::DocumentMut::new();
            doc.insert("mcp_servers", toml_edit::Item::Table(servers));
            doc.to_string().trim_end().to_owned()
        }
    }
}

/// Plan adding this shim's entry, or replacing another `mdoc` entry.
pub(crate) fn plan_add(places: &Places, host: Host) -> Result<Change, Refusal> {
    let path = places.config(host).ok_or(Refusal::NotInstalled)?;
    let entry = places.entry()?;
    let snapshot = Snapshot::read(&path, host)?;
    let kind = match snapshot.entry() {
        Some(old) if old == entry => return Err(Refusal::AlreadyAdded),
        Some(old) => Kind::Replace { old },
        None => Kind::Add,
    };
    let before = snapshot.bytes.clone();
    let after = snapshot.with(Some(&entry));
    Ok(Change {
        host,
        kind,
        path,
        entry,
        comments: None,
        before,
        after,
    })
}

/// Plan taking the `mdoc` entry out; the other servers stay.
pub(crate) fn plan_remove(places: &Places, host: Host) -> Result<Change, Refusal> {
    let path = places.config(host).ok_or(Refusal::NotInstalled)?;
    let snapshot = Snapshot::read(&path, host)?;
    let entry = snapshot.entry().ok_or(Refusal::NotAdded)?;
    let comments = snapshot.entry_comments();
    let before = snapshot.bytes.clone();
    let after = snapshot.with(None);
    Ok(Change {
        host,
        kind: Kind::Remove,
        path,
        entry,
        comments,
        before,
        after,
    })
}

/// Write `change`, the user having said yes to it: only if the file is
/// still what it was planned from, through temp files in the same folder
/// so the AI app never reads half of it. The file as it was is kept once
/// beside it. Both files are staged first, so the check is the last step
/// before they land; a new file never replaces one that appeared since.
pub(crate) fn apply(change: &Change) -> Result<(), Refusal> {
    let folder = change.path.parent().ok_or(Refusal::NotInstalled)?;
    if !folder.is_dir() {
        return Err(Refusal::NotInstalled);
    }
    if is_link(&change.path) {
        return Err(Refusal::Linked);
    }
    let failed = |error: std::io::Error| Refusal::Io {
        writing: true,
        error: error.to_string(),
    };
    let staged = stage(&change.path, folder, &change.after).map_err(failed)?;
    let kept = (change.before.as_deref())
        .map(|before| stage(&change.path, folder, before))
        .transpose()
        .map_err(failed)?;
    let now = match std::fs::read(&change.path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(Refusal::Io {
                writing: false,
                error: error.to_string(),
            });
        }
    };
    if now != change.before {
        return Err(Refusal::Changed);
    }
    // The file as it was, once, beside it; the next change replaces it.
    if let Some(kept) = kept {
        kept.persist(backup(&change.path))
            .map_err(|error| failed(error.error))?;
    }
    match change.before {
        Some(_) => staged.persist(&change.path).map(drop),
        None => staged.persist_noclobber(&change.path).map(drop),
    }
    .map_err(|error| match error.error.kind() {
        std::io::ErrorKind::AlreadyExists => Refusal::Changed,
        _ => failed(error.error),
    })
}

/// Where [`apply`] keeps the file as it was before its change.
pub(crate) fn backup(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(BACKUP);
    path.with_file_name(name)
}

/// Whether `path` is a symbolic link (a dotfiles repository's, say).
fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

/// `bytes` in a temp file in `folder`, synced, with `path`'s mode on Unix.
fn stage(path: &Path, folder: &Path, bytes: &[u8]) -> std::io::Result<tempfile::NamedTempFile> {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new_in(folder)?;
    file.write_all(bytes)?;
    // The temp file is the user's alone; keep the old file's mode instead.
    #[cfg(unix)]
    if let Ok(old) = std::fs::metadata(path) {
        file.as_file().set_permissions(old.permissions())?;
    }
    #[cfg(not(unix))]
    let _ = path;
    file.as_file().sync_all()?;
    Ok(file)
}

/// The link Claude's Test connection opens: a new chat with the first
/// prompt filled in and not sent (item 1's spike).
pub(crate) fn claude_test_link() -> String {
    crate::ai::links::claude(FIRST_PROMPT)
}

#[cfg(test)]
mod tests;
