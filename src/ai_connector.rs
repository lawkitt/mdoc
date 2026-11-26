//! mdoc's end of the AI connector (ADR 0126; Milestone 2 items 7 and 8):
//! while "AI connector (preview)" is on and the Vault is open, the
//! connector service and an owner-only listener for the `mdoc-mcp` shim
//! (`mdoc_mcp::ipc`). Each call is answered by `mdoc_connector::serve`,
//! so every read and delivery is the service's. With the setting off
//! there is no service and no listener at all.
//!
//! Each call also tells the window what it needs to know ([`Event`]): an
//! AI app asked while nothing is shared (the share prompt, item 8), a read
//! or delivery was recorded (the Project page's AI activity), or a document
//! it asked for needs review (item 9's notifications).
pub(crate) mod setup;

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
    /// A read of `document` was recorded in the AI project's activity.
    Activity { document: String },
    /// AI app `app` delivered: an AI Reply, and the Document it landed as
    /// when it asked for one (item 9). Recorded in the activity too.
    Delivered {
        app: String,
        reply: String,
        document: Option<String>,
    },
    /// AI app `app` asked for a document of the AI project that needs
    /// review, named to it as `label` (#1164 Decision 4; item 9).
    NeedsReview {
        app: String,
        document: String,
        label: String,
    },
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
    /// The listener stops first: from then on nothing reaches an AI app
    /// (#1220). A call it accepted still holds its handler, and so the
    /// connector: it is then shared nothing, so such a call reads and
    /// delivers nothing more. That waits for a delivery being saved, so it
    /// runs off the window's thread.
    fn drop(&mut self) {
        drop(self._listening.take());
        let connector = self.connector.clone();
        let stopped = std::thread::Builder::new()
            .name("mdoc-connector-stop".into())
            .spawn({
                let connector = connector.clone();
                move || connector.stop_sharing()
            });
        if stopped.is_err() {
            connector.stop_sharing();
        }
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
/// `setting` is on: `Ok(None)` while it is off. `Err` holds why it
/// couldn't listen, as Settings › AI connector shows it (#1220).
pub(crate) fn start(
    vault: Arc<Vault>,
    setting: &AiConnectorConfig,
    endpoint: Option<&Endpoint>,
) -> Result<Option<Running>, String> {
    let Some(running) = service(vault, setting) else {
        return Ok(None);
    };
    let Some(endpoint) = endpoint else {
        eprintln!("mdoc: the AI connector has no endpoint: no data folder");
        return Err("Couldn't start the connector: this system gives mdoc no data folder.".into());
    };
    listen(running, endpoint).map(Some)
}

/// Listen at `endpoint` for `running`.
fn listen(mut running: Running, endpoint: &Endpoint) -> Result<Running, String> {
    // Turned off and on again quickly (Settings › AI connector), the
    // listener that just stopped may still hold the name for a moment: try
    // again a few times before giving up.
    let mut tries = 1;
    loop {
        match ipc::listen(endpoint, running.answer.clone()) {
            Ok(listening) => {
                running._listening = Some(listening);
                return Ok(running);
            }
            Err(_) if tries < LISTEN_TRIES => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(error) => {
                eprintln!("mdoc: the AI connector can't listen at {endpoint}: {error}");
                return Err(failed_start(&error));
            }
        }
    }
}

/// A failed start in Settings' words (#1220, decided by default on #1091).
pub(crate) fn failed_start(error: &std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::AddrInUse {
        return "Couldn't start the connector: another copy of mdoc is using it. Quit the other \
                copy, then turn AI connector (preview) off and on."
            .into();
    }
    let why = error.to_string();
    let why = why.trim_end().trim_end_matches('.');
    format!(
        "Couldn't start the connector: {why}. Turn AI connector (preview) off and on to try \
         again."
    )
}

/// How many times [`start`] tries to listen, 50 ms apart.
const LISTEN_TRIES: u32 = 10;

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
            Done::Text { doc_id, .. } | Done::Outline { doc_id, .. } | Done::Part { doc_id, .. },
        ) => Some(Event::Activity {
            document: doc_id.clone(),
        }),
        Answer::Ok(Done::Delivered {
            reply, document, ..
        }) => Some(Event::Delivered {
            app,
            reply: reply.clone(),
            document: document.clone(),
        }),
        Answer::Ok(Done::NeedsReview { doc_id, label, .. }) => Some(Event::NeedsReview {
            app,
            document: doc_id.clone(),
            label: label.clone(),
        }),
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
    match product_name(app) {
        Some(name) => name.into(),
        None if app.is_empty() => "An AI app".into(),
        None => app.into(),
    }
}

