//! Quitting: each tab queued is asked in turn, then the quit is the
//! writer's last job (ADR 0130): documents, Projects and the session are
//! written and the window closes in one turn. The workspace view asks,
//! writes and closes the window; this owner decides what comes next.
use crate::document::comparison_ui;
use crate::workspace::{
    Workspace, replies, saving, saving::session_writer::SessionEffect, sidebar::navigation::Arrival,
};
use gpui::{App, Context, Window};
use mdoc_store::session_store;

use std::{cell::Cell, collections::VecDeque, rc::Rc};

/// Whether a close or quit waits on a document's answer, shared read-only
/// with the documents: an import finishing then waits too (one
/// channel each way).
#[derive(Clone, Default)]
pub(crate) struct CloseWatch(Rc<Cell<bool>>);

impl CloseWatch {
    pub(crate) fn resolving(&self) -> bool {
        self.0.get()
    }
}

/// What the workspace view carries out for a quit-flow transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum QuitEffect {
    /// Activate tab `id` and ask about it before the quit goes on.
    Resolve(u64),
    /// Every tab is resolved: activate `restore` (the tab active when the
    /// quit began) and queue the quit as the writer's last job.
    Settle { restore: Option<u64> },
}

#[derive(Default)]
pub(in crate::workspace) struct QuitFlow {
    /// Tabs still to ask, while quitting.
    queue: Option<VecDeque<u64>>,
    /// The tab active when the quit began; it is active again at the end.
    restore: Option<u64>,
    /// Every tab resolved: the final save and the window close run.
    finishing: bool,
    /// `resolving()`, as the documents read it.
    watch: CloseWatch,
}

impl QuitFlow {
    pub(in crate::workspace) fn quitting(&self) -> bool {
        self.queue.is_some()
    }

    pub(in crate::workspace) fn finishing(&self) -> bool {
        self.finishing
    }

    /// A quit is waiting on an answer.
    pub(in crate::workspace) fn resolving(&self) -> bool {
        self.queue.is_some()
    }

    /// `resolving()` for the documents, kept current by every transition.
    pub(in crate::workspace) fn watch(&self) -> CloseWatch {
        self.watch.clone()
    }

    fn publish(&self) {
        self.watch.0.set(self.resolving());
    }

    /// Whether a quit may begin now.
    pub(in crate::workspace) fn may_quit(&self) -> bool {
        self.queue.is_none() && !self.finishing
    }

    /// Whether a single tab may be closed now.
    pub(in crate::workspace) fn may_close(&self) -> bool {
        !self.resolving() && !self.finishing
    }

    /// Quit with tab `active` in front: ask the `at_risk` tabs in turn.
    pub(in crate::workspace) fn quit(&mut self, active: u64, at_risk: VecDeque<u64>) -> QuitEffect {
        self.restore = Some(active);
        self.queue = Some(at_risk);
        self.publish();
        self.step()
    }

    /// The next tab to ask, or the end of the questions; nothing unless
    /// quitting.
    pub(in crate::workspace) fn advance(&mut self) -> Option<QuitEffect> {
        self.queue.is_some().then(|| self.step())
    }

    fn step(&mut self) -> QuitEffect {
        match self.queue.as_mut().and_then(VecDeque::pop_front) {
            Some(id) => QuitEffect::Resolve(id),
            None => QuitEffect::Settle {
                restore: self.restore.take(),
            },
        }
    }

    /// The questions are over: the final writes start.
    pub(in crate::workspace) fn settle(&mut self) {
        self.finishing = true;
    }

    /// A tab was answered for: the next one, while quitting.
    pub(in crate::workspace) fn resolved(&mut self) -> Option<QuitEffect> {
        self.advance()
    }

    /// The quit stops: a write failed. Returns whether it was finishing.
    pub(in crate::workspace) fn abandon(&mut self) -> bool {
        let finishing = std::mem::take(&mut self.finishing);
        self.queue = None;
        self.publish();
        finishing
    }
}

impl Workspace {
    /// The window's close button: quit, or with the menu bar item on, hide
    /// the window with every tab kept in memory. A quit already
    /// asking goes on.
    pub(crate) fn close_requested(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::shelf::menu_bar::keeps_running(cx) && self.quit_flow.may_quit() {
            crate::shelf::menu_bar::close_window(window, cx);
        } else {
            self.quit(window, cx);
        }
    }

