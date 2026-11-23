//! The folder sheet: a dropped folder shows its files before anything
//! opens; Open as New Project and Add to a Project import into it; one
//! folder opens at most 20 new tabs.
use super::*;
use crate::workspace::ShowHome;
use crate::workspace::folder_sheet::{CAP, CAP_NOTE, Status};
use gpui::Action;

/// A folder `name` in `dir` holding Markdown files `files`.
fn folder(dir: &std::path::Path, name: &str, files: &[&str]) -> PathBuf {
    let folder = dir.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    for file in files {
        std::fs::write(folder.join(file), format!("Text of {file}")).unwrap();
    }
    folder
}

fn drop_paths(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, paths: Vec<PathBuf>) {
    tabs.update_in(cx, |tabs, window, cx| tabs.open_paths(paths, window, cx));
    cx.run_until_parked();
    draw(cx);
}

fn sheet_rows(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<(String, Status, bool)> {
    cx.update(|_, cx| {
        tabs.read(cx)
            .folder_sheets
            .sheet()
            .expect("a folder sheet shows")
            .rows()
            .iter()
            .map(|r| (r.name.clone(), r.status.clone(), r.ticked))
            .collect()
    })
}

fn sheet_open(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| tabs.read(cx).folder_sheets.open())
}

fn focused_row(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<usize> {
    cx.update(|window, cx| tabs.read(cx).focused_folder_row(window))
}

/// Every tab's Project, in sidebar order.
/// The live views' Projects, in `tab_names`' order.
fn tab_projects(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Option<String>> {
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        let mut named: Vec<(String, Option<String>)> = (w.tabs.iter())
            .map(|t| {
                let name = t
                    .view()
                    .map(|view| view.read(cx).display_name())
                    .unwrap_or_else(|| tab_label(t, cx).name);
                (name, w.sidebar_project_link(t, cx).map(|l| l.id))
            })
            .collect();
        named.sort_by_key(|(name, _)| name.to_lowercase());
        named.into_iter().map(|(_, project)| project).collect()
    })
}

/// Every Project the window has, by name.
fn project_names(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        (tabs.read(cx).projects.summaries().into_iter())
            .map(|p| p.name)
            .collect()
    })
}

/// Load every tab, so members get their names as they load.
fn load_all(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) {
    let ids: Vec<_> = cx.update(|_, cx| tabs.read(cx).tabs.iter().map(|t| t.id()).collect());
    for id in ids {
        tabs.update_in(cx, |tabs, window, cx| tabs.activate(id, window, cx));
        cx.run_until_parked();
    }
}

#[gpui::test]
fn a_dropped_folder_shows_its_files_before_anything_opens(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = folder(
        dir.path(),
        "Petrova v Northwind",
        &["b.md", "a.md", "README.md"],
    );
    std::fs::create_dir_all(path.join("drafts")).unwrap();
    std::fs::write(path.join("notes.bin"), [0u8, 1, 2]).unwrap();
    std::fs::write(path.join(".draft.md"), "Hidden").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    drop_paths(&tabs, cx, vec![path.clone()]);
    assert!(cx.debug_bounds("folder-sheet").is_some());
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(
            no_documents(tabs),
            "nothing opens before the sheet is answered"
        );
        let sheet = tabs.folder_sheets.sheet().unwrap();
        assert_eq!(sheet.name(), "Petrova v Northwind");
        assert_eq!(sheet.skipped(), Some("1 subfolder, 1 hidden file skipped"));
        assert_eq!(sheet.primary_label(), "Open as New Project");
    });
    // Sorted by name, all ticked, README included: no name rules.
    assert_eq!(
        sheet_rows(&tabs, cx),
        [
            ("a.md".into(), Status::New, true),
            ("b.md".into(), Status::New, true),
            ("README.md".into(), Status::New, true),
        ]
    );
    // Open as New Project: the files are imported into a Project named
    // after the folder.
    click_toolbar(cx, "folder-primary");
    load_all(&tabs, cx);
    assert!(!sheet_open(&tabs, cx));
    assert_eq!(tab_names(&tabs, cx), ["a", "b", "README"]);
    assert_eq!(project_names(&tabs, cx), ["Petrova v Northwind"]);
    let project = cx.update(|_, cx| tabs.read(cx).projects.summaries()[0].id.clone());
    assert_eq!(tab_projects(&tabs, cx), vec![Some(project.clone()); 3]);
    assert_eq!(vault(&tabs, cx).documents_in(Some(&project)).len(), 3);
}

