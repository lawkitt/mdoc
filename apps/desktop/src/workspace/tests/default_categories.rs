//! Settings › Detection › Categories to detect: the default for
//! documents outside a Project and the list each new Project starts from.
use super::*;
use mdoc_pii::Category;

/// Tick or untick `category` in Settings and save, as a chip click does.
fn set_default(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, category: Category, on: bool) {
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    panel.update(cx, |panel, cx| panel.set_detects(category, on, cx));
    cx.run_until_parked();
}

fn projects(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<mdoc_vault::Project> {
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        (w.projects.summaries().into_iter())
            .filter_map(|summary| w.projects.project(&summary.id).cloned())
            .collect()
    })
}

fn originals(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        view.read(cx)
            .pii
            .review()
            .variants()
            .iter()
            .map(|v| v.original.to_string())
            .collect()
    })
}

/// Scan proposals for `names` in the active tab, as a finished scan adds them.
fn detect(view: &Entity<DocumentView>, cx: &mut VisualTestContext, names: &[(&str, Category)]) {
    view.update(cx, |view, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let detections = names
            .iter()
            .map(|(name, category)| {
                let start = text.find(name).unwrap();
                mdoc_pii::Detection {
                    range: start..start + name.len(),
                    category: *category,
                    score: 0.9,
                    recognizer: mdoc_pii::Recognizer::Model,
                }
            })
            .collect();
        view.pii.review_mut().ingest(&text, detections).unwrap();
        view.review_changed(cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_saved_mapping_project_starts_from_the_default_and_keeps_its_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    set_default(&tabs, cx, Category::Address, false);
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = new_project(&tabs, cx, tab);
    assert_eq!(projects(&tabs, cx)[0].skip, ["ADDRESS"]);
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).scan_skip(), [Category::Address].into());
    });

    // A later Settings change leaves the existing Project and its member
    // alone.
    set_default(&tabs, cx, Category::Address, true);
    set_default(&tabs, cx, Category::Email, false);
    assert_eq!(projects(&tabs, cx)[0].skip, ["ADDRESS"]);
    assert_eq!(vault(&tabs, cx).project(&id).unwrap().skip, ["ADDRESS"]);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).scan_skip(), [Category::Address].into());
    });
    // Leaving the Project hands the tab back to the default.
    move_tab(&tabs, cx, tab, None);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).scan_skip(), [Category::Email].into());
    });
}

#[gpui::test]
fn a_folder_opened_as_a_project_starts_from_the_default(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Matter");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("a.md"), "Text").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    set_default(&tabs, cx, Category::Phone, false);
    set_default(&tabs, cx, Category::Bank, false);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![folder], window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    click_toolbar(cx, "folder-primary");
    cx.run_until_parked();
    let created = projects(&tabs, cx);
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].name, "Matter");
    assert_eq!(created[0].skip, ["PHONE", "BANK"]);
    // Its member scans with the Project's list.
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(view.read(cx).project.link().is_some());
        assert_eq!(
            view.read(cx).scan_skip(),
            [Category::Phone, Category::Bank].into()
        );
    });
}

#[gpui::test]
fn unticking_withdraws_unapplied_proposals_only_in_tabs_without_a_project(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let text = "Anna Petrova met Bob Day and Cara Lin at 1 Main St.";
    // A Project's document: its Project decides, whatever Settings says.
    let member = open_applied(
        &tabs,
        cx,
        dir.path(),
        "member.md",
        text,
        &[("Anna Petrova", Category::Person)],
    );
    new_project(&tabs, cx, member);
    let member_view = active(&tabs, cx);
    detect(&member_view, cx, &[("Bob Day", Category::Person)]);
    // A document without a Project: an applied name, a manual one and scan proposals.
    open_applied(
        &tabs,
        cx,
        dir.path(),
        "loose.md",
        &format!("{text} "),
        &[("Anna Petrova", Category::Person)],
    );
    let loose = active(&tabs, cx);
    detect(
        &loose,
        cx,
        &[
            ("Bob Day", Category::Person),
            ("1 Main St", Category::Address),
        ],
    );
    loose.update(cx, |view, cx| {
        let source = view.editor.read(cx).text().to_owned();
        let start = source.find("Cara Lin").unwrap();
        view.pii
            .review_mut()
            .add_manual(&source, start..start + 8, Category::Person)
            .unwrap();
    });
    let before = originals(&loose, cx);
    assert!(before.contains(&"Bob Day".to_string()), "{before:?}");

    set_default(&tabs, cx, Category::Person, false);
    let after = originals(&loose, cx);
    assert!(!after.contains(&"Bob Day".to_string()), "{after:?}");
    assert!(
        after.contains(&"Cara Lin".to_string()),
        "manual additions stay"
    );
    assert!(
        after.contains(&"1 Main St".to_string()),
        "other categories stay"
    );
    cx.update(|_, cx| {
        let view = loose.read(cx);
        assert!(view.editor.read(cx).text().starts_with("PERSON_1 met"));
        assert_eq!(view.pii.review().applied().len(), 1, "applied stays");
    });
    assert!(
        originals(&member_view, cx).contains(&"Bob Day".to_string()),
        "the document follows its Project"
    );
}
