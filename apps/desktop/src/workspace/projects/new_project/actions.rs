//! New Project's actions: asking, cancelling and confirming the sheet, and
//! the job on the writer that makes the empty Project.

use super::state::unused_name;
use super::{NewProjectJob, NewProjectSheet};
use crate::document::markdown_search::{SearchInput, SearchInputEvent};
use crate::workspace::projects::text;
use crate::workspace::{
    Workspace, projects::project_page::Place, saving::autosave::Write,
    saving::document_writer::Target,
};
use gpui::{Context, Focusable, Window, prelude::*};
use std::collections::HashSet;

impl Workspace {
    /// New Project: ask for the name. A locked Vault opens first.
    pub(in crate::workspace) fn ask_new_project(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        // Over the palette or the Open Project list, the sheet takes their
        // place, and focus goes back where they would have sent it.
        if self.palette.palette.is_some() {
            self.close_palette(window, cx);
        }
        if self.project_ui.list_open {
            self.close_project_list(window, cx);
        }
        let return_focus = window.focused(cx);
        let input = cx.new(|cx| {
            SearchInput::new(cx)
                .with_theme(self.theme.clone())
                .with_key_context("ProjectName")
                .with_placeholder(text::PROJECT_NAME)
                .with_label(text::NAME)
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
    pub(in crate::workspace) fn cancel_new_project(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(in crate::workspace) fn confirm_new_project(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    /// Projects there are now, the empty Project is written once, with its
    /// categories, and only then does the window take it and show its page.
    pub(in crate::workspace) fn run_empty_project(
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
        match vault.create_project_with(new, |project| project.skip = skip) {
            Ok(project) => {
                let id = project.id.clone();
                self.projects.put(project);
                self.show_project_page(&id, Place::Header, window, cx);
            }
            // The sheet has gone: the bell opens with the reason, even if
            // it was closed over an earlier notice.
            Err(error) => {
                let effect = self.notices.post_now(text::could_not_create(&name, &error));
                self.carry_out_notices(effect);
            }
        }
        cx.notify();
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
    pub(in crate::workspace) fn type_new_project_name(
        &mut self,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(sheet) = &self.new_project {
            sheet.input.update(cx, |input, cx| {
                input.set_value(name.into(), cx);
                input.move_to_end(cx);
            });
        }
    }
}
