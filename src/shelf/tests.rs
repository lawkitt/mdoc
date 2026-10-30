use super::*;
use mdoc_pii::Category;
use std::time::Duration;

fn identity(alias: &str, category: Category, originals: &[&str]) -> ExportedIdentity {
    ExportedIdentity {
        alias: alias.into(),
        previous: Vec::new(),
        category,
        custom: false,
        originals: originals.iter().map(|o| (*o).into()).collect(),
    }
}

fn names() -> Vec<ExportedIdentity> {
    vec![
        identity("PERSON_1", Category::Person, &["Anna Petrova"]),
        identity("ORG_1", Category::Organization, &["Northwind Ltd"]),
    ]
}

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn tab<'a>(id: u64, title: &'a str, text: &'a str, pending: usize) -> OpenTab<'a> {
    OpenTab {
        id,
        title,
        text,
        aliases: vec!["PERSON_1", "ORG_1"],
        pending,
        names: names(),
        case: None,
        edited: at(id),
    }
}

fn case_with(member: &Path, text: Option<&str>) -> Case {
    let mut case = Case::new("Petrova".into(), &names(), Some(member.to_path_buf()));
    if let Some(text) = text {
        case.set_reviewed(member, Some(fingerprint(text)));
    }
    case
}

#[test]
fn a_ready_tab_copies_the_editor_text_and_previews_aliases() {
    let text = "# PERSON_1 and ORG_1\n\n> PERSON_1 signed.\n\nThird.\nFourth.";
    let row = ShelfDocument::of_tab(tab(1, "Anna Petrova.md", text, 0)).unwrap();
    assert_eq!(row.readiness, Readiness::Ready);
    assert_eq!(row.title, "PERSON_1.md");
    assert_eq!(
        row.text_for_ai(),
        Some(mdoc_pii::handoff::for_ai(text, ["PERSON_1", "ORG_1"]).as_str())
    );
    let lines: Vec<_> = row.preview.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(lines, ["PERSON_1 and ORG_1", "PERSON_1 signed.", "Third."]);
    assert_eq!(row.preview[0].aliases, [0..8, 13..18]);
    assert_eq!(row.words, 7, "Markdown markers are not words");
    assert_eq!(row.file_name(), "PERSON_1.md");
}

#[test]
fn a_tab_without_aliases_is_not_listed_and_one_with_proposals_is_not_ready() {
    let mut none = tab(1, "a.md", "Anna Petrova", 0);
    none.aliases.clear();
    assert!(ShelfDocument::of_tab(none).is_none());
    // Pending names mask the title even before they are applied.
    let mut pending = tab(2, "Boris Ivanov.md", "PERSON_1 met Boris Ivanov.", 2);
    pending
        .names
        .push(identity("PERSON_2", Category::Person, &["Boris Ivanov"]));
    let row = ShelfDocument::of_tab(pending).unwrap();
    assert_eq!(row.readiness, Readiness::LeftToReview(2));
    assert_eq!(row.readiness.label(), "2 left to review");
    assert_eq!(row.title, "PERSON_2.md");
    assert_eq!(row.text_for_ai(), None);
    assert!(row.preview.is_empty(), "no text of a document not ready");
}

#[test]
fn a_closed_member_is_ready_only_while_it_matches_its_reviewed_text() {
    let path = Path::new("/docs/Anna Petrova.md");
    let text = "PERSON_1 met ORG_1.";
    let member = |case: &Case, text: Option<&str>| {
        ShelfDocument::of_member(ClosedMember {
            case,
            path,
            text,
            edited: at(5),
        })
    };
    let reviewed = case_with(path, Some(text));
    let row = member(&reviewed, Some(text));
    assert_eq!(row.readiness, Readiness::Ready);
    assert_eq!(row.title, "PERSON_1.md");
    assert_eq!(
        row.key,
        ShelfKey::Member {
            case: reviewed.id.clone(),
            path: path.into()
        }
    );
    assert_eq!(row.case.as_ref().unwrap().id, reviewed.id);
    assert_eq!(
        row.text_for_ai(),
        Some(mdoc_pii::handoff::for_ai(text, ["PERSON_1", "ORG_1"]).as_str())
    );

    let edited = member(&reviewed, Some("PERSON_1 met ORG_1 again."));
    assert_eq!(edited.readiness, Readiness::ChangedSinceReview);
    assert_eq!(edited.readiness.label(), "Changed since review");
    assert_eq!(edited.text_for_ai(), None);

    let never = member(&case_with(path, None), Some(text));
    assert_eq!(never.readiness, Readiness::ChangedSinceReview);

    let unreadable = member(&reviewed, None);
    assert_eq!(unreadable.readiness, Readiness::ChangedSinceReview);

    // A known original wins even over a matching fingerprint.
    let leaked = "Anna Petrova met Northwind Ltd and Anna Petrova.";
    let row = member(&case_with(path, Some(leaked)), Some(leaked));
    assert_eq!(row.readiness, Readiness::NamesNotReplaced(2));
    assert_eq!(row.readiness.label(), "2 names not replaced");
    assert_eq!(row.text_for_ai(), None);
    assert!(row.preview.is_empty());
}

