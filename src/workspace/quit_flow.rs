//! Quitting: each tab queued is asked in turn, then documents, Projects and
//! the session are written and the window closes. The workspace view asks,
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
pub(super) enum QuitEffect {
    /// Activate tab `id` and ask about it before the quit goes on.
    Resolve(u64),
    /// Every tab is resolved: activate `restore` (the tab active when the
    /// quit began), write documents and the session and, once no write
    /// runs, finish.
    Settle { restore: Option<u64> },
}

#[derive(Default)]
pub(super) struct QuitFlow {
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
    pub(super) fn quitting(&self) -> bool {
        self.queue.is_some()
    }

    pub(super) fn finishing(&self) -> bool {
        self.finishing
    }

    /// A quit is waiting on an answer.
    pub(super) fn resolving(&self) -> bool {
        self.queue.is_some()
    }

    /// `resolving()` for the documents, kept current by every transition.
    pub(super) fn watch(&self) -> CloseWatch {
        self.watch.clone()
    }

    fn publish(&self) {
        self.watch.0.set(self.resolving());
    }

    /// Whether a quit may begin now.
    pub(super) fn may_quit(&self) -> bool {
        self.queue.is_none() && !self.finishing
    }

    /// Whether a single tab may be closed now.
    pub(super) fn may_close(&self) -> bool {
        !self.resolving() && !self.finishing
    }

    /// Quit with tab `active` in front: ask the `at_risk` tabs in turn.
    pub(super) fn quit(&mut self, active: u64, at_risk: VecDeque<u64>) -> QuitEffect {
        self.restore = Some(active);
        self.queue = Some(at_risk);
        self.publish();
        self.step()
    }

    /// The next tab to ask, or the end of the questions; nothing unless
    /// quitting.
    pub(super) fn advance(&mut self) -> Option<QuitEffect> {
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
    pub(super) fn settle(&mut self) {
        self.finishing = true;
    }

    /// A tab was answered for: the next one, while quitting.
    pub(super) fn resolved(&mut self) -> Option<QuitEffect> {
        self.advance()
    }

    /// The quit stops: a write failed. Returns whether it was finishing.
    pub(super) fn abandon(&mut self) -> bool {
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
