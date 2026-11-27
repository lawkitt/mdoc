//! Ask's view: the Project page's Ask split button and its menu. A click
//! calls the owner.
use crate::ui;
use crate::workspace::Workspace;
use crate::workspace::ai::text;
use gpui::{AnyElement, Context, div, prelude::*, px};
use mdoc_store::settings::AskWith;

impl Workspace {
    /// The page's Ask split button, while the connector runs.
    pub(in crate::workspace) fn project_ask_button(
        &self,
        project: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.ai_connector.as_ref()?;
        let theme = self.theme.get();
        let last = crate::ai::ask::on_project_page(self.ask_last());
        let (ask, menu) = (project.to_string(), project.to_string());
        let main = self
            .page_control("project-page-ask", crate::ai::ask::label(last), cx)
            .on_click(cx.listener(move |this, _, _, cx| this.ask_about_project(&ask, last, cx)));
        let open = self.project_ui.ask_menu.is_some();
        let (frame, more) = ui::split_button(
            main,
            "project-page-ask-more",
            text::MORE_WAYS_TO_ASK,
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
                            this.show_ask_menu(menu.clone(), event.position(), window, cx)
                        },
                    )),
                )
                .into_any_element(),
        )
    }

    /// The page's Ask menu: Ask Claude and Ask in a ChatGPT thread, the last ticked.
    pub(in crate::workspace) fn project_ask_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (id, position) = self.project_ui.ask_menu.as_ref()?;
        self.ai_connector.as_ref()?;
        // It belongs to the page it was opened over.
        if self.project_ui.page.as_ref().map(|page| page.id()) != Some(id.as_str()) {
            return None;
        }
        let theme = self.theme.get();
        let last = crate::ai::ask::on_project_page(self.ask_last());
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
                this.close_ask_menu(window, cx);
                cx.stop_propagation();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.ask_menu_dismissed(cx)));
        for (key, with) in [
            ("project-ask-claude", AskWith::Claude),
            ("project-ask-chatgpt", AskWith::ChatGpt),
        ] {
            let id = id.clone();
            menu =
                menu.child(
                    ui::menu_row(
                        key,
                        crate::ai::ask::label(with),
                        None,
                        Some(with == last),
                        theme,
                        true,
                    )
                    .when(cfg!(test), move |v| v.debug_selector(move || key.into()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ask_from_menu(&id, with, window, cx)
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
}
