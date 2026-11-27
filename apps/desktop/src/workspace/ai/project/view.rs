//! The AI project's view: the sidebar's indicator and the rail's icon, the
//! share prompt, the Project page's AI line and its activity. It calls
//! the owner's methods, never the Vault.
use crate::workspace::ai::text;
use crate::workspace::{Workspace, projects::project_page::Place};
use crate::{ui, ui::style};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use mdoc_store::document_id::VaultId;
use mdoc_vault::{AiActivity, AiEvent, Project};
use std::collections::HashMap;

/// The newest activity lines the Project page lists.
pub(in crate::workspace) const ACTIVITY_SHOWN: usize = 100;

impl Workspace {
    /// The sidebar's foot: the AI project, with Stop sharing.
    pub(in crate::workspace) fn ai_indicator(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.ai_connector.as_ref()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let shared = self
            .ai_shared()
            .and_then(|id| self.projects.project(&id).cloned());
        let card = div()
            .id("ai-project")
            .when(cfg!(test), |v| v.debug_selector(|| "ai-project".into()))
            .role(gpui::Role::Group)
            .aria_label(text::SHARED_WITH_AI_APPS)
            .flex()
            .flex_col()
            .gap(px(2.))
            .px_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_1()
                    .child(
                        div()
                            .px_1()
                            .text_size(px(10.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(p.header_muted)
                            .child(text::SHARED_WITH_AI_APPS_HEADING),
                    )
                    .children(self.ai_badge(cx)),
            );
        let card = match shared {
            Some(project) => {
                let id = project.id.clone();
                let color =
                    style::project_color(crate::workspace::ProjectLink::of(&project).hue(), theme);
                card.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            ui::control("ai-project-open", "", theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "ai-project-open".into())
                                })
                                .aria_label(text::open_project(&project.name))
                                .flex()
                                .items_center()
                                .gap_2()
                                .flex_1()
                                .min_w_0()
                                .child(div().flex_shrink_0().size(px(8.)).rounded_full().bg(color))
                                .child(div().min_w_0().truncate().child(project.name.clone()))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.show_project_page(&id, Place::Header, window, cx)
                                })),
                        )
                        .child(
                            ui::link_button("ai-stop-sharing", text::STOP_SHARING, theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "ai-stop-sharing".into())
                                })
                                .flex_shrink_0()
                                .text_size(px(12.))
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.stop_sharing_with_ai(cx)),
                                ),
                        ),
                )
            }
            None => card.child(
                div()
                    .px_1()
                    .pb_1()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(text::NO_PROJECT_SHARED),
            ),
        };
        Some(card.into_any_element())
    }

    /// The collapsed rail's mark of the AI project: its colour, its name
    /// in the tooltip, its page on a click.
    pub(in crate::workspace) fn ai_rail_icon(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.ai_connector.as_ref()?;
        let theme = self.theme.get();
        let shared = self
            .ai_shared()
            .and_then(|id| self.projects.project(&id).cloned());
        let label = match &shared {
            Some(project) => text::shared_with_ai_apps(&project.name),
            None => text::NO_PROJECT_SHARED_WITH_AI_APPS.to_string(),
        };
        let open = shared.as_ref().map(|project| project.id.clone());
        let badge = self.ai_rail_badge();
        let label = match self.ai.unseen().len() {
            0 => label,
            count => text::with_new(label, count),
        };
        let counts = badge.is_some();
        Some(
            div()
                .relative()
                .child(
                    ui::header_icon("rail-ai-project", label, ui::Icon::Reply, theme, false)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "rail-ai-project".into())
                        })
                        .size(px(32.))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.ai_rail_clicked(counts, open.as_deref(), window, cx)
                        })),
                )
                .children(badge)
                .when_some(shared.filter(|_| !counts), |v, project| {
                    v.child(
                        div()
                            .absolute()
                            .top(px(6.))
                            .right(px(6.))
                            .size(px(7.))
                            .rounded_full()
                            .bg(style::project_color(
                                crate::workspace::ProjectLink::of(&project).hue(),
                                theme,
                            )),
                    )
                })
                .into_any_element(),
        )
    }

    /// The share prompt: a card at the window's bottom left, over the
    /// sidebar's foot. It takes no focus. While a dialog or sheet is up it
    /// waits, and shows once that closes: it is drawn above the window's
    /// own layers. A system file picker is its own window: the card shows
    /// under it, so nothing need redraw when the picker closes.
    pub(in crate::workspace) fn ai_prompt_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.ai_connector.as_ref()?;
        let prompt = self.ai.prompt()?;
        if self.drawn_modal_up(window, cx) {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let title = text::asked_for_documents(crate::ai::connector::display_name(&prompt.app));
        let rows = (prompt.choices.iter().enumerate()).filter_map(|(index, id)| {
            let project = self.projects.project(id)?;
            let selected = index == prompt.chosen;
            let color =
                style::project_color(crate::workspace::ProjectLink::of(project).hue(), theme);
            Some(
                ui::control(
                    SharedString::from(format!("ai-share-choice-{index}")),
                    "",
                    theme,
                    true,
                )
                .when(cfg!(test), move |v| {
                    v.debug_selector(move || format!("ai-share-choice-{index}"))
                })
                .role(gpui::Role::RadioButton)
                .aria_label(project.name.clone())
                .aria_selected(selected)
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(14.))
                        .rounded_full()
                        .border_1()
                        .border_color(if selected { p.header_fg } else { p.border })
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(selected, |v| {
                            v.child(div().size(px(6.)).rounded_full().bg(p.header_fg))
                        }),
                )
                .child(div().flex_shrink_0().size(px(8.)).rounded_full().bg(color))
                .child(div().min_w_0().truncate().child(project.name.clone()))
                .on_click(cx.listener(move |this, _, _, cx| this.choose_ai_share(index, cx))),
            )
        });
        let card = ui::panel("ai-share-prompt", theme)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "ai-share-prompt".into())
            })
            .role(gpui::Role::Group)
            .aria_label(title.clone())
            .w(px(320.))
            .max_w_full()
            .p_4()
            .rounded_lg()
            .shadow_lg()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(ui::card_text(text::SHARE_A_PROJECT, theme))
            .child(
                div()
                    .id("ai-share-choices")
                    .role(gpui::Role::RadioGroup)
                    .aria_label(text::PROJECT_TO_SHARE)
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(rows),
            )
            .child(
                ui::card_actions()
                    .justify_end()
                    .child(
                        ui::action_control(
                            "ai-share-not-now",
                            crate::ui::text::NOT_NOW,
                            theme,
                            true,
                            false,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "ai-share-not-now".into())
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.ai_share_not_now(cx))),
                    )
                    .child(
                        ui::primary_button("ai-share", text::SHARE, theme, true)
                            .when(cfg!(test), |v| v.debug_selector(|| "ai-share".into()))
                            .on_click(cx.listener(|this, _, _, cx| this.share_ai_choice(cx))),
                    ),
            );
        Some(
            gpui::deferred(div().absolute().bottom(px(16.)).left(px(16.)).child(card))
                .with_priority(1)
                .into_any_element(),
        )
    }

    /// Under the Project page's counts, while it is the AI project.
    pub(in crate::workspace) fn ai_project_chip(
        &self,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.ai_shared().as_deref() != Some(project.id.as_str()) {
            return None;
        }
        let theme = self.theme.get();
        let accent = theme.search_accent();
        Some(
            div()
                .id("project-page-ai")
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "project-page-ai".into())
                })
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(12.))
                .child(
                    div()
                        .px_2()
                        .py(px(1.))
                        .rounded_full()
                        .border_1()
                        .border_color(accent)
                        .text_color(accent)
                        .child(text::SHARED_WITH_AI_APPS),
                )
                .child(
                    self.page_control("project-page-stop-sharing", text::STOP_SHARING, cx)
                        .text_color(accent)
                        .on_click(cx.listener(|this, _, _, cx| this.stop_sharing_with_ai(cx))),
                )
                .into_any_element(),
        )
    }

    /// The Project page's AI activity: which app read which document, and
    /// each delivery, newest first.
    pub(in crate::workspace) fn ai_activity_segment(
        &self,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.theme.get().pdf_style();
        let mut list = div()
            .id("project-activity")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "project-activity".into())
            })
            .flex()
            .flex_col()
            .gap_1();
        let entries = match self.ai.activity(&project.id) {
            Some(Ok(entries)) => entries.as_slice(),
            Some(Err(error)) => {
                return list
                    .child(ui::card_error(
                        text::activity_unreadable(error),
                        self.theme.get(),
                    ))
                    .into_any_element();
            }
            None => &[],
        };
        if entries.is_empty() {
            return list
                .child(
                    div()
                        .py_2()
                        .text_size(px(12.))
                        .text_color(p.header_muted)
                        .child(text::NO_ACTIVITY),
                )
                .into_any_element();
        }
        let (documents, replies) = self.activity_titles(&project.id);
        let now = crate::workspace::home::now();
        for (index, entry) in entries.iter().take(ACTIVITY_SHOWN).enumerate() {
            // Numbered from the oldest, so a row keeps its control (and
            // focus) as newer lines arrive above it.
            let number = entries.len() - index;
            let line = activity_line(entry, &documents, &replies, now);
            let target = line.opens.clone();
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .py_1()
                    .child(
                        self.page_control(
                            SharedString::from(format!("project-activity-{number}")),
                            line.text,
                            cx,
                        )
                        .truncate()
                        .when(target.is_none(), |v| v.cursor_default())
                        .on_click(cx.listener(
                            move |this, _, window, cx| match &target {
                                Some(Opens::Document(id)) => {
                                    if let Some(id) = VaultId::parse(id) {
                                        this.open_project_member(id, window, cx);
                                    }
                                }
                                Some(Opens::Reply(id)) => this.open_reply(id, window, cx),
                                None => {}
                            },
                        )),
                    )
                    .child(
                        div()
                            .px_2()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .truncate()
                            .child(line.detail),
                    ),
            );
        }
        if entries.len() > ACTIVITY_SHOWN {
            list = list.child(
                div()
                    .py_2()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(text::showing_newest(ACTIVITY_SHOWN, entries.len())),
            );
        }
        list.into_any_element()
    }
}

