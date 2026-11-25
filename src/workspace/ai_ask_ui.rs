//! Ask in the window (#1164 Decisions 2, 3 and 22; Milestone 2 item 10),
//! only while "AI connector (preview)" runs: what the document header's
//! choice does (`TabEvent::Ask`), and the Project page's Ask button. The
//! words and links are `crate::ai_ask`'s.
//!
//! Ask Claude or Ask ChatGPT shares the Project as part of the click, in
//! place of any other, then opens the app with a prompt naming the
//! document by the label the connector gives it. Copy for AI is today's
//! Copy for AI. Each choice is remembered as `ai_connector.ask`.
use super::Workspace;
use crate::{ai_ask, ui};
use gpui::{AnyElement, Context, Window, div, prelude::*, px};
use mdoc_store::settings::{self, AskWith};

impl Workspace {
    /// Ask's last choice, as Settings holds it.
    pub(super) fn ask_last(&self) -> AskWith {
        self.ai_setting().ask
    }

    /// Remember `with` as Ask's last choice. Kept for this window when the
    /// save fails; the error is logged.
    fn remember_ask(&mut self, with: AskWith, cx: &mut Context<Self>) {
        let (prefs, path) = {
            let store = self.preferences.borrow();
            let Some(current) = store.current.clone() else {
                return;
            };
            (current, store.path.clone())
        };
        if prefs.ai_connector.ask == with {
            return;
        }
        let mut changed = prefs;
        changed.ai_connector.ask = with;
        let write = settings::next_write();
        if let Some(path) = path
            && let Err(error) = settings::save_in_order(&path, &changed, write)
        {
            eprintln!("mdoc: could not save Ask's choice: {error}");
        }
        self.preferences.borrow_mut().current = Some(changed);
        self.settings.update(cx, |panel, cx| {
            panel.ai_connector_changed(|ai| ai.ask = with, cx)
        });
    }

    /// The document header's choice `with` in tab `tab`; `move_to_new`:
    /// the Unfiled card's "Move to a new Project and share".
    pub(super) fn ask_from_tab(
        &mut self,
        tab: u64,
        with: AskWith,
        move_to_new: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remember_ask(with, cx);
        if with == AskWith::CopyForAi {
            self.copy_tab_for_ai(tab, window, cx);
            cx.notify();
            return;
        }
        if self.ai_connector.is_none() {
            return;
        }
        let Some(document) = self.tab_document(tab, cx) else {
            return;
        };
        if self.excluded_from_ai(document.as_str()) {
            self.notices.post(ai_ask::EXCLUDED);
            cx.notify();
            return;
        }
        // What it shows now is what the app reads.
        self.autosave(tab, window, cx);
        let project = (self.tab_view(tab))
            .and_then(|view| view.read(cx).project.link().map(|link| link.id.clone()));
        match project {
            Some(project) => self.ask_app(&project, Some(document.as_str()), with, cx),
            // Unfiled never leaves: it moves, then asks (`ask_after_move`).
            None if move_to_new && self.can_move(tab, cx) => {
                self.ai_ask.after_move(document.as_str(), with);
                self.move_to_new_project(tab, window, cx);
            }
            None => {}
        }
        cx.notify();
    }

    /// Document `document` moved into Project `made`, which the move made
    /// for an Ask with `with`: share that Project and ask.
    pub(super) fn ask_after_move(
        &mut self,
        document: &str,
        made: &str,
        with: AskWith,
        cx: &mut Context<Self>,
    ) {
        self.ask_app(made, Some(document), with, cx);
    }

    /// The Project page's choice `with` for Project `project`.
    pub(super) fn ask_about_project(
        &mut self,
        project: &str,
        with: AskWith,
        cx: &mut Context<Self>,
    ) {
        let with = ai_ask::on_project_page(with);
        self.remember_ask(with, cx);
        self.project_ui.ask_menu = None;
        self.ask_app(project, None, with, cx);
        cx.notify();
    }

