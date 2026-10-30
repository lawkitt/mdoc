//! A quit from outside mdoc on macOS: the Dock's Quit, a quit Apple Event,
//! logout. AppKit asks first (`applicationShouldTerminate:`, a vendored
//! patch, see vendor/gpui-pre-macos/MDOC-PATCH.md). mdoc answers "later"
//! and asks the same questions as Quit (ADR 0071): the answer is yes once
//! the quit flow closes the window, and no if a question is cancelled, so a
//! logout or shutdown that asked goes on or stops with it.

use crate::workspace::Workspace;
use gpui::{App, AsyncApp, Global, WindowHandle};
use std::{cell::Cell, rc::Rc};

/// The workspace window and its app, once the window is open.
pub(crate) type QuitTarget = (WindowHandle<Workspace>, AsyncApp);

/// AppKit's question, answered now or later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    Now,
    Later,
}

/// mdoc's side of AppKit's terminate question, kept as a global so the quit
/// flow can answer it.
#[derive(Clone)]
pub(crate) struct QuitGate(Rc<GateState>);

struct GateState {
    /// mdoc quits by itself: the terminate that follows goes through.
    open: Cell<bool>,
    /// AppKit waits for `reply`.
    waiting: Cell<bool>,
    reply: Box<dyn Fn(bool)>,
}

impl Global for QuitGate {}

impl QuitGate {
    /// `reply` answers a question this gate deferred: `true` terminates.
    pub(crate) fn new(reply: impl Fn(bool) + 'static) -> Self {
        Self(Rc::new(GateState {
            open: Cell::new(false),
            waiting: Cell::new(false),
            reply: Box::new(reply),
        }))
    }

    /// AppKit asks whether to terminate. Unless mdoc quits by itself, defer
    /// the answer and start the quit flow in `workspace` on the foreground
    /// executor, since this runs inside an AppKit callback. Before the
    /// window opens there is nothing to lose.
    pub(crate) fn should_terminate(&self, workspace: Option<QuitTarget>) -> Answer {
        if self.0.open.get() {
            return Answer::Now;
        }
        let Some((handle, mut cx)) = workspace else {
            return Answer::Now;
        };
        self.0.waiting.set(true);
        let gate = self.clone();
        cx.foreground_executor()
            .clone()
            .spawn(async move {
                match handle.update(&mut cx, |workspace, window, cx| {
                    workspace.quit_from_system(window, cx)
                }) {
                    Ok(true) => {}
                    // A question not about quitting is open: answer it first.
                    Ok(false) => gate.answer(false),
                    // The window is gone: nothing is left to ask about.
                    Err(_) => gate.answer(true),
                }
            })
            .detach();
        Answer::Later
    }

    /// Whether AppKit waits for the quit flow's answer.
    pub(crate) fn waiting(&self) -> bool {
        self.0.waiting.get()
    }

    /// The last window closed. Returns true when that answers AppKit (it
    /// then terminates); otherwise mdoc quits by itself and the gate opens.
    pub(crate) fn last_window_closed(&self) -> bool {
        if self.waiting() {
            self.answer(true);
            true
        } else {
            self.0.open.set(true);
            false
        }
    }

    fn answer(&self, terminate: bool) {
        if self.0.waiting.replace(false) {
            (self.0.reply)(terminate);
        }
    }
}

/// The user cancelled a quit: a system quit waiting on it is refused.
pub(crate) fn cancelled(cx: &App) {
    if let Some(gate) = cx.try_global::<QuitGate>() {
        gate.answer(false);
    }
}

/// Whether a system quit waits on the quit flow: its questions come to the
/// front, since mdoc may be in the background.
pub(crate) fn waiting(cx: &App) -> bool {
    cx.try_global::<QuitGate>().is_some_and(QuitGate::waiting)
}

/// Answer AppKit's terminate question with `gate`, for the workspace in
/// `workspace` once its window is open.
#[cfg(target_os = "macos")]
pub(crate) fn install(gate: &QuitGate, workspace: Rc<std::cell::RefCell<Option<QuitTarget>>>) {
    let gate = gate.clone();
    gpui_macos::set_should_terminate_handler(Some(Box::new(move || {
        let workspace = workspace.try_borrow().ok().and_then(|open| open.clone());
        match gate.should_terminate(workspace) {
            Answer::Now => gpui_macos::TerminateReply::Now,
            Answer::Later => gpui_macos::TerminateReply::Later,
        }
    })));
}
