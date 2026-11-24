//! New Project (Vault & Projects item 31): from the Project menu, the
//! palette or the "+" on the tree's Projects header. A sheet asks for the
//! name, as both phones do; Create queues one job on the window's writer,
//! which makes the empty Project at the head of the queue and then opens
//! its page.
use super::{
    Close, Import, New, NewProject, Open, OpenPalette, OpenProject, PasteAiResponse,
    PseudonymizeClipboard, RetryDocument, Settings, SwitchDocument, SwitchDocumentBack,
    ToggleSidebar, Workspace,
    autosave::Write,
    document_writer::Target,
    project_page::Place,
    projects_ui::{CancelProjectName, ConfirmProjectName},
    shortcuts_ui::ShowShortcuts,
};
use crate::{
    markdown_search::{SearchInput, SearchInputEvent},
    ui,
};
use gpui::{AnyElement, Context, Entity, FocusHandle, Focusable, Window, div, prelude::*, px};
use std::collections::HashSet;

/// The name question while it shows: view state only.
pub(super) struct NewProjectSheet {
    input: Entity<SearchInput>,
    /// Where focus goes back when it is cancelled.
    return_focus: Option<FocusHandle>,
    _changes: gpui::Subscription,
}

/// New Project, queued: the name as asked. What the name becomes, and the
/// Project's categories, are read when the job reaches the head.
pub(super) struct NewProjectJob {
    name: String,
}

impl Workspace {
    /// New Project: ask for the name. A locked Vault opens first.
    pub(super) fn ask_new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready || self.quit_flow.quitting() {
            return;
        }
        if let Some(sheet) = &self.new_project {
            let focus = sheet.input.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
            return;
        }
        if !self.vault_key_ready(|this, window, cx| this.ask_new_project(window, cx), cx) {
            return;
        }
        if self.vault.vault().is_none() {
            return;
        }
        self.close_project_list(window, cx);
        let return_focus = window.focused(cx);
        let input = cx.new(|cx| {
            SearchInput::new(cx)
                .with_theme(self.theme.clone())
                .with_key_context("ProjectName")
                .with_placeholder("Project name")
                .with_label("Name")
        });
        let changes = cx.subscribe(&input, |_, _, _: &SearchInputEvent, cx| cx.notify());
        let focus = input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.new_project = Some(NewProjectSheet {
            input,
            return_focus,
            _changes: changes,
        });
        cx.notify();
    }

    /// Cancel or Esc: nothing is made.
    pub(super) fn cancel_new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = self.new_project.take() else {
            return;
        };
        match sheet.return_focus {
            Some(focus) => window.focus(&focus, cx),
            None => self.focus_active(window, cx),
        }
        cx.notify();
    }

    /// Create or Enter: a name that is blank once trimmed makes nothing
    /// (#677), and the sheet stays.
    pub(super) fn confirm_new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = &self.new_project else {
            return;
        };
        let name = sheet.input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        self.cancel_new_project(window, cx);
        self.next_op += 1;
        let target = Target::Op(self.next_op);
        self.queue_write(
            target,
            Write::NewProject(NewProjectJob { name }),
            window,
            cx,
        );
    }

    /// At the head of the writer's queue: the name is numbered against the
    /// Projects there are now, the empty Project is written, and only then
    /// does the window take it and show its page.
    pub(super) fn run_new_project(
        &mut self,
        job: NewProjectJob,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        // The Vault's names, and this window's: a rename still waiting to be
        // written counts too.
        let taken: HashSet<String> = (vault.projects().into_iter().map(|p| p.name))
            .chain(self.projects.summaries().into_iter().map(|p| p.name))
            .collect();
        let name = unused_name(&job.name, &taken);
        // Its categories start from Settings › Detection, as Move to New
        // Project's do.
        let skip = (self.preferences.borrow().current.as_ref())
            .map(|p| p.categories.skip.clone())
            .unwrap_or_default();
        let new = mdoc_vault::NewProject {
            name: name.clone(),
            colour: None,
            pinned: false,
        };
        let made = vault
            .create_project(new)
            .and_then(|project| vault.update_project(&project.id, |project| project.skip = skip));
        match made {
            Ok(project) => {
                let id = project.id.clone();
                self.projects.put(project);
                self.show_project_page(&id, Place::Header, window, cx);
            }
            Err(error) => {
                self.notices
                    .post(format!("Could not create “{name}”: {error}"));
            }
        }
        cx.notify();
    }

    pub(super) fn new_project_view(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = self.new_project.as_ref()?;
        // The sheet keeps focus while it is the top dialog: a document that
        // finishes loading behind it must not take the keyboard.
        let field = sheet.input.read(cx).focus_handle(cx);
        if !self.project_ui.explainer
            && !window.has_active_prompt()
            && !self.new_project_group.contains_focused(window, cx)
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
                            .aria_label("New Project")
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
                            .on_action(|_: &crate::menus::About, _, _| {})
                            .on_action(|_: &SwitchDocument, _, _| {})
                            .on_action(|_: &SwitchDocumentBack, _, _| {})
                            .on_action(|_: &ToggleSidebar, _, _| {})
                            .on_action(|_: &RetryDocument, _, _| {})
                            .on_action(|_: &super::shell::ShowHome, _, _| {})
                            .on_action(|_: &super::shell::PasteAsNewTab, _, _| {})
                            .on_action(|_: &PasteAiResponse, _, _| {})
                            .on_action(|_: &PseudonymizeClipboard, _, _| {})
                            .on_action(|_: &super::RestoreClipboard, _, _| {})
                            .on_action(|_: &super::CheckForUpdates, _, _| {})
                            .key_context("UiPanel")
                            .track_focus(&self.new_project_group)
                            .tab_group()
                            .tab_stop(false)
                            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                                ui::cycle(window, cx, Some(&this.new_project_group), false);
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                                ui::cycle(window, cx, Some(&this.new_project_group), true);
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
                            .child(ui::card_title("New Project"))
                            .child(ui::card_text(
                                "Documents you add from its page land in it.",
                                theme,
                            ))
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
                                            "Cancel",
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
                                            "Create",
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

    /// Tests and renders: type `name` into the sheet's field.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(super) fn type_new_project_name(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(sheet) = &self.new_project {
            sheet.input.update(cx, |input, cx| {
                input.set_value(name.into(), cx);
                input.move_to_end(cx);
            });
        }
    }
}

/// `name`, or "name 2", "name 3"… when a Project has it already: the
/// phones' rule (iOS and Android `unusedName`): two Projects of one name
/// could not be told apart in the Project pickers.
pub(super) fn unused_name(name: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(name) {
        return name.to_string();
    }
    (2..)
        .map(|n| format!("{name} {n}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_name_is_numbered_from_two() {
        let taken = |names: &[&str]| names.iter().map(|n| n.to_string()).collect();
        assert_eq!(unused_name("Smith", &taken(&[])), "Smith");
        assert_eq!(unused_name("Smith", &taken(&["Jones"])), "Smith");
        assert_eq!(unused_name("Smith", &taken(&["Smith"])), "Smith 2");
        assert_eq!(
            unused_name("Smith", &taken(&["Smith", "Smith 2", "Smith 4"])),
            "Smith 3"
        );
        // Exact, as on the phones.
        assert_eq!(unused_name("smith", &taken(&["Smith"])), "smith");
    }
}
