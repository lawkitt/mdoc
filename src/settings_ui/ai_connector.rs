//! Settings › AI connector (Milestone 2 item 11): the "AI connector
//! (preview)" switch, and with it on, "Open deliveries when they arrive",
//! the companion's shortcut (macOS; item 15), adding mdoc to Claude
//! Desktop and to ChatGPT through their config files, and Test
//! connection. With the switch off the page shows the switch alone.
//!
//! Every button that writes shows a consent card first: the file, and the
//! exact entry added or removed (`crate::ai_connector::setup`). Nothing is
//! written until its primary button.
use super::{CloseSettings, Panel};
use crate::ai_connector::setup::{self, Change, Host, Kind, Places, Status};
use crate::companion::hotkey::{self, NotAHotkey};
use crate::style::{self, Theme};
use crate::ui;
use gpui::{AnyElement, App, Context, FocusHandle, Keystroke, Window, div, prelude::*, px};

#[cfg(test)]
mod tests;

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
    /// Leaving the page or closing Settings: an unanswered card goes, and
    /// nothing is written.
    pub(super) fn forget(&mut self, cx: &mut App) {
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

    /// `host`'s state, as last read.
    pub(crate) fn status(&self, host: Host) -> Option<&Status> {
        Some(&self.status.as_ref()?[host as usize])
    }
}

