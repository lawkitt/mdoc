//! A quit from outside mdoc on macOS: the Dock's Quit, a quit Apple Event,
//! logout. AppKit asks first (`applicationShouldTerminate:`, a vendored
//! patch, see vendor/gpui-pre-macos/MDOC-PATCH.md); mdoc says no and asks
//! the same questions as Quit (ADR 0071). Their end closes the window, and
//! that quit terminates for real.

use crate::workspace::Workspace;
use gpui::{AsyncApp, WindowHandle};
use std::{cell::Cell, rc::Rc};

/// The workspace window and its app, once the window is open.
pub(crate) type QuitTarget = (WindowHandle<Workspace>, AsyncApp);

/// Whether the next terminate goes through: once mdoc itself quits or
/// restarts.
#[derive(Clone, Default)]
pub(crate) struct QuitGate(Rc<Cell<bool>>);

impl QuitGate {
    /// mdoc quits now: the terminate that follows goes through.
    pub(crate) fn open(&self) {
        self.0.set(true);
    }

    /// AppKit asks whether to terminate. Unless the gate is open, answer no
    /// and start the quit flow in `workspace` on the foreground executor,
    /// since this runs inside an AppKit callback. Before the window opens
    /// there is nothing to lose.
    pub(crate) fn should_terminate(&self, workspace: Option<QuitTarget>) -> bool {
        if self.0.get() {
            return true;
        }
        let Some((handle, mut cx)) = workspace else {
            return true;
        };
        let gate = self.clone();
        cx.foreground_executor()
            .clone()
            .spawn(async move {
                let asked = handle.update(&mut cx, |workspace, window, cx| {
                    workspace.quit_from_system(window, cx)
                });
                if asked.is_err() {
                    // The window is gone: nothing is left to ask about.
                    gate.open();
                    cx.update(|cx| cx.quit());
                }
            })
            .detach();
        false
    }
}

/// Answer AppKit's terminate question with `gate`, for the workspace in
/// `workspace` once its window is open.
#[cfg(target_os = "macos")]
pub(crate) fn install(gate: &QuitGate, workspace: Rc<std::cell::RefCell<Option<QuitTarget>>>) {
    let gate = gate.clone();
    gpui_macos::set_should_terminate_handler(Some(Box::new(move || {
        let workspace = workspace.try_borrow().ok().and_then(|open| open.clone());
        gate.should_terminate(workspace)
    })));
}
