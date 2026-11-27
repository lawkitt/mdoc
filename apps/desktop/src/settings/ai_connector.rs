//! Settings › AI connector (Milestone 2 item 11): the "AI connector
//! (preview)" switch, and with it on, "Open deliveries when they arrive",
//! the companion's shortcut (macOS; item 15), adding mdoc to Claude
//! Desktop and to ChatGPT through their config files, and Test
//! connection. With the switch off the page shows the switch alone.
//!
//! Every button that writes shows a consent card first: the file, and the
//! exact entry added or removed (`crate::ai::connector::setup`). Nothing is
//! written until its primary button.
use super::Panel;
use crate::ai::connector::setup::{Change, Host, Places, Status};
use gpui::{Context, FocusHandle};

#[cfg(test)]
mod tests;

mod actions;
mod view;

/// What the page holds besides the switch.
#[derive(Default)]
pub(crate) struct AiConnectorUi {
    /// Where the setup looks; this app's own, once the page first shows,
    /// unless a test or render injected some.
    places: Option<Places>,
    /// Claude Desktop's and ChatGPT's state, read when the page shows and
    /// after a write; `None` while the preview is off.
    status: Option<[Status; 2]>,
    /// The change waiting for the user's yes.
    consent: Option<Change>,
    /// Why the last button wrote nothing, and for which app.
    error: Option<(Host, String)>,
    /// What the last write did, and for which app.
    done: Option<(Host, String)>,
    /// Claude's Test connection was clicked: the page says what happens.
    tested: bool,
    /// Why the connector, switched on, couldn't start (#1220); the window
    /// sets it with each start, and clears it with the switch off.
    failed: Option<String>,
    focus: Option<FocusHandle>,
    /// Why the companion's shortcut couldn't be registered.
    hotkey_failed: Option<String>,
    /// The shortcut recorder takes the next key press, in any window,
    /// before any binding does.
    recording: Option<gpui::Subscription>,
    /// Why the last key pressed into the recorder isn't a shortcut.
    hotkey_hint: Option<String>,
}

impl AiConnectorUi {
    /// `host`'s state, as last read.
    pub(crate) fn status(&self, host: Host) -> Option<&Status> {
        Some(&self.status.as_ref()?[host as usize])
    }
}

impl Panel {
    /// Tests and headless renders: look in `places` instead of this app's
    /// own folders.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn use_ai_places(&mut self, places: Places, cx: &mut Context<Self>) {
        self.ai.places = Some(places);
        self.ai.status = None;
        self.refresh_ai(cx);
    }

    /// The consent card waiting for a yes, for tests and renders.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn ai_consent(&self) -> Option<&Change> {
        self.ai.consent.as_ref()
    }

    /// "Open deliveries when they arrive" as the page shows it. The window
    /// opens a delivery only while this and the saved value are both on:
    /// off at once, before its save lands, and on only once saved.
    pub(crate) fn opens_deliveries(&self) -> bool {
        self.draft.ai_connector.open_deliveries
    }

    /// "AI connector (preview)" shown off while Settings is open or its
    /// save hasn't landed (#1220, #1273), or an off still being saved under
    /// an on shown since: the window treats it as off, whatever is saved,
    /// so nothing starts the connector or takes the hotkey meanwhile, and
    /// an on after an off starts once its own save lands. Closed with
    /// nothing saving, the saved value rules.
    pub(crate) fn ai_connector_shown_off(&self) -> bool {
        let shown_off =
            !self.draft.ai_connector.preview && (self.open || self.applying || self.save_pending);
        shown_off || self.saving_preview == Some(false)
    }

    /// Tests: hold the next save until `gate` finishes.
    #[cfg(test)]
    pub(crate) fn hold_next_save(&mut self, gate: gpui::Task<()>) {
        self.save_gate = Some(gate);
    }

    /// Why the connector couldn't start, as the page shows it.
    #[cfg(test)]
    pub(crate) fn ai_failed(&self) -> Option<&str> {
        self.ai.failed.as_deref()
    }

    /// Why the companion's shortcut couldn't be registered, as the page
    /// holds it.
    #[cfg(all(test, target_os = "macos"))]
    pub(crate) fn hotkey_failed_shown(&self) -> Option<String> {
        self.ai.hotkey_failed.clone()
    }
}
