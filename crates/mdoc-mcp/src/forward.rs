//! The shim's side of each call: the wire request to the running mdoc of
//! this profile, starting it once when nothing listens.
use crate::{
    ipc::{self, CallError, Endpoint},
    launch,
    wire::{Answer, Request},
};
use std::{
    path::PathBuf,
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
}

/// How long a call waits for mdoc it started: under Codex's 60 s.
const WAIT: Duration = Duration::from_secs(20);

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
}

impl Backend for Forwarder {
    fn call(&mut self, request: &Request) -> Result<Answer, Unreachable> {
        let endpoint = self.endpoint.clone().ok_or(Unreachable::NotRunning)?;
        let bytes = serde_json::to_vec(request).map_err(|_| Unreachable::Failed)?;
        let mut until = None;
        let answer = loop {
            match ipc::call(&endpoint, &bytes) {
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
