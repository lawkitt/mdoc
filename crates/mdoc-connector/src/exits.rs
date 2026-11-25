//! The exits suite's switch (Vault & Projects item 29): taking the
//! connector's check out, so the suite can prove that it then fails at the
//! connector's exits. Built only for tests and the `exits-suite` feature,
//! which the app's dev-dependencies turn on; the app itself has no way to
//! take the check out.
//!
//! With the check out, every read hands out the document's stored Markdown,
//! unchecked and from any Project, as a connector with no check would.
use std::cell::Cell;

thread_local! {
    static TAKEN_OUT: Cell<bool> = const { Cell::new(false) };
}

/// Whether the running test took the connector's check out.
pub fn taken_out() -> bool {
    TAKEN_OUT.with(Cell::get)
}

/// Take the connector's check out on this thread until the guard drops.
pub fn take_out() -> TakenOut {
    TAKEN_OUT.with(|taken| taken.set(true));
    TakenOut
}

/// Puts the connector's check back when dropped.
pub struct TakenOut;

impl Drop for TakenOut {
    fn drop(&mut self) {
        TAKEN_OUT.with(|taken| taken.set(false));
    }
}
