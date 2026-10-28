//! The folder sheet (ADR 0075): a dropped folder shows its files before
//! anything opens; Open files never reads the case key; one folder opens at
//! most 20 new tabs.
use super::*;
use crate::workspace::folder_sheet::{CAP, CAP_NOTE, Status};
use mdoc_store::cases::{Case, CaseError, Store};

fn store(dir: &std::path::Path) -> Store {
    Store::with_key(dir.join("cases"), &[3; 32])
}

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

/// Every tab's case, in sidebar order: its link once loaded, else its record.
fn tab_cases(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Option<String>> {
    cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .iter()
            .map(|t| {
                t.view()
                    .and_then(|v| v.read(cx).case.link().map(|l| l.id.clone()))
                    .or(t.record().case.clone())
            })
            .collect()
    })
}

/// Load every tab, so members join as their Markdown loads.
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
    tabs.update(cx, |tabs, _| tabs.use_case_store(store(dir.path())));
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
        assert_eq!(sheet.primary_label(), "Open as case");
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
    // Open as case: the files open and join a case named after the folder.
    click_toolbar(cx, "folder-primary");
    load_all(&tabs, cx);
    assert!(!sheet_open(&tabs, cx));
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md", "README.md"]);
    let case = cx.update(|_, cx| {
        let cases = tabs.read(cx).case_coordinator.cases();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].name, "Petrova v Northwind");
        cases[0].id.clone()
    });
    assert_eq!(tab_cases(&tabs, cx), vec![Some(case); 3]);
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
fn open_files_never_reads_the_case_key(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = folder(dir.path(), "Matter", &["a.md", "b.md"]);
    let (tabs, cx) = boot_home(cx, Session::default());
    // As at a launch where reading the key would ask: cases stay locked.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(store(dir.path())));
        tabs.load_cases(window, cx);
        assert!(tabs.case_coordinator.locked());
        tabs.cases.key_reads = 0;
    });
    drop_paths(&tabs, cx, vec![path]);
    click_toolbar(cx, "folder-open-files");
    load_all(&tabs, cx);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    assert_eq!(tab_cases(&tabs, cx), [None, None]);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.cases.key_reads, 0, "Open files never reads the key");
        assert!(
            tabs.cases.test_open.is_some(),
            "the staged key is untouched"
        );
        assert!(!tabs.cases.explainer);
        assert!(tabs.case_coordinator.cases().is_empty());
    });
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
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 1 + CAP));
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
fn a_folder_with_files_in_a_loaded_case_adds_to_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, contract) = applied_document(cx, dir.path(), Session::default());
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let case = tabs
        .update_in(cx, |tabs, window, cx| tabs.create_case(first, window, cx))
        .unwrap();
    cx.run_until_parked();
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
                Status::InCase(Some("contract".into())),
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
    assert_eq!(
        tab_names(&tabs, cx),
        ["contract.md", "letter.md", "memo.md"]
    );
    assert_eq!(tab_cases(&tabs, cx), vec![Some(case.clone()); 3]);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.case_coordinator.cases().len(), 1, "no new case");
        let members = &tabs.case_coordinator.case(&case).unwrap().members;
        assert!(members.contains(&session_store::identity(&contract)));
        assert_eq!(members.len(), 3);
    });
}

#[gpui::test]
fn a_folder_dropped_on_a_case_header_adds_to_that_case(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let case = tabs
        .update_in(cx, |tabs, window, cx| tabs.create_case(first, window, cx))
        .unwrap();
    cx.run_until_parked();
    let other = tempfile::tempdir().unwrap();
    let path = folder(other.path(), "Unrelated", &["note.md"]);
    // A real drop on the header, not on the list.
    draw(cx);
    let header = cx.debug_bounds("case-header").unwrap();
    drag_files(cx, &[path], header.center());
    drop_files(cx, header.center());
    cx.update(|_, cx| {
        let sheet = tabs.read(cx).folder_sheets.sheet().unwrap();
        assert_eq!(sheet.primary_label(), "Add to “contract”");
    });
    click_toolbar(cx, "folder-primary");
    load_all(&tabs, cx);
    assert_eq!(tab_names(&tabs, cx), ["contract.md", "note.md"]);
    assert_eq!(tab_cases(&tabs, cx), vec![Some(case); 2]);
}

#[gpui::test]
fn an_open_file_without_a_case_joins_the_new_case(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = folder(dir.path(), "Matter", &["a.md", "b.md"]);
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update(cx, |tabs, _| tabs.use_case_store(store(dir.path())));
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
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    let case = cx.update(|_, cx| tabs.read(cx).case_coordinator.cases()[0].id.clone());
    assert_eq!(tab_cases(&tabs, cx), vec![Some(case); 2]);
}

