//! Export Project… (#1091 Decision 22): the folder picker, then a job on
//! the writer (ADR 0130) writes what the Project's open members show, and
//! the Vault writes the Project's Markdown and originals into a new folder
//! in the one picked, off the main thread
//! ([`mdoc_vault::Vault::export_project`]), with nothing else writing
//! meanwhile. A notice says what was written, and what could not be.
use super::{Workspace, autosave::Ran, autosave::Write, document_writer::Target};
use gpui::{Context, PathPromptOptions, Window};
use std::path::PathBuf;

/// An Export Project waiting for its turn.
pub(super) struct Export {
    project: String,
    name: String,
    into: PathBuf,
}

/// Tests only: what an export's job found stored when it exported.
#[cfg(test)]
pub(super) struct ExportSeen {
    pub(super) folder: PathBuf,
    pub(super) names: Vec<mdoc_pii::restore::ExportedIdentity>,
    /// Each member's id, title, Markdown and notes.
    pub(super) documents: Vec<(String, String, String, String)>,
}

impl Workspace {
    /// Export Project… in the Project page's ⋯ menu: ask for a folder and
    /// export Project `project` into it once the jobs before are done.
    pub(super) fn export_project(
        &mut self,
        project: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        let name = match vault.project(&project) {
            Ok(found) => found.name,
            Err(error) => {
                self.notices
                    .post(format!("Could not export the Project: {error}"));
                cx.notify();
                return;
            }
        };
        let folder = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(format!("Export \u{201c}{name}\u{201d} Here").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let into = match folder.await {
                Ok(Ok(Some(mut folders))) => folders.pop(),
                // Cancelled.
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |this, cx| {
                        this.notices.post(format!(
                            "Could not export \u{201c}{name}\u{201d}: the folder chooser \
                             didn't open. {error}"
                        ));
                        cx.notify();
                    });
                    None
                }
            };
            let Some(into) = into else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                if this.quit_flow.quitting() {
                    return;
                }
                this.next_op += 1;
                let target = Target::Op(this.next_op);
                let export = Export {
                    project,
                    name,
                    into,
                };
                this.queue_write(target, Write::Export(export), window, cx);
            });
        })
        .detach();
    }

    /// Export at the head of the writer's queue: what every open member
    /// shows is written first (its names, then its text, then its notes),
    /// and the Project's names; if any of it can't be, nothing is exported.
    /// The export runs off the main thread, and the job holds the queue
    /// until it ends, so it reads one stored state.
    pub(super) fn run_export(
        &mut self,
        export: Export,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Ran {
        let Some(vault) = self.vault.vault().cloned() else {
            return Ran::Ended;
        };
        let Export {
            project,
            name,
            into,
        } = export;
        for tab in self.open_members(&project, cx) {
            if self.write_tab_now(tab, &vault, cx).is_err() {
                // Its edit is in no file: the notice below names it.
                self.unsaved.insert(tab);
                continue;
            }
            let document = self.tab_document(tab, cx);
            let notes_due =
                (self.tab_view(tab)).is_some_and(|view| view.read(cx).notes.needs_write());
            if let (Some(document), true) = (document, notes_due) {
                self.run_notes_save(&document, window, cx);
            }
        }
        let names_due = self.project_ui.is_unwritten(&project)
            || self.document_writer.has(&Target::Project(project.clone()));
        if names_due {
            match self.write_now(&vault, &[Some(project.clone())]) {
                Ok(()) => {
                    self.project_ui.take_unwritten(&project);
                }
                Err(_) => self.project_ui.mark_unwritten(&project),
            }
        }
        // A member whose write failed would export an older revision:
        // nothing is exported, and the notice says which.
        let unsaved = self.project_unsaved(&project, cx);
        if !unsaved.is_empty() {
            self.notices.post(unsaved_notice(&name, &unsaved));
            cx.notify();
            return Ran::Ended;
        }
        let task = cx.background_executor().spawn({
            let (vault, project) = (vault.clone(), project.clone());
            async move { vault.export_project(&project, &into) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let exported = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                #[cfg(test)]
                if let Ok(exported) = &exported {
                    this.saw_export(&vault, &project, exported.folder.clone());
                }
                this.notices.post(exported_notice(&name, exported));
                cx.notify();
                this.advance_writer(window, cx);
            });
        })
        .detach();
        Ran::Holds
    }

    /// The tabs of Project `project`'s documents: linked to it, or filed
    /// there by the Vault.
    fn open_members(&self, project: &str, cx: &gpui::App) -> Vec<u64> {
        (self.tabs.iter())
            .filter(|tab| {
                tab.view().is_some_and(|view| {
                    let view = view.read(cx);
                    let filed = (view.session.document.vault.as_ref())
                        .is_some_and(|id| self.project_of(id.as_str()).as_deref() == Some(project));
                    (view.project.link()).is_some_and(|link| link.id == project) || filed
                })
            })
            .map(|tab| tab.id())
            .collect()
    }

    /// What of Project `project` the window holds that the Vault doesn't:
    /// its open members whose text, review or notes are not written, by
    /// name, and "its names" when the Project's own write failed.
    fn project_unsaved(&self, project: &str, cx: &gpui::App) -> Vec<String> {
        let mut unsaved: Vec<String> = (self.open_members(project, cx).into_iter())
            .filter_map(|id| {
                let view = self.tabs.get(id)?.view()?.read(cx);
                let behind = self.unsaved.contains(&id)
                    || view.unsaved(cx)
                    || view.notes.unsaved()
                    || view.saved_review() != view.session.document.review;
                behind.then(|| format!("\u{201c}{}\u{201d}", view.display_name()))
            })
            .collect();
        if self.project_ui.is_unwritten(project) {
            unsaved.push("its names".into());
        }
        unsaved
    }

    /// Tests only: keep what the export read, as the Vault stores it now,
    /// while the job still holds the queue.
    #[cfg(test)]
    fn saw_export(&mut self, vault: &mdoc_vault::Vault, project: &str, folder: PathBuf) {
        let names = (vault.project(project))
            .map(|project| project.mapping.exported())
            .unwrap_or_default();
        let documents = (vault.documents_in(Some(project)).into_iter())
            .filter_map(|summary| vault.document(&summary.id).ok())
            .map(|d| (d.id, d.title, d.markdown, d.notes))
            .collect();
        self.exports_seen.push(ExportSeen {
            folder,
            names,
            documents,
        });
    }
}

fn unsaved_notice(name: &str, unsaved: &[String]) -> String {
    format!(
        "Could not export \u{201c}{name}\u{201d}: {} could not be saved, so the export \
         would miss the latest changes. Nothing was exported.",
        unsaved.join(", ")
    )
}

fn exported_notice(
    name: &str,
    exported: Result<mdoc_vault::Exported, mdoc_vault::VaultError>,
) -> String {
    let exported = match exported {
        Ok(exported) => exported,
        Err(error) => return format!("Could not export \u{201c}{name}\u{201d}: {error}"),
    };
    let folder = dunce::simplified(&exported.folder).display().to_string();
    let written = match exported.documents {
        1 => "1 document".to_string(),
        n => format!("{n} documents"),
    };
    let mut notice = format!("Exported {written} of \u{201c}{name}\u{201d} to {folder}.");
    if !exported.failed.is_empty() {
        let failed: Vec<String> = (exported.failed.iter())
            .map(|(title, error)| format!("\u{201c}{title}\u{201d} ({error})"))
            .collect();
        notice.push_str(&format!(" Not written in full: {}.", failed.join(", ")));
    }
    notice
}
