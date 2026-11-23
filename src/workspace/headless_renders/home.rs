//! Home's Continue strip, with cards and with nothing to review, and the
//! palette searching titles and Project names. They run on a Vault of their
//! own, so earlier scenes' documents don't crowd the strip; the fixture
//! Vault comes back after.
use super::{Driver, Result};
use mdoc_vault::{
    DocumentOrigin, NewDocument, NewProject, NewReply, ReplyOwner, SourceKind, Vault,
};
use std::{sync::Arc, time::Duration};

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let vault = Vault::open(dir.path().join("vault"), &[7; 32]).map_err(|e| e.to_string())?;
    let project = |name: &str| {
        vault
            .create_project(NewProject {
                name: name.into(),
                colour: None,
                pinned: false,
            })
            .map(|project| project.id)
            .map_err(|e| e.to_string())
    };
    let smith = project("Smith v Jones")?;
    let lease = project("Lease review")?;
    let document = |title: &str, project: Option<&str>, kind: SourceKind| {
        vault
            .create_document(NewDocument {
                project: project.map(Into::into),
                title: title.into(),
                source_kind: kind,
                origin: DocumentOrigin::Ordinary,
                markdown: format!("# {title}\n"),
            })
            .map(|document| document.id)
            .map_err(|e| e.to_string())
    };
    // Reviewed: it never shows.
    let done = document("Engagement letter", None, SourceKind::Word)?;
    vault
        .update_document(&done, |document| document.scanned = true)
        .map_err(|e| e.to_string())?;
    // Oldest to newest, a second apart, so the strip's order is fixed: the
    // oldest is only counted, then the reply, then two documents.
    document("Schedule of works", Some(&lease), SourceKind::Spreadsheet)?;
    std::thread::sleep(Duration::from_millis(1_100));
    vault
        .save_reply(NewReply {
            owner: ReplyOwner::Project(smith.clone()),
            title: "Reply to Witness statement".into(),
            source_app: None,
            markdown: "PERSON_1 confirms the timeline.\n".into(),
        })
        .map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(1_100));
    // Same second or not, newest first and by title agree.
    document("Witness statement", Some(&smith), SourceKind::Pdf)?;
    document("Lease agreement", Some(&lease), SourceKind::Pdf)?;

    // Earlier scenes' writes land in the fixture Vault before it steps aside.
    flush(d)?;
    let fixture = d.change(move |w, window, cx| {
        let fixture = w.vault.vault().cloned();
        w.restore_vault_for_render(Arc::new(vault));
        w.projects.clear();
        w.load_projects(cx);
        w.show_home(window, cx);
        cx.notify();
        fixture
    })?;
    d.capture("home-strip")?;

    d.change(|w, window, cx| {
        w.open_palette(window, cx);
        w.palette_query_for_render("lease", cx);
    })?;
    d.capture("palette-search-title")?;
    d.change(|w, _, cx| w.palette_query_for_render("smith", cx))?;
    d.capture("palette-search-project")?;
    d.change(|w, window, cx| w.close_palette(window, cx))?;

    // Nothing left to review: one reviewed document.
    let empty = Vault::open(dir.path().join("empty"), &[7; 32]).map_err(|e| e.to_string())?;
    let only = empty
        .create_document(NewDocument {
            project: None,
            title: "Engagement letter".into(),
            source_kind: SourceKind::Word,
            origin: DocumentOrigin::Ordinary,
            markdown: "# Engagement letter\n".into(),
        })
        .map_err(|e| e.to_string())?;
    empty
        .update_document(&only.id, |document| document.scanned = true)
        .map_err(|e| e.to_string())?;
    d.change(move |w, _, cx| {
        w.restore_vault_for_render(Arc::new(empty));
        w.projects.clear();
        w.load_projects(cx);
        cx.notify();
    })?;
    d.capture("home-strip-empty")?;

    d.change(move |w, _, cx| {
        if let Some(fixture) = fixture {
            w.restore_vault_for_render(fixture);
        }
        w.projects.clear();
        w.load_projects(cx);
        cx.notify();
    })?;
    d.settle()?;
    drop(dir);
    Ok(())
}

/// Run every pending autosave to its end.
fn flush(d: &mut Driver) -> Result<()> {
    for _ in 0..200 {
        d.cx.advance_clock(Duration::from_secs(1));
        d.settle()?;
        let pending = d.change(|w, _, cx| {
            let ids: Vec<u64> = w.tabs.iter().map(|tab| tab.id()).collect();
            ids.into_iter()
                .any(|id| w.unsaved.contains(&id) || w.write_pending_for(id, cx))
        })?;
        if !pending {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("autosaves did not finish".into())
}