    /// Share Project `project` and open `with`'s app with the prompt for
    /// `document`, or for the Project.
    fn ask_app(
        &mut self,
        project: &str,
        document: Option<&str>,
        with: AskWith,
        cx: &mut Context<Self>,
    ) {
        if !self.share_with_ai(project, cx) {
            return;
        }
        let prompt = match document {
            None => ai_ask::PROJECT_PROMPT.to_string(),
            Some(document) => {
                let Some(running) = &self.ai_connector else {
                    return;
                };
                // The label the app will see: from the connector's own
                // list, never a title.
                let label = match running.connector.list_documents() {
                    Ok(listed) => (listed.into_iter())
                        .find(|row| row.doc_id == document)
                        .map(|row| row.label),
                    Err(error) => {
                        self.notices
                            .post(format!("Could not ask about the document. {error}"));
                        return;
                    }
                };
                let Some(label) = label else {
                    self.notices.post(ai_ask::EXCLUDED);
                    return;
                };
                ai_ask::document_prompt(&label, document)
            }
        };
        let Some(link) = ai_ask::link(with, &prompt) else {
            return;
        };
        cx.open_url(&link);
        let name = (self.projects.project(project))
            .map(|project| project.name.clone())
            .unwrap_or_default();
        self.notices.post(ai_ask::opened(with, &name));
    }

    /// Whether document `id` is excluded from AI apps.
    fn excluded_from_ai(&self, id: &str) -> bool {
        (self.vault.vault().into_iter())
            .flat_map(|vault| vault.documents())
            .any(|summary| summary.id == id && summary.exclude_from_ai)
    }

    // ---- The Project page --------------------------------------------------

    /// The page's Ask split button, while the connector runs.
    pub(super) fn project_ask_button(
        &self,
        project: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.ai_connector.as_ref()?;
        let theme = self.theme.get();
        let last = ai_ask::on_project_page(self.ask_last());
        let (ask, menu) = (project.to_string(), project.to_string());
        let main = self
            .page_control("project-page-ask", ai_ask::label(last), cx)
            .on_click(cx.listener(move |this, _, _, cx| this.ask_about_project(&ask, last, cx)));
        let open = self.project_ui.ask_menu.is_some();
        let (frame, more) = ui::split_button(
            main,
            "project-page-ask-more",
            "More ways to ask",
            open,
            theme,
        );
        Some(
            frame
                .child(
                    more.when(cfg!(test), |v| {
                        v.debug_selector(|| "project-page-ask-more".into())
                    })
                    .on_click(cx.listener(
                        move |this, event: &gpui::ClickEvent, window, cx| {
                            this.menu_return = window.focused(cx);
                            this.project_ui.ask_menu = Some((menu.clone(), event.position()));
                            window.focus(&this.menu_focus, cx);
                            cx.notify();
                        },
                    )),
                )
                .into_any_element(),
        )
    }

    /// The page's Ask menu: Ask Claude and Ask ChatGPT, the last ticked.
    pub(super) fn project_ask_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id, position) = self.project_ui.ask_menu.as_ref()?;
        self.ai_connector.as_ref()?;
        let theme = self.theme.get();
        let last = ai_ask::on_project_page(self.ask_last());
        let close = |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
            this.project_ui.ask_menu = None;
            if let Some(focus) = this.menu_return.take() {
                window.focus(&focus, cx);
            }
            cx.notify();
        };
        let mut menu = ui::panel("project-ask-menu", theme)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "project-ask-menu".into())
            })
            .track_focus(&self.menu_focus)
            .key_context("UiPanel UiMenu")
            .p_1()
            .rounded_lg()
            .shadow_lg()
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                ui::cycle(window, cx, Some(&this.menu_focus), false);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, Some(&this.menu_focus), true);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |this, _: &ui::CloseMenu, window, cx| {
                close(this, window, cx);
                cx.stop_propagation();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.project_ui.ask_menu = None;
                cx.notify();
            }));
        for (key, with) in [
            ("project-ask-claude", AskWith::Claude),
            ("project-ask-chatgpt", AskWith::ChatGpt),
        ] {
            let id = id.clone();
            menu = menu.child(
                ui::menu_row(
                    key,
                    ai_ask::label(with),
                    None,
                    Some(with == last),
                    theme,
                    true,
                )
                .when(cfg!(test), move |v| v.debug_selector(move || key.into()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    close(this, window, cx);
                    this.ask_about_project(&id, with, cx);
                })),
            );
        }
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(*position)
                    .snap_to_window()
                    .child(div().mt(px(12.)).child(menu)),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// Show Project `project`'s page with its Ask menu open, for renders.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(super) fn open_project_ask_menu(
        &mut self,
        project: &str,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_project_page(project, super::project_page::Place::Header, window, cx);
        self.project_ui.ask_menu = Some((project.to_string(), position));
        window.focus(&self.menu_focus, cx);
        cx.notify();
    }
}
