//! A phone's scan as the platform sees it (ADR 0120): no fixed deadline
//! but a stall guard, a free-memory check at the start, the time left as
//! windows finish, and a pause while the phone is at the critical thermal
//! state.
use super::{JobProgress, JobStage};
use crate::{
    CoreError,
    setup::{DETECTOR_FREE_BYTES, DetectorSpeed},
};
use mdoc_detect::{ScanObserver, ScanProgress};
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

/// The device's thermal state, in iOS's four steps. Android maps its
/// statuses onto them: none nominal, light fair, moderate and severe
/// serious, critical and worse critical.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Thermal {
    Nominal,
    Fair,
    Serious,
    Critical,
}

/// Refuses a scan that starts with less memory free than the detector
/// needs, because the system would end the app partway instead.
pub(super) fn check_free_memory(progress: Option<&dyn JobProgress>) -> Result<(), CoreError> {
    match progress.and_then(|p| p.free_memory()) {
        Some(free) if free < DETECTOR_FREE_BYTES => Err(CoreError::Detect {
            message: "Not enough memory is free to find names right now. Close other apps, then try again.".into(),
        }),
        _ => Ok(()),
    }
}

/// The detector's observer on a phone.
pub(super) struct PhoneScan<'a> {
    progress: Option<&'a dyn JobProgress>,
    speed: Option<DetectorSpeed>,
    /// When the windows were known, before the engine loads.
    started: Cell<Option<Instant>>,
    /// Since when the scan has been paused to cool down.
    cooling: Cell<Option<Instant>>,
    cooled: Cell<Duration>,
}

impl<'a> PhoneScan<'a> {
    pub(super) fn new(progress: Option<&'a dyn JobProgress>, speed: Option<DetectorSpeed>) -> Self {
        Self {
            progress,
            speed,
            started: Cell::new(None),
            cooling: Cell::new(None),
            cooled: Cell::new(Duration::ZERO),
        }
    }

    /// Seconds left: at the start the device's timed run; then the pace so
    /// far, time paused to cool down left out.
    fn remaining(&self, scan: ScanProgress) -> Option<u64> {
        let left = scan.chars.saturating_sub(scan.chars_done);
        let seconds = if scan.done == 0 {
            self.started.set(Some(Instant::now()));
            self.speed?.seconds(scan.chars)
        } else {
            let elapsed = self
                .started
                .get()?
                .elapsed()
                .saturating_sub(self.cooled.get());
            elapsed.as_secs_f64() * left as f64 / scan.chars_done.max(1) as f64
        };
        Some(seconds.round() as u64)
    }
}

impl ScanObserver for PhoneScan<'_> {
    fn progress(&self, scan: ScanProgress) {
        let Some(progress) = self.progress else {
            return;
        };
        progress.advanced(scan.done as u64, scan.windows as u64);
        if let Some(seconds) = self.remaining(scan) {
            progress.remaining(seconds);
        }
    }

    /// Paused at the critical state; going on once the phone has cooled to
    /// serious or below.
    fn paused(&self) -> bool {
        let Some(progress) = self.progress else {
            return false;
        };
        let critical = progress.thermal() == Thermal::Critical;
        match (critical, self.cooling.get()) {
            (true, None) => {
                self.cooling.set(Some(Instant::now()));
                progress.stage(JobStage::CoolingDown);
            }
            (false, Some(since)) => {
                self.cooled.set(self.cooled.get() + since.elapsed());
                self.cooling.set(None);
                progress.stage(JobStage::Scanning);
            }
            _ => {}
        }
        critical
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A platform that reports `thermal` states in turn and records what
    /// it hears.
    #[derive(Default)]
    struct Platform {
        free: Option<u64>,
        thermal: Mutex<Vec<Thermal>>,
        heard: Mutex<Vec<String>>,
    }

    impl JobProgress for Platform {
        fn stage(&self, stage: JobStage) {
            self.heard.lock().unwrap().push(format!("{stage:?}"));
        }
        fn advanced(&self, done: u64, total: u64) {
            self.heard.lock().unwrap().push(format!("{done}/{total}"));
        }
        fn remaining(&self, seconds: u64) {
            self.heard.lock().unwrap().push(format!("{seconds} s left"));
        }
        fn thermal(&self) -> Thermal {
            let mut states = self.thermal.lock().unwrap();
            if states.is_empty() {
                Thermal::Nominal
            } else {
                states.remove(0)
            }
        }
        fn free_memory(&self) -> Option<u64> {
            self.free
        }
    }

    #[test]
    fn a_scan_needs_its_memory_free_when_the_platform_knows() {
        let platform = |free| Platform {
            free,
            ..Default::default()
        };
        let refused = check_free_memory(Some(&platform(Some(DETECTOR_FREE_BYTES - 1))));
        assert!(
            matches!(&refused, Err(CoreError::Detect { message }) if message.contains("memory")),
            "{refused:?}"
        );
        assert!(check_free_memory(Some(&platform(Some(DETECTOR_FREE_BYTES)))).is_ok());
        // The Simulator and tests report none.
        assert!(check_free_memory(Some(&platform(None))).is_ok());
        assert!(check_free_memory(None).is_ok());
    }

    #[test]
    fn the_scan_pauses_at_critical_and_goes_on_at_serious() {
        let platform = Platform::default();
        *platform.thermal.lock().unwrap() = vec![
            Thermal::Serious,
            Thermal::Critical,
            Thermal::Critical,
            Thermal::Serious,
        ];
        let scan = PhoneScan::new(Some(&platform), None);
        let paused: Vec<bool> = (0..4).map(|_| scan.paused()).collect();
        assert_eq!(paused, [false, true, true, false]);
        assert_eq!(
            *platform.heard.lock().unwrap(),
            ["CoolingDown", "Scanning"],
            "one stage change each way"
        );
        assert!(!PhoneScan::new(None, None).paused());
    }

    #[test]
    fn time_left_starts_from_the_timed_run_then_follows_the_pace() {
        let platform = Platform::default();
        let speed = DetectorSpeed {
            load_seconds: 2.,
            seconds_per_thousand_chars: 2.,
        };
        let scan = PhoneScan::new(Some(&platform), Some(speed));
        let at = |done, chars_done| ScanProgress {
            done,
            windows: 4,
            chars_done,
            chars: 30_000,
        };
        scan.progress(at(0, 0));
        // Pretend a quarter took 30 s: three quarters take 90 s more.
        scan.started
            .set(Some(Instant::now() - Duration::from_secs(30)));
        scan.progress(at(1, 7_500));
        assert_eq!(
            *platform.heard.lock().unwrap(),
            ["0/4", "62 s left", "1/4", "90 s left"]
        );
        // Without a timed run the first estimate waits for a window.
        let platform = Platform::default();
        PhoneScan::new(Some(&platform), None).progress(at(0, 0));
        assert_eq!(*platform.heard.lock().unwrap(), ["0/4"]);
    }

    #[test]
    fn time_paused_to_cool_down_is_not_pace() {
        let platform = Platform::default();
        let scan = PhoneScan::new(Some(&platform), None);
        scan.progress(ScanProgress {
            windows: 2,
            chars: 2_000,
            ..Default::default()
        });
        scan.started
            .set(Some(Instant::now() - Duration::from_secs(70)));
        scan.cooled.set(Duration::from_secs(60));
        scan.progress(ScanProgress {
            done: 1,
            windows: 2,
            chars_done: 1_000,
            chars: 2_000,
        });
        assert_eq!(platform.heard.lock().unwrap().last().unwrap(), "10 s left");
    }
}
