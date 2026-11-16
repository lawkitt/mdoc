//! How long a scan may run, and its pause for heat. Desktop stops a scan
//! two minutes after it starts. A phone lets a scan run to the end and
//! stops only one that makes no progress for a minute, because older
//! phones need several minutes for a long document. While the platform
//! asks the scan to pause (a phone at the critical thermal state), the scan
//! waits between windows and the wait counts toward neither limit.
use super::DetectError;
use std::{
    cell::Cell,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// How long a scan may run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanLimit {
    /// Stop once the scan has run this long.
    Total(Duration),
    /// Stop once the scan has gone this long without finishing a step.
    Stall(Duration),
}

impl ScanLimit {
    /// Desktop's two-minute cooperative deadline.
    pub const DESKTOP: Self = Self::Total(Duration::from_secs(120));
    /// A phone's stall guard.
    pub const PHONE: Self = Self::Stall(Duration::from_secs(60));
}

/// Model windows a scan has finished, out of all of the source's windows. A
/// window repeated word for word runs and counts once. `chars` counts the
/// characters of those windows, so time per character can be measured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanProgress {
    pub done: usize,
    pub windows: usize,
    pub chars_done: usize,
    pub chars: usize,
}

/// What a scan tells the caller and asks it, on the scanning thread.
pub trait ScanObserver {
    /// Once with no window done when the windows are known, then after
    /// each window.
    fn progress(&self, _progress: ScanProgress) {}
    /// Asked between windows: while true the scan waits.
    fn paused(&self) -> bool {
        false
    }
}

/// A closure as an observer that never pauses.
impl<F: Fn(ScanProgress)> ScanObserver for F {
    fn progress(&self, progress: ScanProgress) {
        self(progress)
    }
}

/// How often a paused scan asks again whether it may go on.
const PAUSE_POLL: Duration = Duration::from_millis(500);

/// One scan's clock against its [`ScanLimit`].
pub(crate) struct Clock {
    started: Instant,
    limit: ScanLimit,
    /// When the scan last finished a step or came out of a pause.
    last: Cell<Instant>,
    /// Time spent paused, which the total limit does not count.
    paused: Cell<Duration>,
}

impl Clock {
    pub(crate) fn new(limit: ScanLimit) -> Self {
        Self::starting_at(Instant::now(), limit)
    }

    fn starting_at(started: Instant, limit: ScanLimit) -> Self {
        Self {
            started,
            limit,
            last: Cell::new(started),
            paused: Cell::new(Duration::ZERO),
        }
    }

    /// Cancelled, or past the limit.
    pub(crate) fn check(&self, cancel: &AtomicBool) -> Result<(), DetectError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(DetectError::Cancelled);
        }
        match self.limit {
            ScanLimit::Total(limit) if self.started.elapsed() > limit + self.paused.get() => {
                Err(DetectError::TooSlow)
            }
            ScanLimit::Stall(limit) if self.last.get().elapsed() > limit => {
                Err(DetectError::Stalled)
            }
            _ => Ok(()),
        }
    }

    /// The scan finished a step: the stall guard starts over.
    pub(crate) fn advanced(&self) {
        self.last.set(Instant::now());
    }

    /// Waits while `observer` asks the scan to pause, until cancelled. The
    /// wait counts toward neither limit.
    pub(crate) fn wait_out_pause(
        &self,
        cancel: &AtomicBool,
        observer: &dyn ScanObserver,
    ) -> Result<(), DetectError> {
        self.wait_out_pause_every(cancel, observer, PAUSE_POLL)
    }

    fn wait_out_pause_every(
        &self,
        cancel: &AtomicBool,
        observer: &dyn ScanObserver,
        poll: Duration,
    ) -> Result<(), DetectError> {
        let start = Instant::now();
        let mut waited = false;
        while observer.paused() {
            waited = true;
            if cancel.load(Ordering::Relaxed) {
                return Err(DetectError::Cancelled);
            }
            std::thread::sleep(poll);
        }
        if waited {
            self.paused.set(self.paused.get() + start.elapsed());
            self.advanced();
        }
        self.check(cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ago(seconds: u64) -> Instant {
        Instant::now() - Duration::from_secs(seconds)
    }

    #[test]
    fn desktop_stops_two_minutes_after_the_start_whatever_the_progress() {
        let clock = Clock::starting_at(ago(121), ScanLimit::DESKTOP);
        clock.advanced();
        assert!(matches!(
            clock.check(&AtomicBool::new(false)),
            Err(DetectError::TooSlow)
        ));
        assert!(
            Clock::starting_at(ago(119), ScanLimit::DESKTOP)
                .check(&AtomicBool::new(false))
                .is_ok()
        );
    }

    #[test]
    fn a_phone_runs_past_two_minutes_while_it_progresses_and_stops_after_a_minute_without() {
        let clock = Clock::starting_at(ago(600), ScanLimit::PHONE);
        clock.last.set(ago(59));
        assert!(clock.check(&AtomicBool::new(false)).is_ok());
        clock.last.set(ago(61));
        let stalled = clock.check(&AtomicBool::new(false)).unwrap_err();
        assert!(matches!(stalled, DetectError::Stalled));
        assert!(stalled.to_string().contains("no progress for a minute"));
        clock.advanced();
        assert!(clock.check(&AtomicBool::new(false)).is_ok());
    }

    #[test]
    fn cancelling_wins_over_both_limits() {
        for limit in [ScanLimit::DESKTOP, ScanLimit::PHONE] {
            assert!(matches!(
                Clock::new(limit).check(&AtomicBool::new(true)),
                Err(DetectError::Cancelled)
            ));
        }
    }

    /// Pauses for `polls` questions, then lets the scan go on.
    struct Hot(Cell<usize>);
    impl ScanObserver for Hot {
        fn paused(&self) -> bool {
            let left = self.0.get();
            self.0.set(left.saturating_sub(1));
            left > 0
        }
    }

    #[test]
    fn a_pause_longer_than_the_limit_stops_neither_limit() {
        let poll = Duration::from_millis(20);
        for limit in [
            ScanLimit::Total(Duration::from_millis(50)),
            ScanLimit::Stall(Duration::from_millis(50)),
        ] {
            let clock = Clock::new(limit);
            let hot = Hot(Cell::new(6));
            clock
                .wait_out_pause_every(&AtomicBool::new(false), &hot, poll)
                .unwrap();
            assert!(clock.started.elapsed() >= poll * 6, "it waited");
            assert_eq!(hot.0.get(), 0, "until the observer let it go");
        }
    }

    #[test]
    fn cancelling_ends_a_pause() {
        struct Always;
        impl ScanObserver for Always {
            fn paused(&self) -> bool {
                true
            }
        }
        let clock = Clock::new(ScanLimit::PHONE);
        assert!(matches!(
            clock.wait_out_pause_every(&AtomicBool::new(true), &Always, Duration::ZERO),
            Err(DetectError::Cancelled)
        ));
    }

    #[test]
    fn no_pause_checks_the_limit_at_once() {
        let clock = Clock::starting_at(ago(61), ScanLimit::PHONE);
        assert!(matches!(
            clock.wait_out_pause(&AtomicBool::new(false), &|_: ScanProgress| {}),
            Err(DetectError::Stalled)
        ));
    }
}
