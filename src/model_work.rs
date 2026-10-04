//! Admission is retained by the worker, even after its UI has cancelled/closed.
use std::sync::{Condvar, Mutex};

#[derive(Default)]
struct State {
    busy: bool,
    stopping: bool,
}
struct Admission {
    state: Mutex<State>,
    idle: Condvar,
}
impl Admission {
    const fn new() -> Self {
        Self {
            state: Mutex::new(State {
                busy: false,
                stopping: false,
            }),
            idle: Condvar::new(),
        }
    }
    fn shutdown(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.stopping = true;
        self.idle.notify_all();
        while state.busy {
            state = self.idle.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }
}
static ADMISSION: Admission = Admission::new();
pub struct Permit {
    admission: &'static Admission,
}
impl Permit {
    pub fn acquire() -> Result<Self, String> {
        Self::acquire_from(&ADMISSION)
    }
    fn acquire_from(admission: &'static Admission) -> Result<Self, String> {
        let mut state = admission.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopping {
            return Err("The application is shutting down.".into());
        }
        if state.busy {
            return Err("Another model job is running. Retry when it finishes.".into());
        }
        state.busy = true;
        Ok(Self { admission })
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self
            .admission
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        state.busy = false;
        self.admission.idle.notify_all();
    }
}
pub fn busy() -> bool {
    ADMISSION
        .state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .busy
}
/// Called synchronously before GPUI's short asynchronous quit deadline. Native
/// runtime destructors must not run while a worker is loading or using a model.
pub fn shutdown() {
    ADMISSION.shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_does_not_release_a_worker_owned_slot() {
        static TEST_ADMISSION: Admission = Admission::new();
        let permit = Permit::acquire_from(&TEST_ADMISSION).unwrap();
        assert!(Permit::acquire_from(&TEST_ADMISSION).is_err());
        std::thread::spawn(move || {
            assert!(TEST_ADMISSION.state.lock().unwrap().busy);
            drop(permit);
        })
        .join()
        .unwrap();
        assert!(Permit::acquire_from(&TEST_ADMISSION).is_ok());
    }
    #[test]
    fn shutdown_waits_for_worker_and_rejects_new_jobs() {
        static TEST_ADMISSION: Admission = Admission::new();
        let permit = Permit::acquire_from(&TEST_ADMISSION).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let shutdown = std::thread::spawn(move || {
            TEST_ADMISSION.shutdown();
            tx.send(()).unwrap();
        });
        let mut state = TEST_ADMISSION.state.lock().unwrap();
        while !state.stopping {
            state = TEST_ADMISSION.idle.wait(state).unwrap();
        }
        drop(state);
        assert!(Permit::acquire_from(&TEST_ADMISSION).is_err());
        assert!(rx.try_recv().is_err());
        drop(permit);
        shutdown.join().unwrap();
        rx.recv().unwrap();
        assert!(Permit::acquire_from(&TEST_ADMISSION).is_err());
    }
}
