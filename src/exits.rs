//! Test support for the suite of ways text leaves mdoc
//! (`workspace::tests::exits`): the places where an exit's text is checked
//! (#1091 Decision 19), and a switch that takes one of them out.
//!
//! Each gate hands out only what the Vault's outbound check
//! (`Vault::text_for_ai`) passes, now, against the names its Project
//! holds. The suite takes one gate's check out at a time ([`take_out`]) and
//! proves that it then fails at that gate's exits and nowhere else.

use std::cell::Cell;

/// A place an exit's text is checked. Exits that share one check share
/// its gate: the Share tab's button, the menus, the shortcut and the
/// palette all reach the editor's Copy for AI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gate {
    /// The editor's Copy for AI (`Workspace::try_copy_for_ai`).
    EditorCopyForAi,
    /// The shelf's Copy for AI, a click or ⌘C (`ShelfRows::copy`).
    ShelfCopy,
    /// The shelf's drag: its text and its `.md` file (`ShelfRows::drag`).
    ShelfDrag,
    // Milestone 2: the connector joins here, with a gate at its one
    // `Vault::text_for_ai` call and its exit in the suite's table.
}

impl Gate {
    pub(crate) const ALL: [Gate; 3] = [Gate::EditorCopyForAi, Gate::ShelfCopy, Gate::ShelfDrag];
}

thread_local! {
    static TAKEN_OUT: Cell<Option<Gate>> = const { Cell::new(None) };
}

/// Whether the running test took `gate`'s check out: its exits then hand
/// out what they would without it.
pub(crate) fn taken_out(gate: Gate) -> bool {
    TAKEN_OUT.with(|taken| taken.get() == Some(gate))
}

/// Take `gate`'s check out until the guard drops.
pub(crate) fn take_out(gate: Gate) -> TakenOut {
    TAKEN_OUT.with(|taken| taken.set(Some(gate)));
    TakenOut
}

/// Puts a taken-out check back when dropped.
pub(crate) struct TakenOut;

impl Drop for TakenOut {
    fn drop(&mut self) {
        TAKEN_OUT.with(|taken| taken.set(None));
    }
}
