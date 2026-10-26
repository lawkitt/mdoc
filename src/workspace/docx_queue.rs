//! DOCX conversions waiting for the one supervised worker (ADR 0005): tabs
//! queue their jobs, one runs at a time, and cancelled jobs or jobs of
//! closed tabs are skipped. The workspace view hands the next job to its
//! document, which runs it and reports back.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// A tab's request to convert `path`, valid for its `generation`.
#[derive(Clone, Debug)]
pub(super) struct DocxJob {
    pub(super) id: u64,
    pub(super) path: PathBuf,
    pub(super) generation: u64,
    pub(super) cancel: Arc<AtomicBool>,
}

/// What the workspace view carries out for a queue transition.
#[derive(Debug)]
pub(super) enum DocxEffect {
    /// Run this job in its tab's document.
    Run(DocxJob),
}

#[derive(Default)]
pub(super) struct DocxQueue {
    waiting: VecDeque<DocxJob>,
    /// The tab and generation of the job running now.
    running: Option<(u64, u64)>,
}

impl DocxQueue {
    pub(super) fn push(&mut self, job: DocxJob) {
        self.waiting.push_back(job);
    }

    /// The next job to run, unless one is running. Cancelled jobs and jobs
    /// whose tab has no document (`open` is false) are dropped.
    pub(super) fn next(&mut self, open: impl Fn(u64) -> bool) -> Option<DocxEffect> {
        if self.running.is_some() {
            return None;
        }
        while let Some(job) = self.waiting.pop_front() {
            if job.cancel.load(Ordering::Relaxed) || !open(job.id) {
                continue;
            }
            self.running = Some((job.id, job.generation));
            return Some(DocxEffect::Run(job));
        }
        None
    }

    /// Tab `id`'s job for `generation` ended; a stale report changes
    /// nothing.
    pub(super) fn finished(&mut self, id: u64, generation: u64) {
        if self.running == Some((id, generation)) {
            self.running = None;
        }
    }

    /// Tab `id` closed: its waiting jobs go. A running job ends by itself.
    pub(super) fn forget(&mut self, id: u64) {
        self.waiting.retain(|job| job.id != id);
    }

    /// Nothing waits and nothing runs.
    #[cfg(test)]
    pub(super) fn idle(&self) -> bool {
        self.waiting.is_empty() && self.running.is_none()
    }

    /// Pretend tab `id`'s job for `generation` is running.
    #[cfg(test)]
    pub(super) fn occupy(&mut self, id: u64, generation: u64) {
        self.running = Some((id, generation));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: u64, generation: u64) -> DocxJob {
        DocxJob {
            id,
            path: format!("/contracts/{id}.docx").into(),
            generation,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn ran(effect: Option<DocxEffect>) -> Option<(u64, u64)> {
        effect.map(|DocxEffect::Run(job)| (job.id, job.generation))
    }

    #[test]
    fn one_job_runs_at_a_time_in_order() {
        let mut queue = DocxQueue::default();
        queue.push(job(1, 1));
        queue.push(job(2, 1));
        assert_eq!(ran(queue.next(|_| true)), Some((1, 1)));
        assert_eq!(ran(queue.next(|_| true)), None);
        // A stale report leaves the running job in place.
        queue.finished(1, 0);
        assert_eq!(ran(queue.next(|_| true)), None);
        queue.finished(1, 1);
        assert_eq!(ran(queue.next(|_| true)), Some((2, 1)));
        queue.finished(2, 1);
        assert!(queue.idle());
    }

    #[test]
    fn cancelled_jobs_and_closed_tabs_are_skipped() {
        let mut queue = DocxQueue::default();
        let cancelled = job(1, 1);
        cancelled.cancel.store(true, Ordering::Relaxed);
        queue.push(cancelled);
        queue.push(job(2, 1));
        queue.push(job(3, 1));
        queue.push(job(4, 1));
        queue.forget(3);
        assert_eq!(ran(queue.next(|id| id != 2)), Some((4, 1)));
        queue.finished(4, 1);
        assert!(queue.idle());
        queue.occupy(9, 1);
        assert!(!queue.idle());
    }
}
