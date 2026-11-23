//! Export Project… (#1091 Decision 22): the folder picker, then the
//! Vault writes the Project's Markdown and originals into a new folder in
//! it, off the main thread ([`mdoc_vault::Vault::export_project`]). A
//! notice says what was written, and what could not be.
use super::Workspace;
use gpui::{Context, PathPromptOptions, Window};

impl Workspace {
    /// Ask for a folder and export Project `project` into it.
    // Its consumer is the Project page's ⋯ menu, which item 17 (#1122)
    // brings to vault-desktop; wired in when that merges.
    #[allow(dead_code)]
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
        // Edits not written yet go first, so the export has them.
        self.autosave_all(window, cx);
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
            // Wait for the autosaves queued above, and any still running.
            while this
                .update(cx, |this, _| this.document_writer.busy())
                .unwrap_or(false)
            {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
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

#[allow(dead_code)]
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
