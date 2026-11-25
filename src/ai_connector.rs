//! mdoc's end of the AI connector (ADR 0126; Milestone 2 item 7): while
//! "AI connector (preview)" is on and the Vault is open, the connector
//! service and an owner-only listener for the `mdoc-mcp` shim
//! (`mdoc_mcp::ipc`). Each call is answered by `mdoc_connector::serve`,
//! so every read and delivery is the service's. With the setting off
//! there is no service and no listener at all.
pub(crate) mod setup;

use mdoc_connector::Connector;
use mdoc_mcp::ipc::{self, Endpoint, Listening};
use mdoc_store::settings::AiConnectorConfig;
use mdoc_vault::Vault;
use std::sync::Arc;

/// The connector and its listener; dropping it stops listening.
pub(crate) struct Running {
    /// Shared with the window: what it shares, Stop sharing (item 8);
    /// turning the preview off stops sharing through it.
    pub(crate) connector: Arc<Connector>,
    _listening: Listening,
}

/// Start the connector over `vault` and listen at `endpoint`, while
/// `setting` is on; `None` while it is off, or when the endpoint can't be
/// held (it says why on stderr).
pub(crate) fn start(
    vault: Arc<Vault>,
    setting: &AiConnectorConfig,
    endpoint: &Endpoint,
) -> Option<Running> {
    let connector = Arc::new(Connector::start(vault, setting)?);
    let serving = connector.clone();
    let answer: ipc::Handler =
        Arc::new(move |line: &[u8]| mdoc_connector::serve::answer_line(&serving, line));
    // Turned off and on again quickly, the listener that just stopped may
    // still hold the name for a moment (Settings › AI connector): try again
    // a few times before giving up.
    let mut tries = 1;
    loop {
        match ipc::listen(endpoint, answer.clone()) {
            Ok(listening) => {
                return Some(Running {
                    connector,
                    _listening: listening,
                });
            }
            Err(_) if tries < LISTEN_TRIES => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(error) => {
                eprintln!("mdoc: the AI connector can't listen at {endpoint}: {error}");
                return None;
            }
        }
    }
}

/// How many times [`start`] tries to listen, 50 ms apart.
const LISTEN_TRIES: u32 = 10;

/// This profile's endpoint: the dev profile's when there is one.
pub(crate) fn endpoint() -> Option<Endpoint> {
    Endpoint::for_profile(mdoc_store::app_paths::dev_profile())
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
        let on = AiConnectorConfig { preview: true };
        let running = start(vault, &on, &endpoint);
        release.join().unwrap();
        assert!(running.is_some(), "gave up while the old listener stopped");
        assert_eq!(
            ask(&endpoint).unwrap(),
            Answer::Refused(Refusal::NothingShared)
        );
    }

    /// The shim reads the dev profile as mdoc does.
    #[test]
    fn the_shim_names_the_dev_profile_as_mdoc_does() {
        use mdoc_store::app_paths;
        assert_eq!(ipc::DEV_PROFILE_VAR, app_paths::DEV_PROFILE_VAR);
        assert_eq!(ipc::DEV_PROFILE_HONOURED, app_paths::DEV_PROFILE_HONOURED);
    }
}
