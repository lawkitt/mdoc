//! Deleting a Project: the question, then a job on the writer that writes
//! its names, moves or deletes its documents and forgets it; and the
//! question again when its documents changed meanwhile.

use super::text;
use crate::workspace::{
    Workspace,
    saving::autosave::{Op, Ran, Write},
    saving::document_writer::Target,
};
use gpui::{Context, PromptLevel, Window};
use mdoc_store::document_id::{DocumentId, VaultId};
use mdoc_vault::{ProjectDocuments, VaultError};

impl Workspace {
    /// Ask, then delete the Project: a non-empty one asks whether its
    /// documents move to Unfiled or are deleted with it.
    pub(in crate::workspace) fn delete_project(
        &mut self,
        project_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self.projects.project(&project_id).map(|p| p.name.clone()) else {
            return;
        };
        // What the question asks about: only these may go with it.
        let members = self.project_members(&project_id);
        #[cfg(test)]
        {
            self.projects.asked_to_delete.push_back(members.clone());
        }
        let count = members.len();
        let answer = if count == 0 {
            window.prompt(
                PromptLevel::Warning,
                &text::delete_project_question(&name),
                Some(text::DELETE_EMPTY_DETAIL),
                &[text::DELETE_BUTTON, crate::ui::text::CANCEL],
                cx,
            )
        } else {
            window.prompt(
                PromptLevel::Warning,
                &text::delete_project_question(&name),
                Some(&text::delete_detail(count)),
                &[
                    gpui::PromptButton::new(text::MOVE_TO_UNFILED),
                    gpui::PromptButton::new(text::DELETE_DOCUMENTS),
                    gpui::PromptButton::cancel(crate::ui::text::CANCEL),
                ],
                cx,
            )
        };
        cx.spawn_in(window, async move |this, cx| {
            // An empty Project has no documents to move.
            let documents = match answer.await.ok() {
                Some(0) => ProjectDocuments::MoveToUnfiled,
                Some(1) if count > 0 => ProjectDocuments::Delete,
                _ => return,
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.forget_project(&project_id, documents, members, window, cx)
            });
        })
        .detach();
    }

    /// Project `project_id`'s documents as the Vault lists them now.
    pub(in crate::workspace) fn project_members(&self, project_id: &str) -> Vec<VaultId> {
        let Some(vault) = self.vault.vault() else {
            return Vec::new();
        };
        (vault.documents_in(Some(project_id)).into_iter())
            .filter_map(|summary| VaultId::parse(&summary.id))
            .collect()
    }

    /// Delete Project `project_id` now, its documents going as `documents`
    /// says, while they are still `members`, the ones the user was asked
    /// about. Its open members load again as Unfiled, or close unwritten.
    pub(in crate::workspace) fn forget_project(
        &mut self,
        project_id: &str,
        documents: ProjectDocuments,
        members: Vec<VaultId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.vault.vault().is_none() || self.quit_flow.quitting() {
            return;
        }
        // What the members show is read when the delete runs.
        for member in &members {
            self.projects.moving.insert(member.clone());
        }
        self.next_op += 1;
        let op = Op::Delete {
            project: project_id.to_string(),
            documents,
            members,
        };
        let target = Target::Op(self.next_op);
        // Any document id stands in: a delete names its members itself.
        let id = VaultId::parse(&"0".repeat(32)).expect("a valid id");
        self.queue_write(target, Write::Op(id, op), window, cx);
    }

    /// Run a Project delete at the head of the writer's queue; the job ends
    /// once its members have loaded again.
    pub(in crate::workspace) fn run_delete(
        &mut self,
        project_id: String,
        documents: ProjectDocuments,
        members: Vec<VaultId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Ran {
        let Some(vault) = self.vault.vault().cloned() else {
            for member in &members {
                self.projects.moving.remove(member);
            }
            return Ran::Ended;
        };
        // The Project's documents changed since the question (an import, a
        // move): nothing is written, and the question comes again. The
        // Vault checks once more under its lock, for other handles.
        let mut now: Vec<String> = (self.project_members(&project_id).iter())
            .map(|id| id.as_str().to_string())
            .collect();
        let mut asked: Vec<String> = members.iter().map(|m| m.as_str().to_string()).collect();
        now.sort();
        asked.sort();
        if now != asked {
            return self.ask_delete_again(project_id, &members, window, cx);
        }
        // Moved to Unfiled, each open member loads again from the Vault:
        // what it shows now is written first, edits made while the delete
        // waited included. One that can't be written stops the delete
        // before anything moves, so no stored revision replaces its edit.
        if documents == ProjectDocuments::MoveToUnfiled {
            for member in &members {
                let Some(tab) = self.find_document(&DocumentId::Vault(member.clone()), None, cx)
                else {
                    continue;
                };
                // A name applied while the delete waited joins the
                // Project's names, which each member takes to Unfiled; an
                // alias reconciled on the way is in the text written next.
                if let Err(error) = self.write_tab_now(tab, &vault, cx) {
                    for member in &members {
                        self.projects.moving.remove(member);
                    }
                    // Its edit is in no file: eviction keeps it.
                    self.unsaved.insert(tab);
                    let name = (self.tabs.get(tab).and_then(|tab| tab.view().cloned()))
                        .map(|view| view.read(cx).display_name())
                        .unwrap_or_default();
                    self.notices
                        .post(text::delete_member_not_saved(&name, &error));
                    cx.notify();
                    return Ran::Ended;
                }
            }
            // The Project's names as the window holds them, written before
            // anything moves; if they can't be, nothing moves.
            if let Err(error) = self.write_now(&vault, &[Some(project_id.clone())]) {
                for member in &members {
                    self.projects.moving.remove(member);
                }
                self.project_ui.mark_unwritten(&project_id);
                self.notices.post(text::delete_names_not_saved(&error));
                cx.notify();
                return Ran::Ended;
            }
        }
        // Only what was confirmed is deleted: the Vault compares the
        // Project's documents with the ones asked about under its write
        // lock, so an import or a move since stops it.
        let confirmed: Vec<String> = members.iter().map(|m| m.as_str().to_string()).collect();
        let result = vault.delete_project_with(&project_id, documents, &confirmed);
        if let Ok(false) = result {
            return self.ask_delete_again(project_id, &members, window, cx);
        }
        let result = result.map(|_| ());
        let tabs: Vec<(u64, VaultId)> = (members.iter())
            .filter_map(|member| {
                let tab = self.find_document(&DocumentId::Vault(member.clone()), None, cx)?;
                Some((tab, member.clone()))
            })
            .collect();
        match result {
            Ok(()) => {
                if (self.project_ui.page.as_ref()).is_some_and(|page| page.id() == project_id) {
                    // Home takes the deleted Project page's stop.
                    self.show_home_as(
                        crate::workspace::sidebar::navigation::Arrival::Replace,
                        window,
                        cx,
                    );
                }
                self.project_gone(&project_id, cx);
                for (tab, member) in tabs {
                    match documents {
                        ProjectDocuments::MoveToUnfiled => self.reload_tab(tab, member, window, cx),
                        ProjectDocuments::Delete => {
                            self.projects.moving.remove(&member);
                            self.remove(tab, window, cx);
                        }
                    }
                }
                for member in &members {
                    if self
                        .find_document(&DocumentId::Vault(member.clone()), None, cx)
                        .is_none()
                    {
                        self.projects.moving.remove(member);
                    }
                }
            }
            Err(error) => {
                // What it moved or deleted before the failure is as the
                // Vault holds it now.
                if let Ok(project) = vault.project(&project_id) {
                    self.projects.put(project);
                }
                for (tab, member) in tabs {
                    match documents {
                        // Written at the head: the Vault holds what it showed.
                        ProjectDocuments::MoveToUnfiled => self.reload_tab(tab, member, window, cx),
                        // Not written first: one still there keeps its view
                        // and its edits; one gone closes.
                        ProjectDocuments::Delete => {
                            self.projects.moving.remove(&member);
                            let gone = matches!(
                                vault.document(member.as_str()),
                                Err(VaultError::NotFound { .. })
                            );
                            if gone {
                                self.remove(tab, window, cx);
                            }
                        }
                    }
                }
                for member in &members {
                    if self
                        .find_document(&DocumentId::Vault(member.clone()), None, cx)
                        .is_none()
                    {
                        self.projects.moving.remove(member);
                    }
                }
                self.notices.post(text::could_not_delete(&error));
            }
        }
        // Its own replies went with it.
        self.drop_gone_reply_page(window, cx);
        self.refresh_saved_replies(cx);
        self.checkpoint(window, cx);
        cx.notify();
        self.hold_for_reloads()
    }

    /// A Project delete found other documents than its question named:
    /// nothing is deleted, and the question comes again for them.
    pub(super) fn ask_delete_again(
        &mut self,
        project_id: String,
        members: &[VaultId],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Ran {
        for member in members {
            self.projects.moving.remove(member);
        }
        self.notices.post(text::DELETE_CHANGED);
        self.delete_project(project_id, window, cx);
        cx.notify();
        Ran::Ended
    }
}
