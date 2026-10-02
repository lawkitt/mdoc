//! Admission is retained by the worker, even after its UI has cancelled/closed.
use std::sync::atomic::{AtomicBool, Ordering};
static BUSY: AtomicBool = AtomicBool::new(false);
pub struct Permit {
    busy: &'static AtomicBool,
}
impl Permit {
    pub fn acquire() -> Result<Self, String> {
        Self::acquire_from(&BUSY)
    }
    fn acquire_from(busy: &'static AtomicBool) -> Result<Self, String> {
        busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self { busy })
            .map_err(|_| "Another model job is running. Retry when it finishes.".into())
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.busy.store(false, Ordering::Release);
    }
}
pub fn busy() -> bool {
    BUSY.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_does_not_release_a_worker_owned_slot() {
        static TEST_BUSY: AtomicBool = AtomicBool::new(false);
        let permit = Permit::acquire_from(&TEST_BUSY).unwrap();
        let cancel = AtomicBool::new(true);
        assert!(cancel.load(Ordering::Relaxed));
        assert!(Permit::acquire_from(&TEST_BUSY).is_err());
        std::thread::spawn(move || {
            assert!(TEST_BUSY.load(Ordering::Acquire));
            drop(permit);
        })
        .join()
        .unwrap();
        assert!(Permit::acquire_from(&TEST_BUSY).is_ok());
    }
}