#[gpui::test]
fn an_empty_folder_posts_a_notice_and_shows_no_sheet(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = folder(dir.path(), "Empty", &[]);
    std::fs::create_dir_all(path.join("sub")).unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    drop_paths(&tabs, cx, vec![path]);
    assert!(!sheet_open(&tabs, cx));
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).notices.message(),
            Some("“Empty” has no supported files; 1 subfolder inside “Empty” skipped.")
        );
    });
}

#[gpui::test]
fn open_files_opens_them_unfiled(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = folder(dir.path(), "Matter", &["a.md", "b.md"]);
    let (tabs, cx) = boot_home(cx, Session::default());
    drop_paths(&tabs, cx, vec![path]);
    click_toolbar(cx, "folder-open-files");
    load_all(&tabs, cx);
    assert_eq!(tab_names(&tabs, cx), ["a", "b"]);
    assert_eq!(tab_projects(&tabs, cx), [None, None]);
    assert!(project_names(&tabs, cx).is_empty());
}

#[gpui::test]
fn one_folder_opens_at_most_twenty_new_tabs(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let names: Vec<String> = (1..=25).map(|n| format!("Exhibit {n:02}.md")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let path = folder(dir.path(), "Exhibits", &refs);
    let (tabs, cx) = boot_home(cx, Session::default());
    // One file is already open: it stays ticked and is not counted.
    drop_paths(&tabs, cx, vec![path.join("Exhibit 25.md")]);
    drop_paths(&tabs, cx, vec![path]);
    let rows = sheet_rows(&tabs, cx);
    assert_eq!(rows.len(), 25);
    assert!(rows[..CAP].iter().all(|r| r.2), "the first 20 start ticked");
    assert!(
        rows[CAP..24].iter().all(|r| !r.2),
        "the rest start unticked"
    );
    assert_eq!(rows[24], ("Exhibit 25.md".into(), Status::Open, true));
    // A 21st new tick is refused, and the sheet says why.
    assert!(cx.debug_bounds("folder-sheet-note").is_some());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.toggle_folder_row(CAP, window, cx)
    });
    draw(cx);
    cx.update(|_, cx| {
        let sheet = tabs.read(cx).folder_sheets.sheet().unwrap();
        assert!(sheet.refused());
        assert!(!sheet.rows()[CAP].ticked);
    });
    // Untick one, and another can take its place.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.toggle_folder_row(0, window, cx);
        tabs.toggle_folder_row(CAP, window, cx);
    });
    cx.update(|_, cx| {
        let sheet = tabs.read(cx).folder_sheets.sheet().unwrap();
        assert!(!sheet.refused());
        assert!(sheet.rows()[CAP].ticked && !sheet.rows()[0].ticked);
    });
    click_toolbar(cx, "folder-open-files");
    assert_eq!(vault(&tabs, cx).documents().len(), 1 + CAP);
}

#[gpui::test]
fn the_refusal_reads_up_to_twenty_files_at_a_time(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let names: Vec<String> = (1..=21).map(|n| format!("{n:02}.md")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let path = folder(dir.path(), "Big", &refs);
    let (tabs, cx) = boot_home(cx, Session::default());
    drop_paths(&tabs, cx, vec![path]);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.toggle_folder_row(CAP, window, cx)
    });
    draw(cx);
    assert_eq!(CAP_NOTE, "Up to 20 files at a time");
    cx.update(|_, cx| assert!(tabs.read(cx).folder_sheets.sheet().unwrap().refused()));
}

