//! Export Project… through the window: the folder picker, then a notice
//! saying where the Project's files went (#1091 Decision 22).
use super::*;
use mdoc_vault::{DocumentOrigin, NewDocument, NewProject, SourceKind};

#[gpui::test]
fn export_project_writes_the_project_into_the_picked_folder(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home_with(cx, Session::default(), |tabs, _, _| tabs.use_temp_vault());
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault()).unwrap();
    let project = vault
        .create_project(NewProject {
            name: "Smith v. Jones".into(),
            ..NewProject::default()
        })
        .unwrap();
    vault
        .create_document(NewDocument {
            project: Some(project.id.clone()),
            title: "Letter".into(),
            source_kind: SourceKind::Text,
            origin: DocumentOrigin::Ordinary,
            markdown: "Dear John Smith".into(),
        })
        .unwrap();
    let picked = tempfile::tempdir().unwrap();
    // Cancelled: nothing is written and nothing said.
    let id = project.id.clone();
    tabs.update_in(cx, |tabs, window, cx| tabs.export_project(id, window, cx));
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.directories && !options.files && !options.multiple);
        None
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).notice_text().is_none()));
    // Picked: a new folder named after the Project, and a notice.
    let id = project.id.clone();
    tabs.update_in(cx, |tabs, window, cx| tabs.export_project(id, window, cx));
    let into = picked.path().to_path_buf();
    cx.simulate_path_prompt_response(move |_| Some(vec![into]));
    cx.run_until_parked();
    let folder = picked.path().join("Smith v. Jones");
    assert_eq!(
        std::fs::read_to_string(folder.join("Letter.md")).unwrap(),
        "Dear John Smith"
    );
    let notice = cx.update(|_, cx| tabs.read(cx).notice_text()).unwrap();
    assert!(
        notice.starts_with("Exported 1 document of \u{201c}Smith v. Jones\u{201d} to "),
        "{notice}"
    );
    // Into the Vault itself: refused, and said.
    let id = project.id.clone();
    tabs.update_in(cx, |tabs, window, cx| tabs.export_project(id, window, cx));
    let root = vault.root().to_path_buf();
    cx.simulate_path_prompt_response(move |_| Some(vec![root]));
    cx.run_until_parked();
    let notice = cx.update(|_, cx| tabs.read(cx).notice_text()).unwrap();
    assert!(notice.contains("Vault's own folder"), "{notice}");
}

/// Review of #1145 (P2-8): an open member whose save fails is not
/// exported as its older revision: nothing is exported, and the notice
/// names it.
#[gpui::test]
fn export_project_waits_for_its_members_saves_to_succeed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    // The Vault stops at the next Document write, as a full disk would.
    let fail = Arc::new(AtomicBool::new(false));
    let root = dir.path().join("vault");
    let hook: mdoc_vault::FaultHook = {
        let fail = fail.clone();
        Arc::new(move |step| {
            if fail.load(Ordering::SeqCst) && step.kind == mdoc_vault::ObjectKind::Document {
                mdoc_vault::Fault::Crash
            } else {
                mdoc_vault::Fault::Continue
            }
        })
    };
    let vault = mdoc_vault::Vault::open(&root, &[3; 32]).unwrap();
    let project = vault
        .create_project(NewProject {
            name: "Lease".into(),
            ..NewProject::default()
        })
        .unwrap();
    let letter = vault
        .create_document(NewDocument {
            project: Some(project.id.clone()),
            title: "Letter".into(),
            source_kind: SourceKind::Text,
            origin: DocumentOrigin::Ordinary,
            markdown: "Dear landlord".into(),
        })
        .unwrap();
    drop(vault);
    let (tabs, cx) = boot_home_with(cx, Session::default(), move |tabs, _, _| {
        tabs.use_vault(mdoc_vault::Vault::open_with_hook(root, &[3; 32], hook).unwrap())
    });
    let id = mdoc_store::document_id::VaultId::parse(&letter.id).unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_vault_document(id, window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.editor.read(cx).focus_handle(cx), cx)
    });
    fail.store(true, Ordering::SeqCst);
    cx.simulate_input("New text. ");
    // Its save is the export's own, not a pause's later one.
    view.update(cx, |view, _| view.cancel_autosave());
    let picked = tempfile::tempdir().unwrap();
    let project_id = project.id.clone();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.export_project(project_id, window, cx)
    });
    let into = picked.path().to_path_buf();
    cx.simulate_path_prompt_response(move |_| Some(vec![into]));
    for _ in 0..10 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
    }
    let notice = cx.update(|_, cx| tabs.read(cx).notice_text()).unwrap();
    assert!(
        notice.starts_with("Could not export \u{201c}Lease\u{201d}: \u{201c}"),
        "{notice}"
    );
    assert!(notice.ends_with("Nothing was exported."), "{notice}");
    assert!(!picked.path().join("Lease").exists());
}

