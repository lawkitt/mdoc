//! New Project's view: the sheet that asks for the name.

use crate::ui;
use crate::workspace::projects::text;
use crate::workspace::{
    Close, Import, New, NewProject, Open, OpenPalette, OpenProject, PasteAiResponse,
    PseudonymizeClipboard, RetryDocument, Settings, SwitchDocument, SwitchDocumentBack,
    ToggleSidebar, Workspace,
    palette::shortcuts_ui::ShowShortcuts,
    projects::{CancelProjectName, ConfirmProjectName},
};
use gpui::{AnyElement, Context, Focusable, Window, div, prelude::*, px};

impl Workspace {
    pub(in crate::workspace) fn new_project_view(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = self.project_ui.new_project.as_ref()?;
        // The sheet keeps focus while it is the top dialog: a document that
        // finishes loading behind it must not take the keyboard.
        let field = sheet.input.read(cx).focus_handle(cx);
        if !self.project_ui.explainer
            && !window.has_active_prompt()
            && !self
                .project_ui
                .new_project_group
                .contains_focused(window, cx)
        {
            window.focus(&field, cx);
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let named = !sheet.input.read(cx).value().trim().is_empty();
        Some(
            gpui::deferred(
                div()
                    .id("new-project-overlay")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_start()
                    .justify_center()
                    .pt(px(64.))
                    .bg(theme.scrim())
                    // A modal: nothing behind it takes the pointer.
                    .occlude()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(gpui::MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                    .child(
                        ui::panel("new-project-sheet", theme)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "new-project-sheet".into())
                            })
                            .role(gpui::Role::Dialog)
                            .aria_label(text::NEW_PROJECT)
                            // Window commands wait until the sheet is answered.
                            .on_action(|_: &Close, _, _| {})
                            .on_action(|_: &New, _, _| {})
                            .on_action(|_: &Open, _, _| {})
                            .on_action(|_: &Import, _, _| {})
                            .on_action(|_: &OpenProject, _, _| {})
                            .on_action(|_: &NewProject, _, _| {})
                            .on_action(|_: &Settings, _, _| {})
                            .on_action(|_: &OpenPalette, _, _| {})
                            .on_action(|_: &ShowShortcuts, _, _| {})
                            .on_action(|_: &crate::commands::menus::About, _, _| {})
                            .on_action(|_: &SwitchDocument, _, _| {})
                            .on_action(|_: &SwitchDocumentBack, _, _| {})
                            .on_action(|_: &ToggleSidebar, _, _| {})
                            .on_action(|_: &RetryDocument, _, _| {})
                            .on_action(|_: &crate::workspace::home::ShowHome, _, _| {})
                            .on_action(|_: &crate::workspace::home::PasteAsNewTab, _, _| {})
                            .on_action(|_: &PasteAiResponse, _, _| {})
                            .on_action(|_: &PseudonymizeClipboard, _, _| {})
                            .on_action(|_: &crate::workspace::RestoreClipboard, _, _| {})
                            .on_action(|_: &crate::workspace::CheckForUpdates, _, _| {})
                            .key_context("UiPanel")
                            .track_focus(&self.project_ui.new_project_group)
                            .tab_group()
                            .tab_stop(false)
                            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                                ui::cycle(
                                    window,
                                    cx,
                                    Some(&this.project_ui.new_project_group),
                                    false,
                                );
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                                ui::cycle(
                                    window,
                                    cx,
                                    Some(&this.project_ui.new_project_group),
                                    true,
                                );
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(|this, _: &ConfirmProjectName, window, cx| {
                                this.confirm_new_project(window, cx);
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(|this, _: &CancelProjectName, window, cx| {
                                this.cancel_new_project(window, cx);
                                cx.stop_propagation();
                            }))
                            .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, window, cx| {
                                if e.keystroke.key == "escape" && !e.keystroke.modifiers.modified()
                                {
                                    this.cancel_new_project(window, cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .w(px(400.))
                            .max_w_full()
                            .p_5()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(ui::card_title(text::NEW_PROJECT))
                            .child(ui::card_text(text::NEW_PROJECT_NOTE, theme))
                            .child(
                                div()
                                    .border_1()
                                    .border_color(p.border)
                                    .rounded_sm()
                                    .p_1()
                                    .child(sheet.input.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap_2()
                                    .pt_1()
                                    .child(
                                        ui::action_control(
                                            "new-project-cancel",
                                            crate::ui::text::CANCEL,
                                            theme,
                                            true,
                                            false,
                                        )
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "new-project-cancel".into())
                                        })
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.cancel_new_project(window, cx)
                                            }),
                                        ),
                                    )
                                    .child(
                                        ui::primary_button(
                                            "new-project-create",
                                            text::CREATE,
                                            theme,
                                            named,
                                        )
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "new-project-create".into())
                                        })
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.confirm_new_project(window, cx)
                                            }),
                                        ),
                                    ),
                            ),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