#[gpui::test]
fn a_folder_with_files_in_a_project_adds_to_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let project = new_project(&tabs, cx, first);
    std::fs::write(dir.path().join("letter.md"), "A letter").unwrap();
    std::fs::write(dir.path().join("memo.md"), "A memo").unwrap();
    drop_paths(&tabs, cx, vec![dir.path().to_path_buf()]);
    cx.update(|_, cx| {
        let sheet = tabs.read(cx).folder_sheets.sheet().unwrap();
        assert_eq!(sheet.primary_label(), "Add to “contract”");
    });
    assert_eq!(
        sheet_rows(&tabs, cx),
        [
            (
                "contract.md".into(),
                Status::InProject("contract".into()),
                true
            ),
            ("letter.md".into(), Status::New, true),
            ("memo.md".into(), Status::New, true),
        ]
    );
    // Enter is the primary button.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    load_all(&tabs, cx);
    assert!(!sheet_open(&tabs, cx));
    assert_eq!(tab_names(&tabs, cx), ["contract", "letter", "memo"]);
    assert_eq!(tab_projects(&tabs, cx), vec![Some(project.clone()); 3]);
    assert_eq!(project_names(&tabs, cx).len(), 1, "no new Project");
    assert_eq!(vault(&tabs, cx).documents_in(Some(&project)).len(), 3);
}

#[gpui::test]
fn a_folder_dropped_on_a_project_row_adds_to_that_project(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let project = new_project(&tabs, cx, first);
    let other = tempfile::tempdir().unwrap();
    let path = folder(other.path(), "Unrelated", &["note.md"]);
    // A real drop on the Project's row.
    draw(cx);
    let header: &'static str = Box::leak(format!("tree-project-{project}").into_boxed_str());
    let header = cx.debug_bounds(header).unwrap();
    drag_files(cx, &[path], header.center());
    drop_files(cx, header.center());
    cx.update(|_, cx| {
        let sheet = tabs.read(cx).folder_sheets.sheet().unwrap();
        assert_eq!(sheet.primary_label(), "Add to “contract”");
    });
    click_toolbar(cx, "folder-primary");
    load_all(&tabs, cx);
    assert_eq!(tab_names(&tabs, cx), ["contract", "note"]);
    assert_eq!(tab_projects(&tabs, cx), vec![Some(project); 2]);
}

#[gpui::test]
fn files_dropped_on_a_project_row_are_imported_into_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let project = new_project(&tabs, cx, first);
    let other = tempfile::tempdir().unwrap();
    let note = other.path().join("note.md");
    std::fs::write(&note, "A note").unwrap();
    draw(cx);
    let header: &'static str = Box::leak(format!("tree-project-{project}").into_boxed_str());
    let header = cx.debug_bounds(header).unwrap();
    drag_files(cx, &[note], header.center());
    drop_files(cx, header.center());
    load_all(&tabs, cx);
    assert_eq!(tab_names(&tabs, cx), ["contract", "note"]);
    assert_eq!(tab_projects(&tabs, cx), vec![Some(project); 2]);
}

#[gpui::test]
fn an_open_unfiled_file_moves_into_the_new_project(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = folder(dir.path(), "Matter", &["a.md", "b.md"]);
    let (tabs, cx) = boot_home(cx, Session::default());
    drop_paths(&tabs, cx, vec![path.join("a.md")]);
    drop_paths(&tabs, cx, vec![path]);
    assert_eq!(
        sheet_rows(&tabs, cx),
        [
            ("a.md".into(), Status::Open, true),
            ("b.md".into(), Status::New, true),
        ]
    );
    click_toolbar(cx, "folder-primary");
    load_all(&tabs, cx);
    assert_eq!(tab_names(&tabs, cx), ["a", "b"]);
    let project = cx.update(|_, cx| tabs.read(cx).projects.summaries()[0].id.clone());
    assert_eq!(tab_projects(&tabs, cx), vec![Some(project); 2]);
}

#[gpui::test]
fn an_open_file_in_another_project_stays_there(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let theirs = new_project(&tabs, cx, first);
    // A second Project holds more of the folder's open files: the target.
    let mut others = Vec::new();
    for name in ["Other.md", "b.md"] {
        let text = format!("Boris Ivanov wrote {name}.");
        others.push(open_applied(
            &tabs,
            cx,
            dir.path(),
            name,
            &text,
            &[("Boris Ivanov", Person)],
        ));
    }
    let other = new_project(&tabs, cx, others[0]);
    move_tab(&tabs, cx, others[1], Some(&other));
    std::fs::write(dir.path().join("c.md"), "C").unwrap();
    drop_paths(&tabs, cx, vec![dir.path().to_path_buf()]);
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).folder_sheets.sheet().unwrap().primary_label(),
            "Add to “Other”"
        );
    });
    let rows = sheet_rows(&tabs, cx);
    assert!(rows.contains(&(
        "contract.md".into(),
        Status::InProject("contract".into()),
        true
    )));
    click_toolbar(cx, "folder-primary");
    load_all(&tabs, cx);
    let names = tab_names(&tabs, cx);
    assert_eq!(names.len(), 4);
    let projects = tab_projects(&tabs, cx);
    for (name, project) in names.iter().zip(&projects) {
        let want = if *name == "contract" { &theirs } else { &other };
        assert_eq!(project.as_ref(), Some(want), "{name}");
    }
}

