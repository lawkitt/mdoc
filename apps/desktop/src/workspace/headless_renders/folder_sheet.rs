//! The folder sheet.
use super::{Driver, Result};
use crate::workspace::projects::folder_sheet::{DroppedFolder, Pending};
use std::path::Path;

/// Synthetic folders in the temporary profile: a small one, one over the
/// 20-file cap, and Add to a Project. Each is cancelled: nothing opens.
pub(super) fn scenes(d: &mut Driver, document: &Path, project: &str) -> Result<()> {
    let root = document.parent().ok_or("fixture directory")?;
    let write = |folder: &Path, names: &[String]| -> Result<()> {
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        for name in names {
            std::fs::write(folder.join(name), format!("# {name}\n")).map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    let matter = root.join("Petrova v Northwind");
    write(
        &matter,
        &[
            "Engagement letter.md",
            "README.md",
            "Timeline.md",
            "Witness statement.md",
        ]
        .map(String::from),
    )?;
    write(&matter.join("drafts"), &[])?;
    write(&matter, &[".notes.md".into()])?;
    let exhibits = root.join("Exhibits");
    write(
        &exhibits,
        &(1..=24)
            .map(|n| format!("Exhibit {n:02}.md"))
            .collect::<Vec<_>>(),
    )?;
    for (name, folder, project) in [
        ("folder-sheet", &matter, None),
        ("folder-sheet-over-cap", &exhibits, None),
        (
            "folder-sheet-add-to-project",
            &matter,
            Some(project.to_string()),
        ),
    ] {
        let folder = DroppedFolder::read(folder, mdoc_store::document::is_markdown)
            .ok_or("fixture folder")?;
        d.change(|w, window, cx| {
            let pending = Pending { folder, project };
            w.queue_folder(pending, window, cx)
        })?;
        d.capture(name)?;
        d.key("escape")?;
    }
    Ok(())
}
