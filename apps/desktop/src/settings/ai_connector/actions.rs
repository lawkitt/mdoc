//! What Settings › AI connector does: reading the AI apps' state,
//! recording the companion's shortcut, and changing an AI app's config file
//! once the user says yes.
use super::AiConnectorUi;
use crate::ai::companion::hotkey::{self, NotAHotkey};
use crate::ai::connector::setup::{self, Host, Kind, Places};
use crate::settings::{Panel, text};
use gpui::{App, Context, Keystroke, Window};

impl AiConnectorUi {
    /// Leaving the page or closing Settings: an unanswered card goes, and
    /// nothing is written.
    pub(in crate::settings) fn forget(&mut self, cx: &mut App) {
        self.consent = None;
        self.error = None;
        self.done = None;
        self.tested = false;
        self.stop_recording(cx);
    }

    /// The recorder stops, and the shortcut it paused comes back.
    fn stop_recording(&mut self, cx: &mut App) {
        self.hotkey_hint = None;
        if self.recording.take().is_some() {
            self.hotkey_failed = hotkey::pause(false, cx);
        }
    }
}

impl Panel {
    /// The window's last start of the connector: why it failed, or `None`.
    pub(crate) fn ai_connector_failed(&mut self, failed: Option<String>, cx: &mut Context<Self>) {
        if self.ai.failed != failed {
            self.ai.failed = failed;
            cx.notify();
        }
    }

    /// The window's last registration of the companion's shortcut: why it
    /// failed, or `None`.
    pub(crate) fn hotkey_failed(&mut self, failed: Option<String>, cx: &mut Context<Self>) {
        if self.ai.hotkey_failed != failed {
            self.ai.hotkey_failed = failed;
            cx.notify();
        }
    }

    /// The recorder: the next shortcut pressed, in any window, is the
    /// companion's; Esc or a second click stops it. The registered one
    /// pauses meanwhile, so it can be recorded again.
    pub(crate) fn record_hotkey(&mut self, cx: &mut Context<Self>) {
        if self.ai.recording.is_some() {
            self.ai.stop_recording(cx);
            cx.notify();
            return;
        }
        if self.draft.ai_connector.hotkey_off {
            return;
        }
        let this = cx.weak_entity();
        self.ai.recording = Some(cx.intercept_keystrokes(move |event, _, cx| {
            if this
                .update(cx, |panel, cx| panel.hotkey_keystroke(&event.keystroke, cx))
                .is_ok()
            {
                cx.stop_propagation();
            }
        }));
        self.ai.hotkey_hint = None;
        hotkey::pause(true, cx);
        cx.notify();
    }

    /// A key press while recording: a shortcut saves, Esc stops, anything
    /// else says why it isn't one.
    pub(crate) fn hotkey_keystroke(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        match hotkey::recorded(keystroke, hotkey::key_down()) {
            Ok(shortcut) => {
                self.ai.stop_recording(cx);
                if self.draft.ai_connector.hotkey != shortcut {
                    self.draft.ai_connector.hotkey = shortcut;
                    self.persist(cx);
                }
            }
            Err(NotAHotkey::Modifier) => return,
            Err(NotAHotkey::Cancel) => self.ai.stop_recording(cx),
            Err(NotAHotkey::Rejected(why)) => self.ai.hotkey_hint = Some(why),
        }
        cx.notify();
    }

    fn places(&mut self) -> &Places {
        self.ai.places.get_or_insert_with(|| {
            // Tests and renders never read the real apps' folders.
            if cfg!(test) || mdoc_store::app_paths::isolated() {
                Places::default()
            } else {
                Places::of_this_app()
            }
        })
    }

    /// Read the apps' state again; with the preview off, nothing is read.
    /// The shortcut recorder stops once the preview or the shortcut is off.
    pub(in crate::settings) fn refresh_ai(&mut self, cx: &mut Context<Self>) {
        if !self.draft.ai_connector.preview || self.draft.ai_connector.hotkey_off {
            self.ai.stop_recording(cx);
        }
        self.ai.status = if self.draft.ai_connector.preview {
            let places = self.places();
            Some(Host::ALL.map(|host| setup::status(places, host)))
        } else {
            None
        };
        cx.notify();
    }

    /// Add… or Remove…: plan the change and show it; nothing is written.
    pub(crate) fn ask_ai_change(
        &mut self,
        host: Host,
        remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let places = self.places().clone();
        self.ai.done = None;
        self.ai.error = None;
        let planned = if remove {
            setup::plan_remove(&places, host)
        } else {
            setup::plan_add(&places, host)
        };
        match planned {
            Ok(change) => {
                self.ai.consent = Some(change);
                let focus = (self.ai.focus)
                    .get_or_insert_with(|| cx.focus_handle())
                    .clone();
                window.focus(&focus, cx);
            }
            Err(refusal) => self.ai.error = Some((host, refusal.message(host))),
        }
        self.refresh_ai(cx);
    }

    /// The card's primary button: write the change the user saw.
    pub(crate) fn confirm_ai_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(change) = self.ai.consent.take() else {
            return;
        };
        let host = change.host;
        match setup::apply(&change) {
            Ok(()) => {
                self.ai.error = None;
                self.ai.done = Some((
                    host,
                    text::changed(change.kind == Kind::Remove, host.name()),
                ));
            }
            Err(refusal) => self.ai.error = Some((host, refusal.message(host))),
        }
        window.focus(&self.category_focus, cx);
        self.refresh_ai(cx);
    }

    /// Cancel, or Esc on the card: nothing is written.
    pub(crate) fn cancel_ai_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ai.consent = None;
        window.focus(&self.category_focus, cx);
        cx.notify();
    }

    /// Claude's Test connection: a new chat with the first prompt filled
    /// in, which makes Claude ask, once, to use mdoc's tools.
    pub(crate) fn test_ai_connection(&mut self, cx: &mut Context<Self>) {
        self.ai.tested = true;
        cx.open_url(&setup::claude_test_link());
        cx.notify();
    }
}