/// What a row of the AI activity opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum Opens {
    Document(String),
    Reply(String),
}

/// One row of the AI activity as the page shows it.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::workspace) struct ActivityLine {
    pub(in crate::workspace) text: String,
    pub(in crate::workspace) detail: String,
    pub(in crate::workspace) opens: Option<Opens>,
}

/// `entry` in words: the app by its product's name, the documents and
/// replies by their titles in mdoc (`documents`, `replies`), and how long
/// ago at `now`.
pub(in crate::workspace) fn activity_line(
    entry: &AiActivity,
    documents: &HashMap<String, String>,
    replies: &HashMap<String, String>,
    now: u64,
) -> ActivityLine {
    let app = crate::ai::connector::display_name(&entry.app);
    let quoted = |title: &String| text::quoted(title);
    let document = |id: &str| documents.get(id).map(quoted);
    let age = crate::workspace::home::age(entry.at, now);
    match &entry.event {
        AiEvent::Read { document: id, part } => {
            let title = document(id);
            let part = part.map(text::part).unwrap_or_default();
            ActivityLine {
                text: text::read(
                    &app,
                    title
                        .clone()
                        .unwrap_or_else(|| text::A_DELETED_DOCUMENT.into()),
                    part,
                ),
                detail: age,
                opens: title.map(|_| Opens::Document(id.clone())),
            }
        }
        AiEvent::Delivered {
            reply,
            document: delivered,
            read,
        } => {
            let (text, opens) = match delivered {
                Some(id) => match document(id) {
                    Some(title) => (
                        text::delivered_document(&app, title),
                        Some(Opens::Document(id.clone())),
                    ),
                    None => (text::delivered_deleted_document(&app), None::<Opens>),
                },
                None => match replies.get(reply) {
                    Some(title) => (
                        text::delivered_reply(&app, quoted(title)),
                        Some(Opens::Reply(reply.clone())),
                    ),
                    None => (text::delivered_deleted_reply(&app), None),
                },
            };
            let from = match read.as_slice() {
                [] => None,
                [first, rest @ ..] => {
                    let first = document(first).unwrap_or_else(|| text::A_DELETED_DOCUMENT.into());
                    Some(text::from_documents(first, rest.len()))
                }
            };
            ActivityLine {
                text,
                detail: [Some(age), from]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(text::SEPARATOR),
                opens,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        (pairs.iter())
            .map(|(id, title)| (id.to_string(), title.to_string()))
            .collect()
    }

    #[test]
    fn activity_reads_as_who_did_what_and_when() {
        let documents = titles(&[("a", "Lease"), ("b", "Notice"), ("d", "Summary")]);
        let replies = titles(&[("r", "Reply to Lease")]);
        let now = 10_000;
        let read = |document: &str, part, app: &str| AiActivity {
            at: now - 300,
            app: app.into(),
            event: AiEvent::Read {
                document: document.into(),
                part,
            },
        };
        assert_eq!(
            activity_line(&read("a", None, "claude-ai"), &documents, &replies, now),
            ActivityLine {
                text: "Claude read “Lease”".into(),
                detail: "5 min ago".into(),
                opens: Some(Opens::Document("a".into())),
            }
        );
        let part = activity_line(&read("a", Some(2), "codex"), &documents, &replies, now);
        assert_eq!(part.text, "Codex read “Lease” · part 2");
        let gone = activity_line(&read("x", None, ""), &documents, &replies, now);
        assert_eq!(gone.text, "An AI app read a deleted document");
        assert_eq!(gone.opens, None);
        let delivered = |document: Option<&str>, read: &[&str]| AiActivity {
            at: now,
            app: "claude-ai".into(),
            event: AiEvent::Delivered {
                reply: "r".into(),
                document: document.map(str::to_string),
                read: read.iter().map(|id| id.to_string()).collect(),
            },
        };
        assert_eq!(
            activity_line(&delivered(None, &["a"]), &documents, &replies, now),
            ActivityLine {
                text: "Claude delivered a reply “Reply to Lease”".into(),
                detail: "just now · from “Lease”".into(),
                opens: Some(Opens::Reply("r".into())),
            }
        );
        assert_eq!(
            activity_line(
                &delivered(Some("d"), &["a", "b", "x"]),
                &documents,
                &replies,
                now
            ),
            ActivityLine {
                text: "Claude delivered “Summary” as a document".into(),
                detail: "just now · from “Lease” and 2 more".into(),
                opens: Some(Opens::Document("d".into())),
            }
        );
    }
}
