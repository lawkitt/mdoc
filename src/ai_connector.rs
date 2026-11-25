//! mdoc's end of the AI connector (ADR 0126; Milestone 2 items 7 and 8):
//! while "AI connector (preview)" is on and the Vault is open, the
//! connector service and an owner-only listener for the `mdoc-mcp` shim
//! (`mdoc_mcp::ipc`). Each call is answered by `mdoc_connector::serve`,
//! so every read and delivery is the service's. With the setting off
//! there is no service and no listener at all.
//!
//! Each call also tells the window what it needs to know ([`Event`]): an
//! AI app asked while nothing is shared (the share prompt, item 8), or a
//! read or delivery was recorded (the Project page's AI activity).
use mdoc_connector::Connector;
use mdoc_mcp::ipc::{self, Endpoint, Listening};
use mdoc_mcp::wire::{self, Answer, Done, Refusal, Request};
use mdoc_store::settings::AiConnectorConfig;
use mdoc_vault::Vault;
use std::sync::{
    Arc,
    mpsc::{Receiver, Sender, channel},
};

/// What a call tells the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// AI app `app` asked while nothing was shared: mdoc asks the user to
    /// share a Project (#1164 Decision 2).
    Asked { app: String },
    /// A read or a delivery was recorded in the AI project's activity.
    Activity,
}

/// The connector, its events and its listener; dropping it stops
/// listening.
pub(crate) struct Running {
    /// Shared with the window: what it shares, Stop sharing (item 8).
    pub(crate) connector: Arc<Connector>,
    events: Receiver<Event>,
    /// What the listener runs for each call; tests and renders call it
    /// as the shim would.
    answer: ipc::Handler,
    /// `None` in tests and renders: they never take the profile's
    /// endpoint.
    _listening: Option<Listening>,
}

impl Drop for Running {
    /// A connection the listener accepted holds its handler, and so the
    /// connector, past the stop: from now on it is shared nothing, so such
    /// a call reads and delivers nothing.
    fn drop(&mut self) {
        self.connector.stop_sharing();
    }
}

impl Running {
    /// The events since the last call, oldest first.
    pub(crate) fn events(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// Tests and renders: answer `request` as the listener does.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn call(&self, request: &Request) -> Answer {
        let line = serde_json::to_vec(request).expect("requests serialize");
        serde_json::from_slice(&(self.answer)(&line)).expect("answers parse")
    }
}

/// The service over `vault` and the handler a listener runs, while
/// `setting` is on.
fn service(vault: Arc<Vault>, setting: &AiConnectorConfig) -> Option<Running> {
    let connector = Arc::new(Connector::start(vault, setting)?);
    let (events, received) = channel();
    let serving = connector.clone();
    let answer: ipc::Handler = Arc::new(move |line: &[u8]| answer_line(&serving, &events, line));
    Some(Running {
        connector,
        events: received,
        answer,
        _listening: None,
    })
}

/// Start the connector over `vault` and listen at `endpoint`, while
/// `setting` is on; `None` while it is off, or when the endpoint can't be
/// held (it says why on stderr).
pub(crate) fn start(
    vault: Arc<Vault>,
    setting: &AiConnectorConfig,
    endpoint: &Endpoint,
) -> Option<Running> {
    let mut running = service(vault, setting)?;
    match ipc::listen(endpoint, running.answer.clone()) {
        Ok(listening) => {
            running._listening = Some(listening);
            Some(running)
        }
        Err(error) => {
            eprintln!("mdoc: the AI connector can't listen at {endpoint}: {error}");
            None
        }
    }
}

/// Tests and renders: the connector while `setting` is on, with no
/// listener.
pub(crate) fn start_unlistened(vault: Arc<Vault>, setting: &AiConnectorConfig) -> Option<Running> {
    service(vault, setting)
}

/// This profile's endpoint: the dev profile's when there is one.
pub(crate) fn endpoint() -> Option<Endpoint> {
    Endpoint::for_profile(mdoc_store::app_paths::dev_profile())
}

/// Answer one request line through `mdoc_connector::serve`, and tell the
/// window what it needs to know.
fn answer_line(connector: &Connector, events: &Sender<Event>, line: &[u8]) -> Vec<u8> {
    let request = (serde_json::from_slice::<Request>(line).ok()).filter(|r| r.v == wire::VERSION);
    let Some(request) = request else {
        // A refusal; nothing for the window.
        return mdoc_connector::serve::answer_line(connector, line);
    };
    let app = request.app.clone();
    let answer = mdoc_connector::serve::answer(connector, request);
    if let Some(event) = event_of(app, &answer) {
        // The window may have gone; the answer stands.
        let _ = events.send(event);
    }
    serde_json::to_vec(&answer).expect("answers serialize")
}

/// What `answer` to AI app `app` tells the window, if anything.
fn event_of(app: String, answer: &Answer) -> Option<Event> {
    match answer {
        Answer::Refused(Refusal::NothingShared) => Some(Event::Asked { app }),
        Answer::Ok(
            Done::Text { .. } | Done::Outline { .. } | Done::Part { .. } | Done::Delivered { .. },
        ) => Some(Event::Activity),
        _ => None,
    }
}