#[gpui::test]
fn an_open_file_in_another_case_stays_there(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, contract) = applied_document(cx, dir.path(), Session::default());
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let theirs = tabs
        .update_in(cx, |tabs, window, cx| tabs.create_case(first, window, cx))
        .unwrap();
    cx.run_until_parked();
    // A second loaded case holds more of the folder, so it is the target.
    std::fs::write(dir.path().join("a.md"), "A").unwrap();
    std::fs::write(dir.path().join("b.md"), "B").unwrap();
    std::fs::write(dir.path().join("c.md"), "C").unwrap();
    let mut other = Case::new("Other".into(), &[], None);
    other.members = ["a.md", "b.md", "c.md"]
        .map(|name| session_store::identity(&dir.path().join(name)))
        .to_vec();
    let other_id = other.id.clone();
    tabs.update(cx, |tabs, _| tabs.case_coordinator.cases_mut().push(other));
    drop_paths(&tabs, cx, vec![dir.path().to_path_buf()]);
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).folder_sheets.sheet().unwrap().primary_label(),
            "Add to “Other”"
        );
    });
    let rows = sheet_rows(&tabs, cx);
    assert_eq!(
        rows[3],
        (
            "contract.md".into(),
            Status::InCase(Some("contract".into())),
            true
        )
    );
    click_toolbar(cx, "folder-primary");
    load_all(&tabs, cx);
    let names = tab_names(&tabs, cx);
    assert_eq!(names.len(), 4);
    let cases = tab_cases(&tabs, cx);
    for (name, case) in names.iter().zip(&cases) {
        let want = if *name == "contract.md" {
            &theirs
        } else {
            &other_id
        };
        assert_eq!(case.as_ref(), Some(want), "{name}");
    }
    assert!(contract.exists());
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
    assert_eq!(tab_names(&tabs, cx), ["loose.md"]);
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
    assert_eq!(tab_names(&tabs, cx), ["loose.md"], "Cancel opens nothing");
    click_toolbar(cx, "folder-open-files");
    assert!(!sheet_open(&tabs, cx));
    assert_eq!(tab_names(&tabs, cx), ["loose.md", "three.md", "two.md"]);
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
    // Enter opens as a case, with the ticks as they are.
    tabs.update(cx, |tabs, _| tabs.use_case_store(store(dir.path())));
    drop_paths(&tabs, cx, vec![path]);
    cx.simulate_keystrokes("down space enter");
    cx.run_until_parked();
    load_all(&tabs, cx);
    assert!(!sheet_open(&tabs, cx));
    assert_eq!(tab_names(&tabs, cx), ["a.md", "c.md"]);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).case_coordinator.cases().len(), 1));
}

#[gpui::test]
fn a_case_action_explains_the_keychain_and_not_now_returns_to_the_sheet(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = folder(dir.path(), "Matter", &["a.md", "b.md"]);
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.stage_case_key(Ok(store(dir.path())));
        tabs.cases.key_reads = 0;
    });
    drop_paths(&tabs, cx, vec![path]);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).cases.key_reads, 0, "showing reads nothing"));
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert!(cx.debug_bounds("keychain-explainer").is_some());
    cx.update(|_, cx| assert!(tabs.read(cx).cases.test_open.is_some(), "not read yet"));
    // Not now: back to the sheet, with focus in it.
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("keychain-explainer").is_none());
    assert!(sheet_open(&tabs, cx));
    assert_eq!(focused_row(&tabs, cx), Some(0));
    cx.update(|_, cx| assert!(no_documents(tabs.read(cx))));
    // Continue reads the key, then the files open as a case.
    cx.simulate_keystrokes("enter");
    draw(cx);
    click_toolbar(cx, "keychain-continue");
    load_all(&tabs, cx);
    assert!(!sheet_open(&tabs, cx));
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    let case = cx.update(|_, cx| tabs.read(cx).case_coordinator.cases()[0].id.clone());
    assert_eq!(tab_cases(&tabs, cx), vec![Some(case); 2]);
}

#[gpui::test]
fn without_the_key_the_files_open_without_a_case(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = folder(dir.path(), "Matter", &["a.md"]);
    let (tabs, cx) = boot_home(cx, Session::default());
    // After the explainer: the key store refuses.
    tabs.update(cx, |tabs, _| {
        tabs.stage_case_key(Err(CaseError::NoKeyStore))
    });
    drop_paths(&tabs, cx, vec![path.clone()]);
    cx.simulate_keystrokes("enter");
    draw(cx);
    click_toolbar(cx, "keychain-continue");
    assert!(!sheet_open(&tabs, cx));
    assert_eq!(tab_names(&tabs, cx), ["a.md"]);
    assert_eq!(tab_cases(&tabs, cx), [None]);
    cx.update(|_, cx| {
        let notice = tabs.read(cx).notices.message().unwrap().to_string();
        assert!(
            notice.starts_with("“Matter” opened without a case"),
            "{notice}"
        );
    });
    // Without an explainer (no key store at all), the same.
    let other = folder(dir.path(), "Other", &["b.md"]);
    tabs.update(cx, |tabs, _| {
        tabs.cases.test_may_ask = false;
        tabs.cases.test_open = Some(Err(CaseError::NoKeyStore));
    });
    drop_paths(&tabs, cx, vec![other]);
    click_toolbar(cx, "folder-primary");
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    assert_eq!(tab_cases(&tabs, cx), [None, None]);
}

/// No tab holds a file: Home's placeholder at most.
fn no_documents(tabs: &Workspace) -> bool {
    tabs.tabs
        .iter()
        .all(|t| t.record().markdown.as_os_str().is_empty() && t.record().attachment.is_none())
}
