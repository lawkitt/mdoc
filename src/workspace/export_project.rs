//! Export Project… (#1091 Decision 22): the folder picker, then the
//! Vault writes the Project's Markdown and originals into a new folder in
//! it, off the main thread ([`mdoc_vault::Vault::export_project`]). A
//! notice says what was written, and what could not be.
use super::Workspace;
use gpui::{Context, PathPromptOptions, Window};

impl Workspace {
    /// Export Project… in the Project page's ⋯ menu: ask for a folder and
    /// export Project `project` into it.
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
        // Edits not written yet go first, so the export has them, and a
        // Project save that failed is tried again.
        self.autosave_all(window, cx);
        self.queue_project_writes(window, cx);
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
            // Edits made while the chooser was up go too; then wait for
            // every write queued, and any still running.
            let _ = this.update_in(cx, |this, window, cx| {
                this.autosave_all(window, cx);
                this.queue_project_writes(window, cx);
            });
            while this
                .update(cx, |this, _| this.document_writer.busy())
                .unwrap_or(false)
            {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
            }
            // A member whose write failed would export an older revision:
            // nothing is exported, and the notice says which.
            let unsaved = this
                .update(cx, |this, cx| this.project_unsaved(&project, cx))
                .unwrap_or_default();
            if !unsaved.is_empty() {
                let _ = this.update(cx, |this, cx| {
                    this.notices.post(unsaved_notice(&name, &unsaved));
                    cx.notify();
                });
                return;
            }
            let exported = cx
                .background_executor()
                .spawn(async move { vault.export_project(&project, &into) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.notices.post(exported_notice(&name, exported));
                cx.notify();
            });
        })
        .detach();
    }
}

impl Workspace {
    /// What of Project `project` the window holds that the Vault doesn't:
    /// its open members whose text or review is not written, by name, and
    /// "its names" when the Project's own write failed.
    fn project_unsaved(&self, project: &str, cx: &gpui::App) -> Vec<String> {
        let mut unsaved: Vec<String> = (self.tabs.iter())
            .filter(|tab| {
                tab.view().is_some_and(|view| {
                    let view = view.read(cx);
                    (view.project.link()).is_some_and(|link| link.id == project)
                        && (self.unsaved.contains(&tab.id())
                            || view.unsaved(cx)
                            || view.saved_review() != view.session.document.review)
                })
            })
            .filter_map(|tab| {
                Some(format!(
                    "\u{201c}{}\u{201d}",
                    tab.view()?.read(cx).display_name()
                ))
            })
            .collect();
        if self.project_ui.is_unwritten(project) {
            unsaved.push("its names".into());
        }
        unsaved
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