#[gpui::test]
fn several_folders_get_their_sheets_in_turn(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let first = folder(dir.path(), "First", &["one.md"]);
    let second = folder(dir.path(), "Second", &["two.md", "three.md"]);
    let loose = dir.path().join("loose.md");
    std::fs::write(&loose, "Loose").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    drop_paths(&tabs, cx, vec![first, loose, second]);
    // Plain files open at once; the folders wait.
    assert_eq!(tab_names(&tabs, cx), ["loose"]);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.folder_sheets.sheet().unwrap().name(), "First");
        assert_eq!(tabs.folder_sheets.queued(), 1);
    });
    click_toolbar(cx, "folder-cancel");
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).folder_sheets.sheet().unwrap().name(),
            "Second"
        );
    });
    assert_eq!(tab_names(&tabs, cx), ["loose"], "Cancel opens nothing");
    click_toolbar(cx, "folder-open-files");
    assert!(!sheet_open(&tabs, cx));
    assert_eq!(tab_names(&tabs, cx), ["loose", "three", "two"]);
}

#[gpui::test]
fn the_sheet_works_from_the_keyboard(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = folder(dir.path(), "Keys", &["a.md", "b.md", "c.md"]);
    let (tabs, cx) = boot_home(cx, Session::default());
    drop_paths(&tabs, cx, vec![path.clone()]);
    assert_eq!(
        focused_row(&tabs, cx),
        Some(0),
        "focus starts on the first row"
    );
    // ↓ moves, Space toggles, ↑ moves back; ↑ at the top stays.
    cx.simulate_keystrokes("down down space up up up");
    draw(cx);
    assert_eq!(focused_row(&tabs, cx), Some(0));
    let ticks: Vec<bool> = sheet_rows(&tabs, cx).into_iter().map(|r| r.2).collect();
    assert_eq!(ticks, [true, true, false]);
    // Tab leaves the list for the buttons and stays inside the sheet.
    cx.simulate_keystrokes("tab");
    draw(cx);
    assert_eq!(focused_row(&tabs, cx), None);
    cx.update(|window, cx| {
        let focus = tabs.read(cx).folder_sheet_focus.as_ref().unwrap();
        assert!(focus.group.contains_focused(window, cx));
    });
    cx.simulate_keystrokes("shift-tab");
    draw(cx);
    assert_eq!(focused_row(&tabs, cx), Some(0));
    // Esc cancels: nothing opens and focus goes back.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!sheet_open(&tabs, cx));
    cx.update(|_, cx| assert!(no_documents(tabs.read(cx))));
    // Enter opens as a new Project, with the ticks as they are.
    drop_paths(&tabs, cx, vec![path]);
    cx.simulate_keystrokes("down space enter");
    cx.run_until_parked();
    load_all(&tabs, cx);
    assert!(!sheet_open(&tabs, cx));
    assert_eq!(tab_names(&tabs, cx), ["a", "c"]);
    assert_eq!(project_names(&tabs, cx), ["Keys"]);
}

