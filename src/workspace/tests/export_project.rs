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
