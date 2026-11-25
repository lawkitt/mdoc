//! MCP over stdio, by hand on `serde_json` (no MCP crate, no async), in
//! both protocol eras:
//!
//! - **legacy** (2024-11-05 to 2025-11-25): `initialize` negotiates a
//!   version for the process, then requests come without `_meta`;
//! - **modern** (2026-07-28, stateless): `server/discover`, and every
//!   request names its version and client in `_meta`; results carry
//!   `resultType`, the server's identity and, for lists, cache hints.
//!
//! Three tools, a "restore" prompt and the instructions (#1164 Decisions 7,
//! 8 and 23). Every sentence the model reads is here; mdoc answers by code
//! (`wire`), with only checked document text, labels and headings.
use crate::{
    forward::{Backend, Unreachable},
    wire::{self, Answer, Call, Done, Kind, Refusal, Request, Why},
};
use serde_json::{Map, Value, json};

pub const MODERN: &str = "2026-07-28";
/// Newest first; `initialize` answers the first when asked for another.
pub const LEGACY: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const CLIENT_KEY: &str = "io.modelcontextprotocol/clientInfo";
const SERVER_KEY: &str = "io.modelcontextprotocol/serverInfo";
const UNSUPPORTED_VERSION: i64 = -32022;
/// Lists never change while the shim runs.
const TTL_MS: u64 = 3_600_000;

pub const INSTRUCTIONS: &str = "mdoc shares the user's documents with every name and identifier replaced by an alias such as PERSON_1 or ORG_2; only mdoc knows the real names. Call list_documents, then get_document for what you need. A document that needs review has no text yet: ask the user to review it in mdoc. Write your answer with the aliases exactly as they appear, and never guess a real name. Show your full answer in the chat, then call deliver_reply with that same answer and the doc_ids you read, so mdoc can show it with the names restored. Use kind \"document\" with a title when the user asked for a new document, such as a draft; otherwise \"reply\".";

const RESTORE: &str = "Call deliver_reply with your last answer, exactly as you showed it, aliases and all, with kind \"reply\" and the doc_ids of the documents you read for it. Then say it was delivered; don't repeat the answer.";

fn supported() -> Vec<&'static str> {
    std::iter::once(MODERN).chain(LEGACY).collect()
}

fn server_info() -> Value {
    json!({"name": "mdoc", "title": "mdoc", "version": env!("CARGO_PKG_VERSION")})
}

fn capabilities() -> Value {
    json!({"tools": {}, "prompts": {}})
}

/// The tools, in a fixed order.
pub fn tools() -> Value {
    let read_only = json!({"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false});
    json!({"tools": [
        {
            "name": "list_documents",
            "title": "List shared documents",
            "description": "Lists the documents of the Project the user shared from mdoc: each one's doc_id, label, size in characters and words, and state (\"ready\" or \"needs review\"). Names in labels are aliases.",
            "inputSchema": {"type": "object", "additionalProperties": false},
            "annotations": read_only,
        },
        {
            "name": "get_document",
            "title": "Read a shared document",
            "description": "Returns a ready document's text, with every name replaced by an alias such as PERSON_1. A document over 60,000 characters returns an outline of numbered parts instead: call again with part to read one. A document that needs review returns no text: ask the user to review it in mdoc, then call again.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "doc_id": {"type": "string", "description": "From list_documents."},
                    "part": {"type": "integer", "minimum": 1, "description": "A part of a long document, from its outline."},
                },
                "required": ["doc_id"],
                "additionalProperties": false,
            },
            "annotations": read_only,
        },
        {
            "name": "deliver_reply",
            "title": "Deliver your answer to mdoc",
            "description": "Sends your answer to mdoc, which shows it to the user with the real names restored. Pass the answer exactly as you showed it in the chat, aliases and all, and the doc_ids of the documents you read for it. kind \"reply\" lands on mdoc's Reply page; kind \"document\" saves it as a new document in the shared Project and needs a title. Call it after showing the answer in the chat, never instead.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "enum": ["reply", "document"]},
                    "markdown": {"type": "string", "description": "Your answer, with its aliases."},
                    "read": {"type": "array", "items": {"type": "string"}, "minItems": 1, "description": "The doc_ids you read for it."},
                    "title": {"type": "string", "description": "Needed for a document."},
                },
                "required": ["kind", "markdown", "read"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false},
        },
    ]})
}