/// An AI app's name as the window shows it: the name the app gives
/// itself (`clientInfo.name`) mapped to its product's, else as sent. A
/// screen detail: activity and replies keep the name as sent.
pub(crate) fn display_name(app: &str) -> String {
    // As sent, over the pipe: one line of at most 64 characters, as the
    // activity keeps it.
    let line: String = (app.chars())
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let line: String = line.trim().chars().take(64).collect();
    let app = line.trim_end();
    match app.to_ascii_lowercase().as_str() {
        "" | "ai app" => "An AI app".into(),
        "claude-ai" | "claude" => "Claude".into(),
        "claude-code" => "Claude Code".into(),
        "chatgpt" | "openai-chatgpt" => "ChatGPT".into(),
        "codex" | "codex-mcp-client" => "Codex".into(),
        _ => app.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_mcp::wire::{Answer, Call, Refusal, Request};

    fn ask(endpoint: &Endpoint) -> Result<Answer, ipc::CallError> {
        let request = Request {
            v: mdoc_mcp::wire::VERSION,
            app: "claude-ai".into(),
            call: Call::ListDocuments,
        };
        let answer = ipc::call(endpoint, &serde_json::to_vec(&request).unwrap())?;
        Ok(serde_json::from_slice(&answer).unwrap())
    }

    /// Done when: with the setting off nothing listens, and with it on
    /// the shim's calls reach the connector, until it stops.
    #[test]
    fn the_listener_exists_only_while_the_setting_is_on() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::open(dir.path().join("vault"), &[3; 32]).unwrap());
        let endpoint = Endpoint::for_profile(Some(&dir.path().join("profile"))).unwrap();
        let off = AiConnectorConfig::default();
        assert!(start(vault.clone(), &off, &endpoint).is_none());
        assert!(matches!(ask(&endpoint), Err(ipc::CallError::NotListening)));
        let on = AiConnectorConfig { preview: true };
        let running = start(vault, &on, &endpoint).unwrap();
        assert!(running.connector.ai_project().is_none());
        assert_eq!(
            ask(&endpoint).unwrap(),
            Answer::Refused(Refusal::NothingShared)
        );
        // Over the pipe too, the window hears that an app asked.
        assert_eq!(
            running.events(),
            [Event::Asked {
                app: "claude-ai".into()
            }]
        );
        drop(running);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !matches!(ask(&endpoint), Err(ipc::CallError::NotListening)) {
            assert!(std::time::Instant::now() < deadline, "still listening");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Only a call that found nothing shared asks; a bad request or a
    /// refusal of another kind tells the window nothing.
    #[test]
    fn only_a_call_finding_nothing_shared_asks() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::open(dir.path().join("vault"), &[3; 32]).unwrap());
        let on = AiConnectorConfig { preview: true };
        let running = start_unlistened(vault.clone(), &on).unwrap();
        assert!(running.events().is_empty());
        (running.answer)(b"not json");
        assert!(running.events().is_empty());
        let project = vault.create_project(Default::default()).unwrap();
        running.connector.share(&project.id).unwrap();
        let request = Request {
            v: wire::VERSION,
            app: "claude-ai".into(),
            call: Call::GetDocument {
                doc_id: "00".repeat(16),
                part: None,
            },
        };
        assert_eq!(running.call(&request), Answer::Refused(Refusal::NotShared));
        assert!(running.events().is_empty());
        running.connector.stop_sharing();
        assert_eq!(
            running.call(&request),
            Answer::Refused(Refusal::NothingShared)
        );
        assert_eq!(
            running.events(),
            [Event::Asked {
                app: "claude-ai".into()
            }]
        );
    }

    /// Review of #1213 (P1): a connection accepted before the setting went
    /// off keeps the handler; its later calls find nothing shared.
    #[test]
    fn a_connection_held_across_the_stop_reads_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::open(dir.path().join("vault"), &[3; 32]).unwrap());
        let on = AiConnectorConfig { preview: true };
        let running = start_unlistened(vault.clone(), &on).unwrap();
        let project = vault.create_project(Default::default()).unwrap();
        running.connector.share(&project.id).unwrap();
        let list = serde_json::to_vec(&Request {
            v: wire::VERSION,
            app: "claude-ai".into(),
            call: Call::ListDocuments,
        })
        .unwrap();
        let held = running.answer.clone();
        let answer = |line: &[u8]| serde_json::from_slice::<Answer>(&held(line)).unwrap();
        assert!(matches!(answer(&list), Answer::Ok(Done::Documents { .. })));
        drop(running);
        assert_eq!(answer(&list), Answer::Refused(Refusal::NothingShared));
    }

    #[test]
    fn apps_show_by_their_products_names() {
        assert_eq!(display_name("claude-ai"), "Claude");
        assert_eq!(display_name("claude-code"), "Claude Code");
        assert_eq!(display_name("openai-chatgpt"), "ChatGPT");
        assert_eq!(display_name("codex-mcp-client"), "Codex");
        assert_eq!(display_name("AI app"), "An AI app");
        assert_eq!(display_name(""), "An AI app");
        assert_eq!(display_name("Zed"), "Zed");
        assert_eq!(display_name(" Claude-AI\n"), "Claude");
        assert_eq!(display_name("a\nb"), "a b");
        assert_eq!(display_name(&"x".repeat(100)).chars().count(), 64);
    }

    /// The shim reads the dev profile as mdoc does.
    #[test]
    fn the_shim_names_the_dev_profile_as_mdoc_does() {
        use mdoc_store::app_paths;
        assert_eq!(ipc::DEV_PROFILE_VAR, app_paths::DEV_PROFILE_VAR);
        assert_eq!(ipc::DEV_PROFILE_HONOURED, app_paths::DEV_PROFILE_HONOURED);
    }
}
