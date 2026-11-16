//! How fast the detector runs on this device: one short detector run timed
//! right after the model installs, kept in the models folder (which stays
//! on the device), then each scan's estimate and the speed ceiling. Speed
//! is per character, not per window: a window holds whole paragraphs, from
//! a few dozen tokens to 512, so a window's time does not carry over from
//! one document to another.
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The timed run's text: a page of a made-up contract, English and
/// Russian, as the documents a phone scans.
pub(super) const PROBE: &str = include_str!("probe.txt");

/// A phone slower than this, per 10,000 characters, is below the floor.
const CEILING_SECONDS: f64 = 600.;
const CEILING_CHARS: usize = 10_000;
/// A page, for Settings' figure per 10 pages: the 37-page legal opinion
/// fixture holds about 3,200 characters a page.
pub(crate) const PAGE_CHARS: usize = 3_000;
/// A scan expected to take longer than this shows its estimate first.
const SHOWN_AFTER_SECONDS: f64 = 60.;

const FILE: &str = "detector-speed.json";

/// The timed run's result on this device.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct DetectorSpeed {
    /// Building the detector's sessions, which every scan does.
    pub load_seconds: f64,
    /// Inference per 1,000 characters of the model's windows.
    pub seconds_per_thousand_chars: f64,
}

impl DetectorSpeed {
    /// From one run: its engine load, and its windows' characters and time.
    pub(super) fn measured(load: f64, chars: usize, inference: f64) -> Self {
        Self {
            load_seconds: load,
            seconds_per_thousand_chars: inference * 1000. / chars.max(1) as f64,
        }
    }

    /// Seconds a scan of `chars` characters takes.
    pub(crate) fn seconds(&self, chars: usize) -> f64 {
        self.load_seconds + self.seconds_per_thousand_chars * chars as f64 / 1000.
    }

    /// Past the speed ceiling: the device counts as below the floor.
    pub(crate) fn too_slow(&self) -> bool {
        self.seconds(CEILING_CHARS) > CEILING_SECONDS
    }

    /// "About N minutes on this phone", for a scan expected to take more
    /// than about a minute; `None` for a shorter one.
    pub(crate) fn shown_minutes(&self, chars: usize) -> Option<u32> {
        let seconds = self.seconds(chars);
        (seconds > SHOWN_AFTER_SECONDS).then(|| (seconds / 60.).round().max(1.) as u32)
    }
}

/// What the file keeps: the speed, and the model folder it was timed with,
/// so a new model is timed again.
#[derive(Serialize, Deserialize)]
struct Record {
    model: String,
    speed: DetectorSpeed,
}

pub(super) fn read(root: &Path, model: &str) -> Option<DetectorSpeed> {
    let record: Record = serde_json::from_slice(&std::fs::read(root.join(FILE)).ok()?).ok()?;
    (record.model == model).then_some(record.speed)
}

pub(super) fn write(root: &Path, model: &str, speed: DetectorSpeed) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    let partial = root.join(format!("{FILE}.part"));
    let record = Record {
        model: model.into(),
        speed,
    };
    std::fs::write(&partial, serde_json::to_vec(&record)?)?;
    std::fs::rename(partial, root.join(FILE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Scratch;

    fn speed(per_thousand: f64) -> DetectorSpeed {
        DetectorSpeed {
            load_seconds: 2.,
            seconds_per_thousand_chars: per_thousand,
        }
    }

    /// The 2026-10-31 spike's 20 pages (63,354 bytes) took 35 s on the
    /// iPhone 17 Pro and would take about 123 s on the 12 Pro, about 400 s
    /// on a Pixel 6a: all far inside the ceiling of 10 minutes per 10,000
    /// characters.
    #[test]
    fn the_ceiling_is_ten_minutes_per_ten_thousand_characters() {
        assert!(!speed(2.).too_slow(), "the iPhone 12 Pro");
        assert!(!speed(6.3).too_slow(), "the Pixel 6a");
        assert!(!speed(59.7).too_slow(), "just inside");
        assert!(speed(59.9).too_slow(), "just past, with the load");
        assert!(speed(120.).too_slow());
    }

    #[test]
    fn an_estimate_shows_only_past_about_a_minute() {
        let phone = speed(2.);
        // 20 pages on the 12 Pro: about two minutes.
        assert_eq!(phone.shown_minutes(20 * PAGE_CHARS), Some(2));
        // Two pages take seconds: no estimate.
        assert_eq!(phone.shown_minutes(2 * PAGE_CHARS), None);
        // Just past a minute reads as about one.
        assert_eq!(phone.shown_minutes(30_000), Some(1));
    }

    #[test]
    fn one_run_gives_the_speed_per_character() {
        let measured = DetectorSpeed::measured(1.5, 2_500, 5.);
        assert_eq!(measured.load_seconds, 1.5);
        assert_eq!(measured.seconds_per_thousand_chars, 2.);
        assert_eq!(measured.seconds(10_000), 21.5);
    }

    #[test]
    fn the_record_belongs_to_one_model() {
        let scratch = Scratch::new();
        let root = scratch.path().join("models");
        assert_eq!(read(&root, "gliner2-a"), None);
        write(&root, "gliner2-a", speed(2.)).unwrap();
        assert_eq!(read(&root, "gliner2-a"), Some(speed(2.)));
        assert_eq!(read(&root, "gliner2-b"), None, "a new model is timed again");
        std::fs::write(root.join(FILE), b"not json").unwrap();
        assert_eq!(read(&root, "gliner2-a"), None);
    }
}