pub fn prompts() -> Value {
    json!({"prompts": [
        {"name": "restore", "title": "Restore in mdoc", "description": "Deliver your last answer to mdoc.", "arguments": []},
    ]})
}

/// Which era a request is served in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Era {
    Legacy,
    Modern,
}

/// A JSON-RPC error: code and message, and its data.
type Failure = (i64, String, Option<Value>);

fn invalid(message: impl Into<String>) -> Failure {
    (-32602, message.into(), None)
}

/// One stdio process's server.
pub struct Server<B> {
    backend: B,
    /// The version `initialize` negotiated.
    legacy: Option<&'static str>,
    /// The AI app, as it last named itself.
    app: Option<String>,
}

impl<B: Backend> Server<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            legacy: None,
            app: None,
        }
    }

    /// One message line: the line to write back, if any (notifications and
    /// responses get none). A batch, which 2025-03-26 allows, is answered
    /// as one.
    pub fn handle(&mut self, line: &[u8]) -> Option<String> {
        let Ok(message) = serde_json::from_slice::<Value>(line) else {
            let failure = (-32700, "Parse error".into(), None);
            return Some(reply(Value::Null, Err(failure)).to_string());
        };
        let answer = match message {
            Value::Array(batch) if !batch.is_empty() => {
                let answers: Vec<Value> = batch.iter().filter_map(|m| self.message(m)).collect();
                (!answers.is_empty()).then_some(Value::Array(answers))
            }
            message => self.message(&message),
        };
        // serde_json escapes newlines in strings: one line, as stdio needs.
        answer.map(|answer| answer.to_string())
    }

    fn message(&mut self, message: &Value) -> Option<Value> {
        let Value::Object(message) = message else {
            let failure = (-32600, "Invalid Request".into(), None);
            return Some(reply(Value::Null, Err(failure)));
        };
        let id = message.get("id")?.clone();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Some(reply(id, Err((-32600, "Invalid Request".into(), None))));
        };
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        Some(reply(id, self.request(method, &params)))
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, Failure> {
        if method == "initialize" {
            return Ok(self.initialize(params));
        }
        let meta = params.get("_meta");
        let era = match meta.and_then(|meta| meta.get(VERSION_KEY)) {
            Some(version) => {
                let known = version.as_str().filter(|v| supported().contains(v));
                match known {
                    Some(MODERN) => Era::Modern,
                    Some(_) => Era::Legacy,
                    None => {
                        let data = json!({"supported": supported(), "requested": version});
                        let message = "Unsupported protocol version".into();
                        return Err((UNSUPPORTED_VERSION, message, Some(data)));
                    }
                }
            }
            None if method == "server/discover" => Era::Modern,
            None if self.legacy.is_some() => Era::Legacy,
            None => {
                let message = format!(
                    "Send initialize first, or name the protocol version in _meta (supported: {})",
                    supported().join(", ")
                );
                return Err((-32600, message, None));
            }
        };
        // A modern request names its own app; a legacy one, initialize's.
        let client = meta.and_then(|meta| meta.get(CLIENT_KEY));
        let named = client.and_then(|client| client.get("name")?.as_str());
        let app = match era {
            Era::Modern => named.map(str::to_string),
            Era::Legacy => named.map(str::to_string).or_else(|| self.app.clone()),
        };
        let mut result = match method {
            "server/discover" => json!({
                "supportedVersions": supported(),
                "capabilities": capabilities(),
                "instructions": INSTRUCTIONS,
            }),
            "ping" => json!({}),
            "tools/list" => tools(),
            "tools/call" => self.call_tool(params, app)?,
            "prompts/list" => prompts(),
            "prompts/get" => prompt(params)?,
            _ => return Err((-32601, format!("Method not found: {method}"), None)),
        };
        if era == Era::Modern {
            let fields = result.as_object_mut().expect("results are objects");
            fields.insert("resultType".into(), "complete".into());
            fields.insert("_meta".into(), json!({SERVER_KEY: server_info()}));
            if matches!(method, "server/discover" | "tools/list" | "prompts/list") {
                fields.insert("ttlMs".into(), TTL_MS.into());
                fields.insert("cacheScope".into(), "public".into());
            }
        }
        Ok(result)
    }

    fn initialize(&mut self, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = LEGACY
            .into_iter()
            .find(|v| Some(*v) == asked)
            .unwrap_or(LEGACY[0]);
        self.legacy = Some(version);
        let client = params
            .get("clientInfo")
            .and_then(|c| c.get("name")?.as_str());
        if let Some(name) = client {
            self.app = Some(name.to_string());
        }
        json!({
            "protocolVersion": version,
            "capabilities": capabilities(),
            "serverInfo": server_info(),
            "instructions": INSTRUCTIONS,
        })
    }

    fn call_tool(&mut self, params: &Value, app: Option<String>) -> Result<Value, Failure> {
        let name = params.get("name").and_then(Value::as_str);
        let name = name.ok_or_else(|| invalid("tools/call needs a tool name"))?;
        let empty = Map::new();
        let args = match params.get("arguments") {
            None | Some(Value::Null) => &empty,
            Some(Value::Object(args)) => args,
            Some(_) => return Err(invalid("arguments must be an object")),
        };
        let call = match name {
            "list_documents" => Ok(Call::ListDocuments),
            "get_document" => get_document(args),
            "deliver_reply" => deliver_reply(args),
            _ => return Err(invalid(format!("Unknown tool: {name}"))),
        };
        let call = match call {
            Ok(call) => call,
            Err(why) => return Ok(text(why, true)),
        };
        let request = Request {
            v: wire::VERSION,
            app: app.unwrap_or_else(|| "AI app".into()),
            call,
        };
        Ok(match self.backend.call(&request) {
            Ok(answer) => result(answer),
            Err(Unreachable::TimedOut) if matches!(request.call, Call::Deliver { .. }) => {
                text(DELIVERY_UNKNOWN, true)
            }
            Err(unreachable) => text(unreachable_text(unreachable), true),
        })
    }
}