/// The ids of a host's controls, for tests.
fn ids(host: Host) -> [&'static str; 3] {
    match host {
        Host::Claude => ["ai-claude-add", "ai-claude-remove", "ai-claude-test"],
        Host::ChatGpt => ["ai-chatgpt-add", "ai-chatgpt-remove", "ai-chatgpt-test"],
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

    /// "Open deliveries when they arrive" as the page shows it. The window
    /// opens a delivery only while this and the saved value are both on:
    /// off at once, before its save lands, and on only once saved.
    pub(crate) fn opens_deliveries(&self) -> bool {
        self.draft.ai_connector.open_deliveries
    }

    /// "AI connector (preview)" shown off while Settings is open or its
    /// save hasn't landed (#1220, #1273): the window treats it as off,
    /// whatever is saved, so nothing starts the connector or takes the
    /// hotkey meanwhile. Closed with nothing saving, the saved value rules.
    pub(crate) fn ai_connector_shown_off(&self) -> bool {
        !self.draft.ai_connector.preview && (self.open || self.applying || self.save_pending)
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
    pub(super) fn refresh_ai(&mut self, cx: &mut Context<Self>) {
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
                    format!(
                        "{}. Quit and reopen {} to {} mdoc.",
                        if change.kind == Kind::Remove {
                            "Removed"
                        } else {
                            "Added"
                        },
                        host.name(),
                        if change.kind == Kind::Remove {
                            "unload"
                        } else {
                            "load"
                        },
                    ),
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

    pub(super) fn ai_connector_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let on = self.draft.ai_connector.preview;
        let switch = self.setting_row(
            "AI connector (preview)",
            "Lets Claude Desktop or ChatGPT read the Ready documents of a Project you share, \
             and send its answers back to mdoc. Only pseudonymized text leaves; mdoc restores \
             the names.",
            self.switch(
                "ai-connector-preview",
                "AI connector (preview)",
                on,
                true,
                |p| p.ai_connector.preview = !p.ai_connector.preview,
                theme,
                cx,
            ),
            theme,
        );
        if !on {
            return switch;
        }
        if let Some(change) = &self.ai.consent {
            return self.consent_card(change, theme, cx);
        }
        let p = theme.pdf_style();
        let muted = p.header_muted;
        let shim = (self.ai.places.as_ref())
            .and_then(|places| places.shim.as_ref())
            .map(|path| path.display().to_string());
        let mut section = div()
            .flex()
            .flex_col()
            .child(switch)
            .when_some(self.ai.failed.clone(), |v, why| {
                v.child(
                    div()
                        .id("ai-connector-failed")
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "ai-connector-failed".into())
                        })
                        .pb_3()
                        .text_size(px(12.))
                        .text_color(style::markdown_style(theme).alert_caution)
                        .child(why),
                )
            })
            .child(self.setting_row(
                "Open deliveries when they arrive",
                "A reply or document an AI app sends opens in front, unless a dialog is open.",
                self.switch(
                    "ai-open-deliveries",
                    "Open deliveries when they arrive",
                    self.draft.ai_connector.open_deliveries,
                    true,
                    |p| p.ai_connector.open_deliveries = !p.ai_connector.open_deliveries,
                    theme,
                    cx,
                ),
                theme,
            ));
        // The companion, and so its shortcut, is macOS only for now.
        if cfg!(target_os = "macos") {
            section = section.child(self.hotkey_row(theme, cx));
        }
        for host in Host::ALL {
            section = section.child(self.host_rows(host, shim.is_some(), theme, cx));
        }
        let mono = style::markdown_style(theme).mono.family.clone();
        let path = shim.clone();
        section
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .py_3()
                    .child(
                        div()
                            .id("ai-shim-path")
                            .flex_1()
                            .min_w_0()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child("Connector program: ")
                            .child(div().font_family(mono).child(
                                shim.unwrap_or_else(|| {
                                    setup::Refusal::NoShim.message(Host::Claude)
                                }),
                            )),
                    )
                    .when_some(path, |v, path| {
                        v.child(
                            self.scrolled_quiet("ai-copy-path", "Copy path", theme, true, cx)
                                .when(cfg!(test), |v| v.debug_selector(|| "ai-copy-path".into()))
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        path.clone(),
                                    ))
                                }),
                        )
                    }),
            )
            .into_any_element()
    }

    /// "Companion shortcut": the recorder, showing the shortcut, and its
    /// switch; under it, what the recorder asks or why the shortcut
    /// couldn't be registered.
    fn hotkey_row(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let setting = self.draft.ai_connector;
        let on = !setting.hotkey_off;
        let recording = self.ai.recording.is_some();
        let label = match recording {
            true => "Type a shortcut…".to_string(),
            false => setting.hotkey.label(),
        };
        let mut description =
            "Shows or hides the companion from any app, without leaving it.".to_string();
        if !self.draft.general.menu_bar {
            description.push_str(" Works while “Show mdoc in the menu bar” is on, in General.");
        }
        let caution = style::markdown_style(theme).alert_caution;
        let line = |id: &'static str, text: String, color| {
            div()
                .id(id)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .pb_3()
                .text_size(px(12.))
                .text_color(color)
                .child(text)
        };
        let recorder = self
            .scrolled_control("ai-hotkey-record", label, theme, on, cx)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "ai-hotkey-record".into())
            })
            .aria_label(format!(
                "Companion shortcut, {}. Click to record another",
                setting.hotkey.label()
            ))
            .min_w(px(64.))
            .on_click(cx.listener(move |this, _, _, cx| {
                if on {
                    this.record_hotkey(cx)
                }
            }));
        let switch = self.switch(
            "ai-hotkey",
            "Companion shortcut",
            on,
            true,
            |p| p.ai_connector.hotkey_off = !p.ai_connector.hotkey_off,
            theme,
            cx,
        );
        let hint = match (&self.ai.hotkey_hint, recording) {
            (Some(why), _) => Some((why.clone(), caution)),
            (None, true) => Some((
                "Press the new shortcut with ⌘, ⌃ or ⌥. Esc cancels.".to_string(),
                theme.pdf_style().header_muted,
            )),
            (None, false) => None,
        };
        div()
            .flex()
            .flex_col()
            .child(
                self.setting_row(
                    "Companion shortcut",
                    description,
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(recorder)
                        .child(switch),
                    theme,
                ),
            )
            .when_some(hint, |v, (text, color)| {
                v.child(line("ai-hotkey-hint", text, color))
            })
            .when_some(
                self.ai.hotkey_failed.clone().filter(|_| on && !recording),
                |v, why| v.child(line("ai-hotkey-failed", why, caution)),
            )
            .into_any_element()
    }

    /// One app's row: its state, Add…, Remove… and Test connection, and
    /// what the last button did.
    fn host_rows(
        &self,
        host: Host,
        has_shim: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = theme.pdf_style().header_muted;
        let status = self
            .ai
            .status(host)
            .cloned()
            .unwrap_or(Status::NotInstalled);
        let app = host.name();
        let described = match &status {
            Status::NotInstalled => setup::Refusal::NotInstalled.message(host),
            Status::NotAdded => "Not added.".into(),
            Status::Added => match host {
                Host::Claude => "Added. Quit and reopen Claude Desktop to load it.".into(),
                Host::ChatGpt => format!(
                    "Added. Quit and reopen ChatGPT, then in a Work thread ask: “{}” ChatGPT \
                     asks before it uses each mdoc tool. Codex reads this setting too.",
                    setup::FIRST_PROMPT
                ),
            },
            Status::Other => "Added, but pointing at another copy of mdoc-mcp.".into(),
            Status::Unreadable(refusal) => {
                let path = (self.ai.places.as_ref())
                    .and_then(|places| places.config(host))
                    .map(|path| format!(" {}", path.display()))
                    .unwrap_or_default();
                format!("{}{path}", refusal.message(host))
            }
        };
        let [add_id, remove_id, test_id] = ids(host);
        let offer_add = matches!(status, Status::NotAdded | Status::Other);
        let can_add = has_shim && offer_add;
        let added = matches!(status, Status::Added | Status::Other);
        let mut buttons = div().flex().gap_2().items_center();
        if offer_add {
            let label = if status == Status::Other {
                "Update…"
            } else {
                "Add…"
            };
            buttons = buttons.child(
                self.scrolled_control(add_id, label, theme, can_add, cx)
                    .when(cfg!(test), |v| v.debug_selector(move || add_id.into()))
                    .aria_label(format!("Add to {app}"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if can_add {
                            this.ask_ai_change(host, false, window, cx)
                        }
                    })),
            );
        }
        if added {
            buttons = buttons.child(
                self.scrolled_quiet(remove_id, "Remove…", theme, true, cx)
                    .when(cfg!(test), |v| v.debug_selector(move || remove_id.into()))
                    .aria_label(format!("Remove from {app}"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ask_ai_change(host, true, window, cx)
                    })),
            );
        }
        // ChatGPT has no link that opens a Work thread (item 10's Ask
        // ChatGPT will): its row says what to ask instead.
        if status == Status::Added && host == Host::Claude {
            buttons = buttons.child(
                self.scrolled_control(test_id, "Test connection", theme, true, cx)
                    .when(cfg!(test), |v| v.debug_selector(move || test_id.into()))
                    .on_click(cx.listener(|this, _, _, cx| this.test_ai_connection(cx))),
            );
        }
        let note = |text: String| {
            div()
                .pb_2()
                .text_size(px(11.))
                .text_color(muted)
                .child(text)
        };
        div()
            .flex()
            .flex_col()
            .child(self.setting_row(app, described, buttons, theme))
            .when_some(
                self.ai.error.as_ref().filter(|(at, _)| *at == host),
                |v, (_, why)| {
                    v.child(
                        div()
                            .id(("ai-connector-error", host as usize))
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "ai-connector-error".into())
                            })
                            .py_2()
                            .text_size(px(12.))
                            .text_color(style::markdown_style(theme).alert_caution)
                            .child(why.clone()),
                    )
                },
            )
            .when_some(
                self.ai.done.as_ref().filter(|(at, _)| *at == host),
                |v, (_, done)| v.child(note(done.clone())),
            )
            .when(self.ai.tested && host == Host::Claude, |v| {
                v.child(note(format!(
                    "A new chat opened with “{}” filled in. Send it; Claude asks before it uses \
                     each mdoc tool.",
                    setup::FIRST_PROMPT
                )))
            })
            .into_any_element()
    }

    /// The consent card: the file, the exact entry, and Add or Remove.
    fn consent_card(&self, change: &Change, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let p = theme.pdf_style();
        let mono = style::markdown_style(theme).mono.family.clone();
        let app = change.host.name();
        let servers = change.host.servers();
        let (title, says, primary) = match change.kind {
            Kind::Add => (
                format!("Add mdoc to {app}?"),
                format!("It adds this entry under {servers}:"),
                format!("Add to {app}"),
            ),
            Kind::Replace { .. } => (
                format!("Update mdoc in {app}?"),
                format!("It replaces mdoc's entry under {servers} with this one:"),
                format!("Update {app}"),
            ),
            Kind::Remove => (
                format!("Remove mdoc from {app}?"),
                format!("It removes this entry under {servers}:"),
                "Remove".to_owned(),
            ),
        };
        let code = |id: &'static str, text: String| {
            div()
                .id(id)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(p.border)
                .bg(p.placeholder_bg)
                .font_family(mono.clone())
                .text_size(px(12.))
                .children(text.lines().map(|line| {
                    // Keep the indentation.
                    let indent = line.len() - line.trim_start().len();
                    div()
                        .pl(px(7. * indent as f32))
                        .child(line.trim_start().to_owned())
                }))
        };
        let backup = if change.backs_up() {
            format!(
                " mdoc keeps the file as it was beside it, as {}.",
                change.backup_name()
            )
        } else {
            String::new()
        };
        let shared = if change.host == Host::ChatGpt {
            " Codex reads this file too."
        } else {
            ""
        };
        let reopen = if change.kind == Kind::Remove {
            "unload"
        } else {
            "load"
        };
        let focus = self.ai.focus.clone();
        div()
            .id("ai-consent")
            .when(cfg!(test), |v| v.debug_selector(|| "ai-consent".into()))
            .role(gpui::Role::Dialog)
            .aria_label(title.clone())
            .when_some(focus, |v, focus| v.track_focus(&focus))
            .on_action(cx.listener(|this, _: &CloseSettings, window, cx| {
                this.cancel_ai_change(window, cx);
                cx.stop_propagation();
            }))
            .my_3()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_lg()
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(div().child("mdoc will change one file:"))
            .child(
                div()
                    .id("ai-consent-path")
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "ai-consent-path".into())
                    })
                    .font_family(mono.clone())
                    .text_size(px(12.))
                    .child(change.path.display().to_string()),
            )
            .when_some(change.replaced(), |v, old| {
                v.child(div().child("It takes out this entry:"))
                    .child(code("ai-consent-old", old))
            })
            .child(div().child(says))
            .child(code("ai-consent-entry", change.shown()))
            .child(div().text_color(p.header_muted).child(format!(
                "Everything else in the file stays as it is.{backup}{shared} Quit and reopen \
                 {app} to {reopen} mdoc."
            )))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        self.reveal(
                            "ai-consent-yes",
                            ui::action_control("ai-consent-yes", primary, theme, true, true),
                            true,
                            cx,
                        )
                        .when(cfg!(test), |v| v.debug_selector(|| "ai-consent-yes".into()))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.confirm_ai_change(window, cx)),
                        ),
                    )
                    .child(
                        self.scrolled_quiet("ai-consent-cancel", "Cancel", theme, true, cx)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "ai-consent-cancel".into())
                            })
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.cancel_ai_change(window, cx)
                                }),
                            ),
                    ),
            )
            .into_any_element()
    }
}
