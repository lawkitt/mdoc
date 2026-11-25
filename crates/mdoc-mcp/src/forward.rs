//! The shim's side of each call: the wire request to the running mdoc of
//! this profile, starting it once when nothing listens. A call that mdoc
//! doesn't answer in time is given up, so the shim keeps answering.
use crate::{
    ipc::{self, CallError, Endpoint},
    launch,
    wire::{Answer, Request},
};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

/// Why a call reached no mdoc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unreachable {
    /// Nothing listened, even after starting mdoc.
    NotRunning,
    /// What listens isn't this user's: nothing was sent.
    NotOurs,
    /// The call broke, or its answer didn't parse.
    Failed,
    /// mdoc took the call and didn't answer in time: what it did is not
    /// known.
    TimedOut,
}

/// Where calls go, as an MCP server needs it.
pub trait Backend {
    fn call(&mut self, request: &Request) -> Result<Answer, Unreachable>;
}

pub struct Forwarder {
    endpoint: Option<Endpoint>,
    /// The shim's own path, which mdoc is found beside.
    shim: Option<PathBuf>,
    dev_profile: Option<PathBuf>,
    launched: Option<Instant>,
    answer_within: Duration,
}

/// How long a call waits for mdoc it started.
const WAIT: Duration = Duration::from_secs(20);

/// How long mdoc has to answer a call it took. With [`WAIT`], under
/// Codex's 60 s.
const ANSWER_WITHIN: Duration = Duration::from_secs(30);

/// mdoc is started at most this often.
const RESTART: Duration = Duration::from_secs(60);

impl Forwarder {
    /// This process's profile, endpoint and path.
    pub fn from_env() -> Self {
        let dev_profile = ipc::dev_profile_from_env();
        Self {
            endpoint: Endpoint::for_profile(dev_profile.as_deref()),
            shim: std::env::current_exe().ok(),
            dev_profile,
            launched: None,
            answer_within: ANSWER_WITHIN,
        }
    }

    /// Start mdoc unless it was started within [`RESTART`]: when the
    /// last start was, if there was one.
    fn started(&mut self) -> Option<Instant> {
        if self.launched.is_some_and(|at| at.elapsed() < RESTART) {
            return self.launched;
        }
        let command = (self.shim.as_deref())
            .and_then(|shim| launch::command(shim, self.dev_profile.as_deref()));
        match launch::start(command?) {
            Ok(()) => self.launched = Some(Instant::now()),
            Err(error) => eprintln!("mdoc-mcp: could not start mdoc: {error}"),
        }
        self.launched
    }

    /// One call, given up after `answer_within`: `None` then. The call's
    /// thread is left to end with its connection.
    fn call_once(&self, endpoint: &Endpoint, bytes: &[u8]) -> Option<Result<Vec<u8>, CallError>> {
        let (tx, rx) = mpsc::channel();
        let (endpoint, bytes) = (endpoint.clone(), bytes.to_vec());
        std::thread::Builder::new()
            .name("mdoc-mcp-call".into())
            .spawn(move || {
                let _ = tx.send(ipc::call(&endpoint, &bytes));
            })
            .ok()?;
        rx.recv_timeout(self.answer_within).ok()
    }
}

impl Backend for Forwarder {
    fn call(&mut self, request: &Request) -> Result<Answer, Unreachable> {
        let endpoint = self.endpoint.clone().ok_or(Unreachable::NotRunning)?;
        let bytes = serde_json::to_vec(request).map_err(|_| Unreachable::Failed)?;
        let mut until = None;
        let answer = loop {
            let Some(result) = self.call_once(&endpoint, &bytes) else {
                return Err(Unreachable::TimedOut);
            };
            match result {
                Ok(answer) => break answer,
                Err(CallError::NotListening) => {
                    if until.is_none() {
                        until = self.started().map(|at| at + WAIT);
                    }
                    if until.is_none_or(|until| Instant::now() >= until) {
                        return Err(Unreachable::NotRunning);
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
                Err(CallError::NotOurs) => return Err(Unreachable::NotOurs),
                Err(CallError::Io(error)) => {
                    eprintln!("mdoc-mcp: the call to mdoc broke: {error}");
                    return Err(Unreachable::Failed);
                }
            }
        };
        serde_json::from_slice(&answer).map_err(|_| Unreachable::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{Call, Refusal, VERSION};
    use std::sync::Arc;

    /// Done when: a call mdoc takes and doesn't answer is given up in
    /// time, and the next call is answered.
    #[test]
    fn a_call_mdoc_does_not_answer_is_given_up() {
        let profile = tempfile::tempdir().unwrap();
        let endpoint = Endpoint::for_profile(Some(profile.path())).unwrap();
        let stall: ipc::Handler = Arc::new(|request: &[u8]| {
            if request.windows(5).any(|w| w == b"stall") {
                std::thread::sleep(Duration::from_secs(3));
            }
            serde_json::to_vec(&Answer::Refused(Refusal::NothingShared)).unwrap()
        });
        let _listening = ipc::listen(&endpoint, stall).unwrap();
        let mut forwarder = Forwarder {
            endpoint: Some(endpoint),
            shim: None,
            dev_profile: None,
            launched: None,
            answer_within: Duration::from_millis(300),
        };
        let request = |app: &str| Request {
            v: VERSION,
            app: app.into(),
            call: Call::ListDocuments,
        };
        let started = Instant::now();
        assert_eq!(
            forwarder.call(&request("stall")),
            Err(Unreachable::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(
            forwarder.call(&request("claude-ai")),
            Ok(Answer::Refused(Refusal::NothingShared))
        );
    }
}