fn get_document(args: &Map<String, Value>) -> Result<Call, String> {
    let doc_id = args.get("doc_id").and_then(Value::as_str);
    let doc_id = doc_id.ok_or("get_document needs a doc_id from list_documents.")?;
    let part = match args.get("part") {
        None | Some(Value::Null) => None,
        Some(part) => Some(
            (part.as_u64())
                .filter(|&n| n >= 1)
                .ok_or("part is a whole number from 1.")? as usize,
        ),
    };
    Ok(Call::GetDocument {
        doc_id: doc_id.to_string(),
        part,
    })
}

fn deliver_reply(args: &Map<String, Value>) -> Result<Call, String> {
    let kind = match args.get("kind").and_then(Value::as_str) {
        Some("reply") => Kind::Reply,
        Some("document") => Kind::Document,
        _ => return Err("kind is \"reply\" or \"document\".".into()),
    };
    let markdown = args.get("markdown").and_then(Value::as_str);
    let markdown = markdown.ok_or("deliver_reply needs your answer as markdown.")?;
    let read = args.get("read").and_then(Value::as_array);
    let read: Option<Vec<String>> = read.and_then(|read| {
        let ids = read.iter().map(|id| id.as_str().map(str::to_string));
        ids.collect()
    });
    let read = read.ok_or("read lists the doc_ids you read, as strings.")?;
    let title = match args.get("title") {
        None | Some(Value::Null) => None,
        Some(Value::String(title)) => Some(title.clone()),
        Some(_) => return Err("title is a string.".into()),
    };
    Ok(Call::Deliver {
        kind,
        title,
        read,
        markdown: markdown.to_string(),
    })
}

fn prompt(params: &Value) -> Result<Value, Failure> {
    match params.get("name").and_then(Value::as_str) {
        Some("restore") => Ok(json!({
            "description": "Deliver your last answer to mdoc.",
            "messages": [{"role": "user", "content": {"type": "text", "text": RESTORE}}],
        })),
        _ => Err(invalid("Unknown prompt")),
    }
}

fn text(text: impl Into<String>, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": text.into()}], "isError": is_error})
}

