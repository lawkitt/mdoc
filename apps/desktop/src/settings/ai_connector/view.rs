//! Settings › AI connector as drawn: the switch, the companion's shortcut,
//! each AI app's row and the consent card.
use crate::ai::connector::setup::{self, Change, Host, Kind, Status};
use crate::settings::{CloseSettings, Panel, text};
use crate::ui;
use crate::ui::style::{self, Theme};
use gpui::{AnyElement, Context, div, prelude::*, px};

/// The ids of a host's controls, for tests.
fn ids(host: Host) -> [&'static str; 3] {
    match host {
        Host::Claude => ["ai-claude-add", "ai-claude-remove", "ai-claude-test"],
        Host::ChatGpt => ["ai-chatgpt-add", "ai-chatgpt-remove", "ai-chatgpt-test"],
    }
}

impl Panel {
    pub(in crate::settings) fn ai_connector_section(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on = self.draft.ai_connector.preview;
        let switch = self.setting_row(
            text::AI_CONNECTOR_PREVIEW,
            text::AI_CONNECTOR_DESCRIPTION,
            self.switch(
                "ai-connector-preview",
                text::AI_CONNECTOR_PREVIEW,
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
                text::OPEN_DELIVERIES,
                text::OPEN_DELIVERIES_DESCRIPTION,
                self.switch(
                    "ai-open-deliveries",
                    text::OPEN_DELIVERIES,
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
                            .child(text::CONNECTOR_PROGRAM)
                            .child(div().font_family(mono).child(
                                shim.unwrap_or_else(|| {
                                    setup::Refusal::NoShim.message(Host::Claude)
                                }),
                            )),
                    )
                    .when_some(path, |v, path| {
                        v.child(
                            self.scrolled_quiet("ai-copy-path", text::COPY_PATH, theme, true, cx)
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
            true => text::TYPE_A_SHORTCUT.to_string(),
            false => setting.hotkey.label(),
        };
        let mut description = text::HOTKEY_DESCRIPTION.to_string();
        if !self.draft.general.menu_bar {
            description.push_str(text::HOTKEY_NEEDS_MENU_BAR);
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
            .aria_label(text::record_hotkey(setting.hotkey.label()))
            .min_w(px(64.))
            .on_click(cx.listener(move |this, _, _, cx| {
                if on {
                    this.record_hotkey(cx)
                }
            }));
        let switch = self.switch(
            "ai-hotkey",
            text::COMPANION_SHORTCUT,
            on,
            true,
            |p| p.ai_connector.hotkey_off = !p.ai_connector.hotkey_off,
            theme,
            cx,
        );
        let hint = match (&self.ai.hotkey_hint, recording) {
            (Some(why), _) => Some((why.clone(), caution)),
            (None, true) => Some((
                text::PRESS_NEW_SHORTCUT.to_string(),
                theme.pdf_style().header_muted,
            )),
            (None, false) => None,
        };
        div()
            .flex()
            .flex_col()
            .child(
                self.setting_row(
                    text::COMPANION_SHORTCUT,
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
            Status::NotAdded => text::NOT_ADDED.into(),
            Status::Added => match host {
                Host::Claude => text::ADDED_CLAUDE.into(),
                Host::ChatGpt => text::added_chatgpt(setup::FIRST_PROMPT),
            },
            Status::Other => text::ADDED_OTHER.into(),
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
                text::UPDATE_ELLIPSIS
            } else {
                text::ADD_ELLIPSIS
            };
            buttons = buttons.child(
                self.scrolled_control(add_id, label, theme, can_add, cx)
                    .when(cfg!(test), |v| v.debug_selector(move || add_id.into()))
                    .aria_label(text::add_to(app))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if can_add {
                            this.ask_ai_change(host, false, window, cx)
                        }
                    })),
            );
        }
        if added {
            buttons = buttons.child(
                self.scrolled_quiet(remove_id, text::REMOVE_ELLIPSIS, theme, true, cx)
                    .when(cfg!(test), |v| v.debug_selector(move || remove_id.into()))
                    .aria_label(text::remove_from(app))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ask_ai_change(host, true, window, cx)
                    })),
            );
        }
        // ChatGPT has no link that opens a Work thread (item 10's Ask
        // ChatGPT will): its row says what to ask instead.
        if status == Status::Added && host == Host::Claude {
            buttons = buttons.child(
                self.scrolled_control(test_id, text::TEST_CONNECTION, theme, true, cx)
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
                v.child(note(text::test_opened(setup::FIRST_PROMPT)))
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
                text::add_title(app),
                text::add_says(servers),
                text::add_to(app),
            ),
            Kind::Replace { .. } => (
                text::update_title(app),
                text::update_says(servers),
                text::update_app(app),
            ),
            Kind::Remove => (
                text::remove_title(app),
                text::remove_says(servers),
                text::REMOVE.to_owned(),
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
            text::keeps_backup(change.backup_name())
        } else {
            String::new()
        };
        let shared = if change.host == Host::ChatGpt {
            text::CODEX_READS_TOO
        } else {
            ""
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
            .child(div().child(text::CHANGES_ONE_FILE))
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
                v.child(div().child(text::TAKES_OUT_ENTRY))
                    .child(code("ai-consent-old", old))
            })
            .child(div().child(says))
            .child(code("ai-consent-entry", change.shown()))
            .child(
                div()
                    .text_color(p.header_muted)
                    .child(text::everything_else(
                        &backup,
                        shared,
                        app,
                        change.kind == Kind::Remove,
                    )),
            )
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
                        self.scrolled_quiet(
                            "ai-consent-cancel",
                            crate::ui::text::CANCEL,
                            theme,
                            true,
                            cx,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "ai-consent-cancel".into())
                        })
                        .on_click(
                            cx.listener(|this, _, window, cx| this.cancel_ai_change(window, cx)),
                        ),
                    ),
            )
            .into_any_element()
    }
}
