//! Quitting: each tab queued is asked in turn, then the quit is the
//! writer's last job (ADR 0130): documents, Projects and the session are
//! written and the window closes in one turn. The workspace view asks,
//! writes and closes the window; this owner decides what comes next.

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