/// What mdoc answered, for the model.
fn result(answer: Answer) -> Value {
    let done = match answer {
        Answer::Ok(done) => done,
        Answer::Refused(refusal) => return text(refusal_text(refusal), true),
    };
    match done {
        Done::Documents { documents } => {
            let rows: Vec<Value> = documents
                .into_iter()
                .map(|row| {
                    let state = if row.needs_review.is_some() { "needs review" } else { "ready" };
                    json!({"doc_id": row.doc_id, "label": row.label, "chars": row.chars, "words": row.words, "state": state})
                })
                .collect();
            let listed = json!({"documents": rows});
            json!({
                "content": [{"type": "text", "text": listed.to_string()}],
                "structuredContent": listed,
                "isError": false,
            })
        }
        Done::Text { text: body, .. } => text(body, false),
        Done::Outline {
            label,
            note,
            chars,
            parts,
            ..
        } => {
            let of = parts.len();
            let mut out = note;
            if !out.is_empty() && !out.ends_with("\n\n") {
                out.push_str(if out.ends_with('\n') { "\n" } else { "\n\n" });
            }
            out.push_str(&format!(
                "\"{label}\" is too long to read at once ({chars} characters). Read the parts you need with get_document and part:\n"
            ));
            for row in parts {
                let headings: Vec<&str> = row.headings.iter().map(|h| h.text.as_str()).collect();
                let (part, size) = (row.part, row.chars);
                out.push_str(&format!("\nPart {part} of {of}, {size} characters"));
                if !headings.is_empty() {
                    out.push_str(&format!(": {}", headings.join("; ")));
                }
            }
            text(out, false)
        }
        Done::Part {
            part,
            of,
            text: body,
            ..
        } => json!({
            "content": [
                {"type": "text", "text": format!("Part {part} of {of}.")},
                {"type": "text", "text": body},
            ],
            "isError": false,
        }),
        Done::NeedsReview { label, why, .. } => text(
            format!(
                "\"{label}\" needs review in mdoc before it can be read: {}. Ask the user to review it in mdoc, then call get_document again.",
                why_text(why)
            ),
            false,
        ),
        Done::Delivered {
            document,
            restored,
            unresolved,
            ..
        } => {
            let landed = if document.is_some() {
                "a new document"
            } else {
                "a reply"
            };
            let names = if restored == 1 { "name" } else { "names" };
            let mut out =
                format!("Delivered to mdoc as {landed}. mdoc restored {restored} {names}.");
            if !unresolved.is_empty() {
                out.push_str(&format!(
                    " These aliases match no name in the shared Project and stay as written: {}.",
                    unresolved.join(", ")
                ));
            }
            text(out, false)
        }
    }
}

fn why_text(why: Why) -> String {
    match why {
        Why::NotReviewed => "it hasn't been reviewed".into(),
        Why::ChangedSinceReview => "it changed since it was reviewed".into(),
        Why::NamesNotReplaced { count: 1 } => "it still has a name that isn't replaced".into(),
        Why::NamesNotReplaced { count } => {
            format!("it still has {count} names that aren't replaced")
        }
    }
}

fn refusal_text(refusal: Refusal) -> String {
    match refusal {
        Refusal::NothingShared => "No Project is shared with AI apps. Ask the user to share one in mdoc.".into(),
        Refusal::NotShared => "That document isn't in the shared Project. Call list_documents for the ones you can read.".into(),
        Refusal::NoSuchPart { of } => format!("That document has {of} parts; part counts from 1."),
        Refusal::ShareChanged => "The shared Project changed while reading. Call list_documents again.".into(),
        Refusal::NotRead => "Name the documents you read for this answer, each read with get_document since the Project was shared. If the user shared another Project since, read its documents first.".into(),
        Refusal::TitleNeeded => "A document needs a title.".into(),
        Refusal::Empty => "The answer is empty.".into(),
        Refusal::Unavailable => "mdoc couldn't read its documents.".into(),
        Refusal::BadRequest | Refusal::Version => "mdoc and its connector don't match. Update mdoc.".into(),
    }
}

/// A delivery mdoc took and didn't answer: it may have landed.
const DELIVERY_UNKNOWN: &str = "mdoc didn't answer in time, so this answer may or may not have been delivered. Ask the user to check mdoc before you deliver it again.";

fn unreachable_text(unreachable: Unreachable) -> &'static str {
    match unreachable {
        Unreachable::NotRunning => {
            "mdoc isn't answering. Open mdoc, unlock it if asked, and turn on AI connector (preview) in Settings, then try again."
        }
        Unreachable::NotOurs => {
            "mdoc's connector couldn't be verified as this user's, so nothing was sent."
        }
        Unreachable::Failed => "The call to mdoc failed. Try again.",
        Unreachable::TimedOut => "mdoc didn't answer in time. Try again.",
    }
}