    pub(crate) fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready || !self.quit_flow.may_quit() {
            return;
        }
        if self.tabs.iter().any(|tab| {
            tab.view()
                .is_some_and(|view| view.read(cx).lifecycle.prompting())
        }) {
            return;
        }
        // Documents and their names save themselves: nothing is asked.
        self.start_quit(VecDeque::new(), window, cx);
    }

    /// A quit from outside the app (macOS: the Dock, an Apple Event,
    /// logout): Quit's questions. Returns whether a quit is underway; when
    /// another question blocks it, mdoc comes to the front to show it.
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn quit_from_system(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.quit(window, cx);
        let underway = !self.quit_flow.may_quit();
        if !underway {
            cx.activate(true);
            window.activate_window();
        }
        underway
    }

    /// The user cancelled a quit: no relaunch, and a system quit waiting on
    /// it is refused.
    pub(crate) fn quit_cancelled(cx: &mut App) {
        crate::app::updater::cancel_restart(cx);
        #[cfg(any(target_os = "macos", test))]
        crate::app::system_quit::cancelled(cx);
    }

    /// Ask each tab in `at_risk` in turn, then quit.
    pub(in crate::workspace) fn start_quit(
        &mut self,
        at_risk: VecDeque<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effect = self.quit_flow.quit(self.tabs.active(), at_risk);
        self.carry_out_quit(effect, window, cx);
    }

    pub(in crate::workspace) fn carry_out_quit(
        &mut self,
        effect: QuitEffect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            QuitEffect::Resolve(_) => {
                if let Some(effect) = self.quit_flow.resolved() {
                    self.carry_out_quit(effect, window, cx);
                }
            }
            QuitEffect::Settle { restore } => {
                if let Some(active) = restore {
                    self.activate_as(active, Arrival::Unrecorded, window, cx);
                }
                self.queue_quit(window, cx);
            }
        }
    }

    /// The questions are over: the quit is the writer's last job (ADR
    /// 0130). Editing goes on while the jobs before it run; it reads
    /// everything when it reaches the head.
    pub(in crate::workspace) fn queue_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.quit_flow.settle();
        self.queue_write(
            saving::document_writer::Target::Quit,
            saving::autosave::Write::Quit,
            window,
            cx,
        );
    }

    /// The quit at the head of the writer's queue, all in this one turn:
    /// what every tab shows (its names, its text, its notes, its reply
    /// box), every Project whose names wait, the tab list and the Vault's
    /// summaries are written, and the window closes, so no keystroke can
    /// land between what is written and the close. Anything that can't be
    /// written cancels the quit, its edit still in its tab.
    pub(in crate::workspace) fn run_quit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> saving::autosave::Ran {
        // Cancelled while it waited: a job before it failed.
        if !self.quit_flow.finishing() {
            return saving::autosave::Ran::Ended;
        }
        // A tab list write still running would land after this one.
        if self.session_writer.writing() {
            self.quit_waits_for_session = true;
            return saving::autosave::Ran::Holds;
        }
        let ids: Vec<u64> = self.tabs.iter().map(|tab| tab.id()).collect();
        for view in self.tabs.iter().filter_map(|tab| tab.view().cloned()) {
            view.update(cx, |view, _| view.cancel_autosave());
        }
        if let Some(vault) = self.vault.vault().cloned() {
            for id in ids {
                if let Err(error) = self.quit_writes(id, &vault, window, cx) {
                    self.unsaved.insert(id);
                    let name = (self.tab_view(id)).map(|view| view.read(cx).display_name());
                    self.notices.post(format!(
                        "Could not save \u{201c}{}\u{201d}: {error}",
                        name.unwrap_or_default()
                    ));
                    self.quit_failed(window, cx);
                    return saving::autosave::Ran::Ended;
                }
                // Its notes or its reply failed, and said so.
                if !self.quit_flow.finishing() {
                    return saving::autosave::Ran::Ended;
                }
            }
            let projects: Vec<String> = (self.projects.summaries().into_iter())
                .map(|summary| summary.id)
                .filter(|id| {
                    self.project_ui.is_unwritten(id)
                        || self
                            .document_writer
                            .has(&saving::document_writer::Target::Project(id.clone()))
                })
                .collect();
            for id in projects {
                if let Err(error) = self.write_now(&vault, &[Some(id.clone())]) {
                    self.project_ui.mark_unwritten(&id);
                    let name =
                        (self.projects.project(&id)).map_or_else(String::new, |p| p.name.clone());
                    self.notices.post(format!(
                        "Could not save the Project \u{201c}{name}\u{201d}: {error}"
                    ));
                    self.quit_failed(window, cx);
                    return saving::autosave::Ran::Ended;
                }
                self.project_ui.take_unwritten(&id);
            }
        }
        // The tab list as it stands now that every document has its id.
        if self.session_writer.enabled()
            && let Some(SessionEffect::Write { path, snapshot }) =
                self.session_writer.checkpoint(self.snapshot(cx))
        {
            match session_store::save(&path, &snapshot) {
                Ok(()) => {
                    let _ = self.session_writer.saved(snapshot);
                }
                Err(error) => {
                    self.session_writer.failed();
                    self.session_failed(error.to_string(), window, cx);
                    return saving::autosave::Ran::Ended;
                }
            }
        }
        if let Some(vault) = self.vault.vault()
            && let Err(error) = vault.flush()
        {
            self.notices
                .post(format!("Could not save the document list: {error}"));
        }
        self.close_window(window, cx);
        // The window is gone: nothing after the last job runs.
        saving::autosave::Ran::Holds
    }

    /// The quit's writes of tab `id`: its document (made now if new), its
    /// notes and the reply in its box.
    pub(in crate::workspace) fn quit_writes(
        &mut self,
        id: u64,
        vault: &mdoc_vault::Vault,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.tab_document(id, cx).is_none() {
            self.create_tab_now(id, vault, cx)?;
        } else {
            self.write_tab_now(id, vault, cx)
                .map_err(|error| error.to_string())?;
        }
        let Some(document) = self.tab_document(id, cx) else {
            return Ok(());
        };
        let Some(view) = self.tab_view(id) else {
            return Ok(());
        };
        if view.read(cx).notes.needs_write() {
            self.run_notes_save(&document, window, cx);
        }
        if self.quit_flow.finishing() && view.read(cx).reply.unsaved().is_some() {
            let save = replies::ReplySave::Box {
                tab: id,
                open: false,
            };
            self.run_reply_save(save, window, cx);
        }
        Ok(())
    }

    /// A write the quit needed failed: the window stays, with the edit.
    pub(in crate::workspace) fn quit_failed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quit_flow.abandon() {
            Self::quit_cancelled(cx);
            crate::shelf::menu_bar::reveal_if_hidden(window, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn close_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.update(cx, |panel, cx| panel.cancel_setup(cx));
        for tab in &self.tabs {
            if let Some(view) = tab.view() {
                Self::release(view, window, cx);
            }
        }
        // Viewing copies leave with the window; launch clears any it misses.
        if let Some(root) = &self.viewing_root {
            let _ = std::fs::remove_dir_all(root);
        }
        for handle in cx
            .windows()
            .into_iter()
            .filter_map(|w| w.downcast::<comparison_ui::View>())
        {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        window.remove_window();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quit_asks_each_tab_at_risk_then_settles_on_the_first_active_one() {
        let mut quit = QuitFlow::default();
        assert!(quit.may_quit() && quit.may_close());
        assert_eq!(quit.quit(5, [2, 3].into()), QuitEffect::Resolve(2));
        assert!(quit.quitting() && quit.resolving() && !quit.may_quit());
        assert!(!quit.may_close());
        assert_eq!(quit.resolved(), Some(QuitEffect::Resolve(3)));
        assert_eq!(
            quit.resolved(),
            Some(QuitEffect::Settle { restore: Some(5) })
        );
        quit.settle();
        assert!(quit.finishing() && !quit.may_quit());
    }

    #[test]
    fn a_failed_save_ends_the_quit() {
        let mut quit = QuitFlow::default();
        quit.quit(1, VecDeque::new());
        quit.settle();
        assert!(quit.watch().resolving());
        assert!(quit.abandon());
        assert!(quit.may_quit() && !quit.watch().resolving());
        assert!(!quit.abandon());
    }
}