#[gpui::test]
fn a_locked_vault_explains_the_keychain_and_not_now_returns_to_the_sheet(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = folder(dir.path(), "Matter", &["a.md", "b.md"]);
    let (tabs, cx) = boot_home(cx, Session::default());
    drop_paths(&tabs, cx, vec![path]);
    // The key locks while the sheet shows, as a Mac's may at launch.
    let root = dir.path().join("vault");
    tabs.update(cx, |tabs, _| {
        tabs.stage_vault_key(Ok(mdoc_vault::Vault::open(root, &[3; 32]).unwrap()))
    });
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert!(cx.debug_bounds("keychain-explainer").is_some());
    // Not now: back to the sheet, with focus in it.
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("keychain-explainer").is_none());
    assert!(sheet_open(&tabs, cx));
    assert_eq!(focused_row(&tabs, cx), Some(0));
    cx.update(|_, cx| assert!(no_documents(tabs.read(cx))));
    // Continue opens the Vault, then the files go into a new Project.
    cx.simulate_keystrokes("enter");
    draw(cx);
    click_toolbar(cx, "keychain-continue");
    load_all(&tabs, cx);
    assert!(!sheet_open(&tabs, cx));
    assert_eq!(tab_names(&tabs, cx), ["a", "b"]);
    let project = cx.update(|_, cx| tabs.read(cx).projects.summaries()[0].id.clone());
    assert_eq!(tab_projects(&tabs, cx), vec![Some(project); 2]);
}

/// No tab holds a document: Home's placeholder at most.
fn no_documents(tabs: &Workspace) -> bool {
    tabs.tabs.iter().all(|t| {
        t.record().markdown.as_os_str().is_empty()
            && t.record().attachment.is_none()
            && t.record().document.is_none()
    })
}

#[gpui::test]
fn the_cap_holds_against_tabs_closed_behind_the_sheet(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let names: Vec<String> = (1..=21).map(|n| format!("{n:02}.md")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let path = folder(dir.path(), "Big", &refs);
    let (tabs, cx) = boot_home(cx, Session::default());
    drop_paths(&tabs, cx, vec![path.join("21.md")]);
    drop_paths(&tabs, cx, vec![path]);
    // Window commands wait: Home and Close don't act behind the sheet.
    for action in [ShowHome.boxed_clone(), Close.boxed_clone()] {
        cx.update(|window, cx| window.dispatch_action(action, cx));
        cx.run_until_parked();
        draw(cx);
        assert_eq!(focused_row(&tabs, cx), Some(0));
        cx.update(|_, cx| assert!(!tabs.read(cx).showing_home(cx)));
    }
    // All 21 are ticked; if the open one goes anyway, 20 still open.
    let open = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.remove(open, window, cx));
    click_toolbar(cx, "folder-open-files");
    assert_eq!(vault(&tabs, cx).documents().len(), 1 + CAP);
}

#[gpui::test]
fn the_sheet_keeps_the_keyboard_while_a_loose_file_loads(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = folder(dir.path(), "Matter", &["a.md", "b.md"]);
    let loose = dir.path().join("loose.md");
    std::fs::write(&loose, "Loose").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    drop_paths(&tabs, cx, vec![loose, path]);
    let loose = active(&tabs, cx);
    cx.update(|_, cx| assert!(!loose.read(cx).lifecycle.loading()));
    assert_eq!(focused_row(&tabs, cx), Some(0));
    cx.simulate_keystrokes("space");
    let ticks: Vec<bool> = sheet_rows(&tabs, cx).into_iter().map(|r| r.2).collect();
    assert_eq!(
        ticks,
        [false, true],
        "Space reaches the sheet, not the document"
    );
}

#[gpui::test]
fn a_deferred_drop_keeps_its_project_header(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let case = new_project(&tabs, cx, tab);
    let other = tempfile::tempdir().unwrap();
    let path = folder(other.path(), "Unrelated", &["note.md"]);
    tabs.update(cx, |tabs, cx| tabs.install_prompts(cx));
    // Kept alive: dropping the answer dismisses the prompt.
    let _answer =
        cx.update(|window, cx| window.prompt(gpui::PromptLevel::Info, "Busy", None, &["OK"], cx));
    cx.run_until_parked();
    cx.update(|window, _| assert!(window.has_active_prompt()));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths_into(vec![path], Some(case), window, cx)
    });
    cx.run_until_parked();
    assert!(!sheet_open(&tabs, cx));
    cx.simulate_keystrokes("escape");
    wait(cx, 200);
    cx.update(|_, cx| {
        let sheet = tabs.read(cx).folder_sheets.sheet().unwrap();
        assert_eq!(sheet.primary_label(), "Add to “contract”");
    });
}