/// Review of #1150: a Project save that failed is tried again by Export
/// Project, which then exports.
#[gpui::test]
fn export_project_retries_a_failed_project_save(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home_with(cx, Session::default(), |tabs, _, _| tabs.use_temp_vault());
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault()).unwrap();
    let project = vault
        .create_project(NewProject {
            name: "Lease".into(),
            ..NewProject::default()
        })
        .unwrap();
    tabs.update_in(cx, |tabs, _, cx| {
        tabs.load_projects(cx);
        // As a save of its names that failed leaves it.
        tabs.project_ui.mark_unwritten(&project.id);
    });
    let picked = tempfile::tempdir().unwrap();
    let id = project.id.clone();
    tabs.update_in(cx, |tabs, window, cx| tabs.export_project(id, window, cx));
    let into = picked.path().to_path_buf();
    cx.simulate_path_prompt_response(move |_| Some(vec![into]));
    for _ in 0..10 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
    }
    let notice = cx.update(|_, cx| tabs.read(cx).notice_text()).unwrap();
    assert!(notice.starts_with("Exported 0 documents"), "{notice}");
    assert!(picked.path().join("Lease").exists());
}

/// Review of #1145, round 2 (P2): notes typed while Export Project waits
/// for another save are written and exported, not the notes before them.
#[gpui::test]
fn export_project_exports_notes_typed_while_it_waited(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home_with(cx, Session::default(), |tabs, _, _| tabs.use_temp_vault());
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault()).unwrap();
    let project = vault
        .create_project(NewProject {
            name: "Lease".into(),
            ..NewProject::default()
        })
        .unwrap();
    let letter = vault
        .create_document(NewDocument {
            project: Some(project.id.clone()),
            title: "Letter".into(),
            source_kind: SourceKind::Text,
            origin: DocumentOrigin::Ordinary,
            markdown: "Dear landlord".into(),
        })
        .unwrap();
    vault.set_notes(&letter.id, "Old notes.".into()).unwrap();
    let id = mdoc_store::document_id::VaultId::parse(&letter.id).unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_vault_document(id, window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let picked = tempfile::tempdir().unwrap();
    let project_id = project.id.clone();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.export_project(project_id, window, cx)
    });
    let into = picked.path().to_path_buf();
    cx.simulate_path_prompt_response(move |_| Some(vec![into]));
    // A text edit while the chooser was up, so Export waits for its write.
    view.update(cx, |view, cx| {
        view.editor
            .update(cx, |editor, cx| editor.set_text("Dear landlord, again", cx));
        view.cancel_autosave();
    });
    cx.run_until_parked();
    // The text is written; the export waits for its next look. Notes typed
    // now are still in their pause when it looks.
    view.update_in(cx, |view, window, cx| {
        view.type_notes("New notes.", window, cx)
    });
    for _ in 0..3 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(60));
        cx.run_until_parked();
    }
    let folder = picked.path().join("Lease");
    let read = |name: &str| std::fs::read_to_string(folder.join(name)).unwrap();
    assert_eq!(read("Letter.md"), "Dear landlord, again");
    assert_eq!(read("Letter.notes.md"), "New notes.");
}