fn reply(id: Value, result: Result<Value, Failure>) -> Value {
    match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, message, data)) => {
            let mut error = json!({"code": code, "message": message});
            if let Some(data) = data {
                error["data"] = data;
            }
            json!({"jsonrpc": "2.0", "id": id, "error": error})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A backend that records what it was asked and answers `answer`.
    struct Fake {
        asked: Vec<Request>,
        answer: Result<Answer, Unreachable>,
    }

    impl Backend for &mut Fake {
        fn call(&mut self, request: &Request) -> Result<Answer, Unreachable> {
            self.asked.push(request.clone());
            self.answer.clone()
        }
    }

    fn fake() -> Fake {
        Fake {
            asked: Vec::new(),
            answer: Ok(Answer::Refused(Refusal::NothingShared)),
        }
    }

    fn send<B: Backend>(server: &mut Server<B>, message: Value) -> Value {
        let line = server.handle(message.to_string().as_bytes()).unwrap();
        assert!(!line.contains('\n'));
        serde_json::from_str(&line).unwrap()
    }

    fn modern(method: &str, params: Value) -> Value {
        let mut params = params;
        params["_meta"] = json!({VERSION_KEY: MODERN, CLIENT_KEY: {"name": "claude-ai", "version": "1"}, "io.modelcontextprotocol/clientCapabilities": {}});
        json!({"jsonrpc": "2.0", "id": "m", "method": method, "params": params})
    }

    #[test]
    fn initialize_echoes_a_legacy_version_it_knows_and_offers_its_newest_otherwise() {
        for (asked, answered) in [
            ("2025-06-18", "2025-06-18"),
            ("2024-11-05", "2024-11-05"),
            ("2099-01-01", "2025-11-25"),
        ] {
            let mut backend = fake();
            let mut server = Server::new(&mut backend);
            let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": asked, "capabilities": {}, "clientInfo": {"name": "claude-ai", "version": "1"}}});
            let result = &send(&mut server, init)["result"];
            assert_eq!(result["protocolVersion"], answered);
            assert_eq!(result["instructions"], INSTRUCTIONS);
            assert_eq!(result["capabilities"], capabilities());
            assert!(result.get("resultType").is_none(), "legacy shapes");
        }
    }

    #[test]
    fn a_request_before_initialize_without_meta_is_refused() {
        let mut backend = fake();
        let mut server = Server::new(&mut backend);
        let list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"});
        let answer = send(&mut server, list.clone());
        assert_eq!(answer["error"]["code"], -32600);
        let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25"}});
        send(&mut server, init);
        assert!(send(&mut server, list)["result"]["tools"].is_array());
        // Notifications get nothing back.
        let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert_eq!(server.handle(note.to_string().as_bytes()), None);
    }

    #[test]
    fn modern_requests_are_served_statelessly_with_their_fields() {
        let mut backend = fake();
        let mut server = Server::new(&mut backend);
        let discover = send(&mut server, modern("server/discover", json!({})))["result"].clone();
        assert_eq!(discover["supportedVersions"][0], MODERN);
        assert_eq!(discover["resultType"], "complete");
        assert_eq!(discover["_meta"][SERVER_KEY]["name"], "mdoc");
        assert_eq!(discover["ttlMs"], TTL_MS);
        assert_eq!(discover["cacheScope"], "public");
        assert_eq!(discover["instructions"], INSTRUCTIONS);
        let list = send(&mut server, modern("tools/list", json!({})))["result"].clone();
        assert_eq!(list["cacheScope"], "public");
        let call = modern(
            "tools/call",
            json!({"name": "list_documents", "arguments": {}}),
        );
        let call = send(&mut server, call)["result"].clone();
        assert_eq!(call["resultType"], "complete");
        assert!(call.get("ttlMs").is_none(), "a call is not cacheable");
        drop(server);
        assert_eq!(backend.asked[0].app, "claude-ai");
    }

    #[test]
    fn an_unknown_version_names_the_supported_ones() {
        let mut backend = fake();
        let mut server = Server::new(&mut backend);
        let mut request = modern("tools/list", json!({}));
        request["params"]["_meta"][VERSION_KEY] = "1900-01-01".into();
        let error = send(&mut server, request)["error"].clone();
        assert_eq!(error["code"], UNSUPPORTED_VERSION);
        assert_eq!(error["data"]["requested"], "1900-01-01");
        assert_eq!(error["data"]["supported"], json!(supported()));
    }

    #[test]
    fn the_tools_and_their_annotations() {
        let tools = tools();
        let names: Vec<&str> = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["list_documents", "get_document", "deliver_reply"]);
        for (tool, read_only) in tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .zip([true, true, false])
        {
            assert_eq!(tool["annotations"]["readOnlyHint"], read_only);
            assert_eq!(tool["annotations"]["destructiveHint"], false);
            assert_eq!(tool["annotations"]["openWorldHint"], false);
            assert_eq!(tool["annotations"]["idempotentHint"], read_only);
        }
    }

    #[test]
    fn bad_arguments_are_answered_without_asking_mdoc() {
        let mut backend = fake();
        let mut server = Server::new(&mut backend);
        for arguments in [
            json!({"name": "get_document", "arguments": {}}),
            json!({"name": "get_document", "arguments": {"doc_id": "a", "part": 0}}),
            json!({"name": "deliver_reply", "arguments": {"kind": "memo", "markdown": "x", "read": ["a"]}}),
            json!({"name": "deliver_reply", "arguments": {"kind": "reply", "markdown": "x", "read": [1]}}),
        ] {
            let result = send(&mut server, modern("tools/call", arguments))["result"].clone();
            assert_eq!(result["isError"], true, "{result}");
        }
        let unknown = modern("tools/call", json!({"name": "rm", "arguments": {}}));
        assert_eq!(send(&mut server, unknown)["error"]["code"], -32602);
        drop(server);
        assert!(backend.asked.is_empty());
    }

    #[test]
    fn the_restore_prompt_asks_for_a_delivery() {
        let mut backend = fake();
        let mut server = Server::new(&mut backend);
        let listed = send(&mut server, modern("prompts/list", json!({})))["result"].clone();
        assert_eq!(listed["prompts"][0]["name"], "restore");
        let got = send(
            &mut server,
            modern("prompts/get", json!({"name": "restore"})),
        )["result"]
            .clone();
        let text = got["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("deliver_reply"));
    }

    #[test]
    fn mdoc_not_answering_is_a_tool_error() {
        let mut backend = fake();
        backend.answer = Err(Unreachable::NotRunning);
        let mut server = Server::new(&mut backend);
        let call = modern("tools/call", json!({"name": "list_documents"}));
        let result = send(&mut server, call)["result"].clone();
        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("AI connector (preview)")
        );
    }

    /// A delivery mdoc took and didn't answer may have landed: the model
    /// is told so, not to retry blindly.
    #[test]
    fn a_delivery_that_timed_out_is_not_retried_blindly() {
        let mut backend = fake();
        backend.answer = Err(Unreachable::TimedOut);
        let mut server = Server::new(&mut backend);
        let deliver = json!({"name": "deliver_reply", "arguments": {"kind": "reply", "markdown": "x", "read": ["a"]}});
        let result = send(&mut server, modern("tools/call", deliver))["result"].clone();
        assert_eq!(result["content"][0]["text"], DELIVERY_UNKNOWN);
        let list = modern("tools/call", json!({"name": "list_documents"}));
        let result = send(&mut server, list)["result"].clone();
        assert_ne!(result["content"][0]["text"], DELIVERY_UNKNOWN);
    }

    #[test]
    fn junk_is_a_parse_error_and_a_batch_is_answered_as_one() {
        let mut backend = fake();
        let mut server = Server::new(&mut backend);
        let junk: Value = serde_json::from_str(&server.handle(b"{nope").unwrap()).unwrap();
        assert_eq!(junk["error"]["code"], -32700);
        let empty: Value = serde_json::from_str(&server.handle(b"[]").unwrap()).unwrap();
        assert_eq!(empty["error"]["code"], -32600);
        let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26"}});
        send(&mut server, init);
        let batch = json!([
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 3, "method": "prompts/list"},
        ]);
        let answers = send(&mut server, batch);
        let answers = answers.as_array().unwrap();
        assert_eq!(answers.len(), 2, "the notification gets none");
        assert!(answers[0]["result"]["tools"].is_array());
        assert_eq!(answers[1]["id"], 3);
    }

    /// A modern request without its client's name is "AI app", whatever an
    /// earlier request named.
    #[test]
    fn each_modern_request_names_its_own_app() {
        let mut backend = fake();
        let mut server = Server::new(&mut backend);
        send(
            &mut server,
            modern("tools/call", json!({"name": "list_documents"})),
        );
        let mut unnamed = modern("tools/call", json!({"name": "list_documents"}));
        unnamed["params"]["_meta"]
            .as_object_mut()
            .unwrap()
            .remove(CLIENT_KEY);
        send(&mut server, unnamed);
        drop(server);
        let apps: Vec<&str> = backend.asked.iter().map(|r| r.app.as_str()).collect();
        assert_eq!(apps, ["claude-ai", "AI app"]);
    }
}