/// The product an AI app's own name (`clientInfo.name`) stands for, when
/// mdoc knows it; "An AI app" for none. A system notification names only
/// these: any other name is the client's free text (item 9 review).
pub(crate) fn product_name(app: &str) -> Option<&'static str> {
    match app.trim().to_ascii_lowercase().as_str() {
        "" | "ai app" => Some("An AI app"),
        "claude-ai" | "claude" => Some("Claude"),
        "claude-code" => Some("Claude Code"),
        "chatgpt" | "openai-chatgpt" => Some("ChatGPT"),
        "codex" | "codex-mcp-client" => Some("Codex"),
        _ => None,
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
        assert!(
            start(vault.clone(), &off, Some(&endpoint))
                .unwrap()
                .is_none()
        );
        assert!(matches!(ask(&endpoint), Err(ipc::CallError::NotListening)));
        let on = AiConnectorConfig {
            preview: true,
            ..Default::default()
        };
        let running = start(vault, &on, Some(&endpoint)).unwrap().unwrap();
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

    /// Turned off and on again quickly (Settings › AI connector), the
    /// listener that just stopped may hold the name a moment longer: the
    /// new one waits for it rather than giving up.
    #[test]
    fn a_name_freed_moments_later_is_taken() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::open(dir.path().join("vault"), &[3; 32]).unwrap());
        let endpoint = Endpoint::for_profile(Some(&dir.path().join("profile"))).unwrap();
        let held = ipc::listen(&endpoint, Arc::new(|_: &[u8]| Vec::new())).unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            drop(held);
        });
        let on = AiConnectorConfig {
            preview: true,
            ..Default::default()
        };
        let running = start(vault, &on, Some(&endpoint));
        release.join().unwrap();
        assert!(
            matches!(running, Ok(Some(_))),
            "gave up while the old listener stopped"
        );
        assert_eq!(
            ask(&endpoint).unwrap(),
            Answer::Refused(Refusal::NothingShared)
        );
    }

    /// #1220: a name another mdoc holds is a failed start, in Settings'
    /// words, not silence.
    #[test]
    fn a_name_held_elsewhere_is_a_failed_start() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::open(dir.path().join("vault"), &[3; 32]).unwrap());
        let endpoint = Endpoint::for_profile(Some(&dir.path().join("profile"))).unwrap();
        let _other = ipc::listen(&endpoint, Arc::new(|_: &[u8]| Vec::new())).unwrap();
        let on = AiConnectorConfig {
            preview: true,
            ..Default::default()
        };
        let Err(why) = start(vault.clone(), &on, Some(&endpoint)) else {
            panic!("started on a name another mdoc holds");
        };
        assert_eq!(
            why,
            "Couldn't start the connector: another copy of mdoc is using it. Quit the other \
             copy, then turn AI connector (preview) off and on."
        );
        let Err(why) = start(vault, &on, None) else {
            panic!("started with no endpoint");
        };
        assert!(why.starts_with("Couldn't start the connector: "), "{why}");
        let other = std::io::Error::other("Access is denied. (os error 5)");
        assert_eq!(
            failed_start(&other),
            "Couldn't start the connector: Access is denied. (os error 5). Turn AI connector \
             (preview) off and on to try again."
        );
    }

    /// #1220: a read of a Ready document, held at a hook once its text is
    /// ready, while the preview turns off, then released, writes nothing
    /// to the pipe.
    #[test]
    fn a_read_held_across_off_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::open(dir.path().join("vault"), &[3; 32]).unwrap());
        let endpoint = Endpoint::for_profile(Some(&dir.path().join("profile"))).unwrap();
        let on = AiConnectorConfig {
            preview: true,
            ..Default::default()
        };
        let mut running = start_unlistened(vault.clone(), &on).unwrap();
        let project = vault.create_project(Default::default()).unwrap();
        let document = vault
            .create_document(mdoc_vault::NewDocument {
                project: Some(project.id.clone()),
                title: "Lease memo".into(),
                source_kind: mdoc_vault::SourceKind::Markdown,
                origin: mdoc_vault::DocumentOrigin::Ordinary,
                markdown: "PERSON_1 rents.".into(),
            })
            .unwrap();
        vault
            .update_document(&document.id, |d| {
                d.scanned = true;
                d.set_ready(true);
            })
            .unwrap();
        running.connector.share(&project.id).unwrap();
        // The hook: the answer is made, then waits for its release.
        let (made, answered) = channel();
        let (release, released) = channel::<()>();
        let released = std::sync::Mutex::new(released);
        let inner = running.answer.clone();
        running.answer = Arc::new(move |line: &[u8]| {
            let answer = inner(line);
            let _ = made.send(answer.clone());
            let _ = released.lock().unwrap().recv();
            answer
        });
        let running = listen(running, &endpoint).unwrap();
        let caller = {
            let endpoint = endpoint.clone();
            std::thread::spawn(move || {
                let request = Request {
                    v: mdoc_mcp::wire::VERSION,
                    app: "claude-ai".into(),
                    call: Call::GetDocument {
                        doc_id: document.id,
                        part: None,
                    },
                };
                ipc::call(&endpoint, &serde_json::to_vec(&request).unwrap())
            })
        };
        let held = (answered.recv_timeout(std::time::Duration::from_secs(10))).unwrap();
        let held: Answer = serde_json::from_slice(&held).unwrap();
        assert!(matches!(held, Answer::Ok(Done::Text { .. })), "{held:?}");
        // The preview turns off, as Settings' switch does.
        drop(running);
        // Let go at once, while the text is still held.
        if let Ok(written) = caller.join().unwrap() {
            assert!(
                written.is_empty(),
                "written after off: {}",
                String::from_utf8_lossy(&written)
            );
        }
        release.send(()).unwrap();
    }

    /// Only a call that found nothing shared asks; a bad request or a
    /// refusal of another kind tells the window nothing.
    #[test]
    fn only_a_call_finding_nothing_shared_asks() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::open(dir.path().join("vault"), &[3; 32]).unwrap());
        let on = AiConnectorConfig {
            preview: true,
            ..Default::default()
        };
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
        let on = AiConnectorConfig {
            preview: true,
            ..Default::default()
        };
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
        let connector = running.connector.clone();
        drop(running);
        shares_nothing_soon(&connector);
        assert_eq!(answer(&list), Answer::Refused(Refusal::NothingShared));
    }

    /// Off shares nothing, off the window's thread: a moment later.
    fn shares_nothing_soon(connector: &Connector) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while connector.ai_project().is_some() {
            assert!(std::time::Instant::now() < deadline, "still shared");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
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