#[test]
fn the_list_puts_open_tabs_first_and_stops_at_eight() {
    let tabs: Vec<_> = (1..=5)
        .map(|id| ShelfDocument::of_tab(tab(id, "t.md", "PERSON_1", 0)).unwrap())
        .collect();
    let case = case_with(Path::new("/m.md"), None);
    let members: Vec<_> = (0..6)
        .map(|n| {
            ShelfDocument::of_member(ClosedMember {
                case: &case,
                path: Path::new("/m.md"),
                text: Some("PERSON_1"),
                // Members are newer than every tab; tabs still come first.
                edited: at(100 + n),
            })
        })
        .collect();
    let list = list(tabs, members);
    assert_eq!(list.len(), MAX_DOCUMENTS);
    let keys: Vec<_> = list[..5].iter().map(|r| r.key.clone()).collect();
    assert_eq!(keys, (1..=5).rev().map(ShelfKey::Tab).collect::<Vec<_>>());
    let edited: Vec<_> = list[5..].iter().map(|r| r.edited).collect();
    assert_eq!(edited, [at(105), at(104), at(103)]);
}

#[test]
fn closed_members_skip_open_and_missing_files_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.md");
    let new = dir.path().join("new.md");
    let open = dir.path().join("open.md");
    for path in [&old, &new, &open] {
        std::fs::write(path, "PERSON_1").unwrap();
    }
    let set = |path: &Path, seconds| {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(at(seconds))
            .unwrap()
    };
    set(&old, 1_000);
    set(&new, 2_000);
    let mut case = Case::new("c".into(), &names(), None);
    case.members = vec![
        old.clone(),
        dir.path().join("missing.md"),
        new.clone(),
        open.clone(),
    ];
    let cases = [case];
    let found = closed_members(&cases, std::slice::from_ref(&open), 8);
    let paths: Vec<_> = found.iter().map(|(_, p, _)| p.to_path_buf()).collect();
    assert_eq!(paths, [new.clone(), old]);
    assert_eq!(
        closed_members(&cases, &[], 1).len(),
        1,
        "only as many as there is room"
    );
    assert_eq!(read_member(&new).as_deref(), Some("PERSON_1"));
    assert_eq!(read_member(&dir.path().join("missing.md")), None);
}

#[test]
fn file_names_are_safe_and_end_in_md() {
    let named = |title: &str| {
        let mut row = ShelfDocument::of_tab(tab(1, title, "PERSON_1", 0)).unwrap();
        row.title = title.into();
        row.file_name()
    };
    assert_eq!(named("PERSON_1 settlement.md"), "PERSON_1 settlement.md");
    assert_eq!(named("a/b: c?.docx"), "a-b- c-.docx.md");
    assert_eq!(named("  .hidden"), "hidden.md");
    assert_eq!(named(""), "Document.md");
    assert_eq!(named("Notes.MARKDOWN"), "Notes.MARKDOWN");
}

#[test]
fn exports_are_private_fresh_files_cleared_at_quit() {
    let dir = tempfile::tempdir().unwrap();
    let ready = ShelfDocument::of_tab(tab(1, "Anna Petrova.md", "PERSON_1 met ORG_1.", 0)).unwrap();
    let pending = ShelfDocument::of_tab(tab(2, "b.md", "PERSON_1 met ORG_1.", 1)).unwrap();
    let exports = ShelfExports::create_in(dir.path()).unwrap();
    let first = exports.export(&ready).unwrap().unwrap();
    let second = exports.export(&ready).unwrap().unwrap();
    assert_ne!(first, second, "every drag writes a fresh export");
    assert_eq!(first.file_name().unwrap(), "PERSON_1.md");
    assert!(first.starts_with(exports.root()));
    assert_eq!(
        std::fs::read_to_string(&first).unwrap(),
        ready.text_for_ai().unwrap()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(exports.root()), 0o700);
        assert_eq!(mode(&first), 0o600);
    }
    assert_eq!(exports.export(&pending).unwrap(), None);
    exports.clear();
    assert!(!first.exists() && !second.exists());
    assert!(
        exports.root().exists(),
        "the folder stays until the process ends"
    );
    let root = exports.root().to_path_buf();
    drop(exports);
    assert!(!root.exists());
}

#[test]
fn a_launch_removes_exports_a_crashed_process_left() {
    let dir = tempfile::tempdir().unwrap();
    let live = ShelfExports::create_in(dir.path()).unwrap();
    // A folder whose owner is gone: its lock file is not held.
    let stale = dir.path().join("mdoc-shelf-1-1");
    std::fs::create_dir(&stale).unwrap();
    std::fs::write(stale.join(".lock"), "").unwrap();
    std::fs::write(stale.join("PERSON_1.md"), "left behind").unwrap();
    let other = dir.path().join("unrelated");
    std::fs::create_dir(&other).unwrap();
    let next = ShelfExports::create_in(dir.path()).unwrap();
    assert!(!stale.exists());
    assert!(live.root().exists(), "a running process keeps its folder");
    assert!(other.exists());
    drop(next);
    drop(live);
}

#[test]
fn restore_clipboard_uses_one_case_and_counts_what_it_did() {
    let restored = restore_clipboard("PERSON_1 and person 1 thank ORG_1 and PERSON_9.", names());
    assert_eq!(
        restored.text,
        "Anna Petrova and Anna Petrova thank Northwind Ltd and PERSON_9."
    );
    assert_eq!(restored.summary(), "2 names restored · 1 unrecognised");
    let one = restore_clipboard("PERSON_1", names());
    assert_eq!(one.summary(), "1 name restored · 0 unrecognised");
}
