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
        checking: false,
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
    let text = "# PERSON_1 and ORG_1\n\n> PERSON_1 signed it today.\n\nThird.\nFourth.";
    let row = ShelfDocument::of_tab(tab(1, "Anna Petrova.md", text, 0)).unwrap();
    assert_eq!(row.readiness, Readiness::Ready);
    assert_eq!(row.title, "PERSON_1.md");
    assert_eq!(row.display_title(), "PERSON_1");
    assert_eq!(
        row.text_for_ai(),
        Some(mdoc_pii::handoff::for_ai(text, ["PERSON_1", "ORG_1"]).as_str())
    );
    // The heading and the one-word lines are not prose.
    let preview = row.preview.as_ref().unwrap();
    assert_eq!(preview.text, "PERSON_1 signed it today.");
    assert_eq!(preview.aliases, vec![0..8; 1]);
    assert_eq!(
        row.replaced,
        [(Category::Person, 1), (Category::Organization, 1)]
    );
    assert_eq!(row.replaced_count(), 2);
    assert_eq!(row.words, 9, "Markdown markers are not words");
    assert_eq!(row.file_name(), "PERSON_1.md");
    assert_eq!(
        row.drag_card(),
        Some((
            "PERSON_1".to_string(),
            "PERSON_1 signed it today.".to_string()
        ))
    );
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
    assert!(row.preview.is_none(), "no text of a document not ready");
    assert_eq!(row.drag_card(), None);
}

#[test]
fn a_kept_original_or_a_scan_in_progress_keeps_an_open_tab_from_leaving() {
    // Anna applied once and kept once: nothing pending, an original left.
    let kept = ShelfDocument::of_tab(tab(1, "a.md", "PERSON_1 thanks Anna Petrova.", 0)).unwrap();
    assert_eq!(kept.readiness, Readiness::NamesNotReplaced(1));
    assert_eq!(kept.text_for_ai(), None);
    assert!(kept.preview.is_none());
    // Just edited: pending mentions are being found again.
    let mut checking = tab(2, "b.md", "PERSON_1 met ORG_1.", 0);
    checking.checking = true;
    let row = ShelfDocument::of_tab(checking).unwrap();
    assert_eq!(row.readiness, Readiness::Checking);
    assert_eq!(row.text_for_ai(), None);
    // A kept name stays masked in the title of a row that is not ready.
    let mut titled = tab(3, "Boris Ivanov.md", "PERSON_1 met ORG_1.", 1);
    titled
        .names
        .push(identity("PERSON_2", Category::Person, &["Boris Ivanov"]));
    assert_eq!(ShelfDocument::of_tab(titled).unwrap().title, "PERSON_2.md");
}

#[test]
fn a_closed_member_names_the_aliases_its_editor_would() {
    let path = Path::new("/docs/a.md");
    // Only the exact alias is relinked as applied when it reopens.
    let text = "person 1 met PERSON_1 and ORG_1.";
    let case = case_with(path, Some(text));
    let row = ShelfDocument::of_member(ClosedMember {
        case: &case,
        path,
        text: Some(text),
        edited: at(1),
    });
    assert_eq!(
        row.text_for_ai(),
        Some(mdoc_pii::handoff::for_ai(text, ["PERSON_1", "ORG_1"]).as_str())
    );
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
    assert!(row.preview.is_none());
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

fn preview_of(text: &str) -> Option<Preview> {
    preview_lines(text, &RestoreMap::new(names()))
}

#[test]
fn a_preview_skips_page_furniture_and_starts_at_the_first_prose() {
    let text = "—\n\n23456\n\nСтраница **1** из **5**\n\nPage 1 of 5\n\n---\n\n\
                | a | b |\n|---|---|\n\n```\ncode that is long enough to be prose\n```\n\n\
                ## A heading that is long enough\n\n\
                1. PERSON_1 agrees to pay ORG_1 the fee.\n";
    let preview = preview_of(text).unwrap();
    assert_eq!(preview.text, "PERSON_1 agrees to pay ORG_1 the fee.");
    assert_eq!(preview.aliases, [0..8, 23..28]);
    assert!(
        preview_of("23456\n\nPage 2 of 5\n").is_none(),
        "no prose, no preview"
    );
}

/// Review of #280: a fence closes only on its own kind and length.
#[test]
fn a_preview_skips_code_in_fences_of_any_kind_and_length() {
    let text = "~~~~\n```\nexample code that is long enough to read as prose\n```\n~~~~\n\n\
                ````md\n```\nnested example that is long enough as prose\n````\n\n\
                PERSON_1 signed the agreement today.\n";
    assert_eq!(
        preview_of(text).unwrap().text,
        "PERSON_1 signed the agreement today."
    );
    // Review of #280: quoted headings and fences are what they are.
    let quoted = "> # A heading long enough to read as prose\n>\n> ```\n\
                  > quoted code that is long enough to read as prose\n> ```\n\n\
                  > PERSON_1 signed the agreement today.\n";
    assert_eq!(
        preview_of(quoted).unwrap().text,
        "PERSON_1 signed the agreement today."
    );
    // Review of #280: nested containers show no markers.
    for nested in [
        "> - PERSON_1 signed the agreement today.",
        "- > PERSON_1 signed the agreement today.",
        "> [!NOTE]\n> PERSON_1 signed the agreement today.",
        "- [x] PERSON_1 signed the agreement today.",
    ] {
        assert_eq!(
            preview_of(nested).unwrap().text,
            "PERSON_1 signed the agreement today.",
            "{nested}"
        );
    }
    // Review of #280: a fence inside a list item, too.
    let listed = "- ```\n  example code long enough to read as prose\n  ```\n\n\
                  PERSON_1 signed the agreement today.\n";
    assert_eq!(
        preview_of(listed).unwrap().text,
        "PERSON_1 signed the agreement today."
    );
    // Review of #280: a quoted fence ends with its quote, and a fence
    // outside the quote opens a block of its own.
    let quote_ends = "> ```\n> quoted code that is long enough as prose\n```\n\
                      This is code, not the document prose.\n```\n\n\
                      PERSON_1 signed the agreement today.\n";
    assert_eq!(
        preview_of(quote_ends).unwrap().text,
        "PERSON_1 signed the agreement today."
    );
    // Review of #280: a fence in a list item ends with the item, at a
    // sibling item or at prose after the list.
    for ended in [
        "- ~~~\n  code example that is long enough as prose\n- PERSON_1 signed the agreement today.\n",
        "- ~~~\n  code example that is long enough as prose\nPERSON_1 signed the agreement today.\n",
        "> - ~~~\n>   code example that is long enough as prose\n> - PERSON_1 signed the agreement today.\n",
        "- > ~~~\n  > code example that is long enough as prose\n- > PERSON_1 signed the agreement today.\n",
    ] {
        assert_eq!(
            preview_of(ended).unwrap().text,
            "PERSON_1 signed the agreement today.",
            "{ended:?}"
        );
    }
    // Code indented in the item stays code, list markers included.
    let indented = "- ```\n  - looks like an item but it is code here\n  ```\n\n\
                    PERSON_1 signed the agreement today.\n";
    assert_eq!(
        preview_of(indented).unwrap().text,
        "PERSON_1 signed the agreement today."
    );
    // Review of #280: a quoted fence inside a list item closes in its quote.
    let quoted_in_list = "- > ```\n  > example code long enough to read as prose\n  > ```\n\n\
                          PERSON_1 signed the agreement today.\n";
    assert_eq!(
        preview_of(quoted_in_list).unwrap().text,
        "PERSON_1 signed the agreement today."
    );
    // An unclosed fence hides the rest.
    assert!(preview_of("```\nPERSON_1 signed the agreement today.\n").is_none());
}

#[test]
fn a_preview_hides_markdown_syntax_and_keeps_bold_and_italic() {
    let preview =
        preview_of("> **Important:** PERSON_1 must _sign_ the [agreement](https://x.test) today.")
            .unwrap();
    assert_eq!(
        preview.text,
        "Important: PERSON_1 must sign the agreement today."
    );
    assert!(!preview.text.contains("**") && !preview.text.contains("]("));
    assert_eq!(&preview.text[preview.bold[0].clone()], "Important:");
    assert_eq!(&preview.text[preview.italic[0].clone()], "sign");
    assert_eq!(&preview.text[preview.aliases[0].clone()], "PERSON_1");
}

#[test]
fn a_preview_joins_prose_into_one_paragraph_cut_at_a_word_with_an_ellipsis() {
    let sentence = "PERSON_1 and ORG_1 agree on every term below.";
    let text = [sentence; 12].join("\n\n");
    let preview = preview_of(&text).unwrap();
    assert!(preview.text.ends_with('…'));
    let body = preview.text.trim_end_matches('…');
    assert!(body.chars().count() <= PREVIEW_CHARS);
    assert!(
        [sentence; 12].join(" ").starts_with(body),
        "lines join with one space: {body}"
    );
    assert!(!body.ends_with(' ') && !body.ends_with("PERSON_"));
    // Short text ends where it ends.
    assert_eq!(preview_of(sentence).unwrap().text, sentence);
    // More prose after a full preview still shows the ellipsis.
    let long = format!("{}\n\nMore prose follows here.", "word ".repeat(60));
    assert!(preview_of(&long).unwrap().text.ends_with('…'));
}

#[test]
fn a_preview_never_shows_an_original_even_one_the_syntax_split() {
    for text in [
        "Anna Petrova signed the agreement with Northwind Ltd today.",
        "Anna **Petrova** signed the agreement with Northwind Ltd today.",
        "Signed by *Anna* Petrova for Northwind Ltd on the day.",
        // Review of #280: formatting inside a link's label.
        "PERSON_1 met [Anna **Petrova**](https://example.test) about the agreement today.",
        // Review of #280: an entity inside a link's label.
        "PERSON_1 met [Anna&#32;Petrova](https://example.test) about the agreement today.",
        // Review of #280: whitespace the preview collapses.
        "ORG_1 met Anna \u{a0}**Petrova** about the agreement today.",
    ] {
        let preview = preview_of(text).unwrap();
        let map = RestoreMap::new(names());
        assert_eq!(map.known_names(&preview.text), 0, "{}", preview.text);
        assert!(!preview.text.contains("**"), "{}", preview.text);
        for part in ["Anna", "Petrova", "&#"] {
            assert!(!preview.text.contains(part), "{}", preview.text);
        }
        assert!(preview.text.contains("PERSON_1"), "{}", preview.text);
    }
}

#[test]
fn replaced_counts_distinct_names_by_category_once_each() {
    let mut open = tab(1, "a.md", "PERSON_1 met ORG_1, then PERSON_1 left.", 0);
    // The case lists the same person again.
    open.names
        .push(identity("PERSON_1", Category::Person, &["Anna Petrova"]));
    let row = ShelfDocument::of_tab(open).unwrap();
    assert_eq!(
        row.replaced,
        [(Category::Person, 1), (Category::Organization, 1)]
    );
}

#[test]
fn the_display_title_drops_only_a_markdown_extension() {
    let mut row = ShelfDocument::of_tab(tab(1, "a.md", "PERSON_1", 0)).unwrap();
    for (title, shown) in [
        ("PERSON_1 notice.md", "PERSON_1 notice"),
        ("Notes.MARKDOWN", "Notes"),
        ("report.txt", "report.txt"),
        (".md", ".md"),
    ] {
        row.title = title.into();
        assert_eq!(row.display_title(), shown);
    }
}

/// Review of #280: an original stored with a double space is masked once
/// the preview has collapsed it.
#[test]
fn a_preview_masks_an_original_stored_with_odd_spacing() {
    let names = RestoreMap::new(vec![identity(
        "PERSON_1",
        Category::Person,
        &["Anna  Petrova", "Boris\u{a0}Ivanov"],
    )]);
    for text in [
        "ORG_1 met Anna  **Petrova** about the agreement today.",
        "ORG_1 met Boris\u{a0}Ivanov about the agreement today.",
    ] {
        let preview = preview_lines(text, &names).unwrap();
        for part in ["Anna", "Petrova", "Boris", "Ivanov"] {
            assert!(!preview.text.contains(part), "{}", preview.text);
        }
        assert!(preview.text.contains("PERSON_1"), "{}", preview.text);
    }
}

/// Review of #280: a name written over two lines of one paragraph, or with
/// an entity, is masked whole: the paragraph is masked before short lines
/// could be judged on their own.
#[test]
fn a_preview_masks_a_name_split_over_the_lines_of_a_paragraph() {
    for text in [
        "PERSON_1 signed the agreement for Ann&#97;\nPetrova today.",
        "PERSON_1 signed for Anna\nPetrova",
        "> PERSON_1 signed the agreement for Anna\n> Petrova today.",
    ] {
        let preview = preview_of(text).unwrap();
        for part in ["Anna", "Petrova"] {
            assert!(!preview.text.contains(part), "{text:?}: {}", preview.text);
        }
    }
    // Emphasis running over two lines (review of #280).
    let preview = preview_of("PERSON_1 met Anna **Petrova\nat the office** yesterday.").unwrap();
    assert!(
        !preview.text.contains("Anna") && !preview.text.contains("**"),
        "{}",
        preview.text
    );
    // A hard line break inside the name (review of #280).
    for text in [
        "ORG_1 met Anna\\\nPetrova about the agreement today.",
        "ORG_1 met Anna<br>\nPetrova about the agreement today.",
    ] {
        let preview = preview_of(text).unwrap();
        assert_eq!(
            preview.text, "ORG_1 met PERSON_1 about the agreement today.",
            "{text:?}"
        );
    }
    // Furniture right above prose, without a blank line (review of #280).
    assert_eq!(
        preview_of("23456\nPage 1 of 5\nORG_1 signed the **agreement** today.")
            .unwrap()
            .text,
        "ORG_1 signed the agreement today."
    );
    // Page furniture in its own paragraph is still skipped.
    assert_eq!(
        preview_of("Page 1 of 5\n\nPERSON_1 signed the agreement today.")
            .unwrap()
            .text,
        "PERSON_1 signed the agreement today."
    );
}

/// Review of #280: quote or list markers written inside code are its text,
/// not a closing fence.
#[test]
fn markers_inside_code_do_not_close_its_fence() {
    for text in [
        "```\n> ```\nliteral code that is long enough to read as prose\n```\n\nPERSON_1 signed the agreement today.",
        "```\n- ```\nliteral code that is long enough to read as prose\n```\n\nPERSON_1 signed the agreement today.",
        "> ```\n> code inside a quote that is long enough as prose\n> ```\n\nPERSON_1 signed the agreement today.",
    ] {
        assert_eq!(
            preview_of(text).unwrap().text,
            "PERSON_1 signed the agreement today.",
            "{text:?}"
        );
    }
}

/// Review of #280: one long paragraph of wrapped lines, with furniture
/// among them, is cleaned in one pass rather than a cut at a time.
#[test]
fn a_long_wrapped_paragraph_previews_in_linear_time() {
    let mut text = String::new();
    for line in 0..100_000 {
        if line % 50 == 49 {
            text.push_str("12\n");
        } else {
            text.push_str("PERSON_1 signed **the** agreement with ORG_1 today\n");
        }
    }
    let started = std::time::Instant::now();
    let preview = preview_of(&text).unwrap();
    assert!(
        preview
            .text
            .starts_with("PERSON_1 signed the agreement with ORG_1 today PERSON_1"),
        "{}",
        preview.text
    );
    assert!(preview.text.ends_with('…'));
    assert!(!preview.text.contains(LINE_MARK) && !preview.text.contains("12"));
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "took {:?}",
        started.elapsed()
    );
}

/// A paragraph cut short may end in half a name: it shows only when its
/// text runs well past the preview, so that end is never in sight.
#[test]
fn a_paragraph_cut_short_never_shows_its_end() {
    let link = format!("[x](https://example.com/{})", "a".repeat(300));
    let mut text = String::from("PERSON_1 signed the agreement today.\n\n");
    while text.len() < 2 * PARAGRAPH_BYTES {
        text.push_str(&link);
        text.push('\n');
    }
    // Mostly hidden link targets: too little text shows, so only the
    // paragraph before it does, and the preview says there is more.
    let preview = preview_of(&text).unwrap();
    assert_eq!(preview.text, "PERSON_1 signed the agreement today.…");
}

/// Review of #280: the limit holds within one line too, cut at a character
/// boundary, so a line of many thousand spans never reaches the scanner whole.
#[test]
fn a_single_huge_line_is_read_only_up_to_the_limit() {
    let mut text = String::from("PERSON_1 agrees ");
    for _ in 0..65_536 {
        text.push_str("**слово** ");
    }
    let preview = preview_of(&text).unwrap();
    assert!(
        preview.text.starts_with("PERSON_1 agrees слово"),
        "{}",
        preview.text
    );
    assert!(preview.text.ends_with('…'));
}

/// Review of #280: an initial on its own line is half a name, not page
/// furniture, so the name is still masked whole.
#[test]
fn an_initial_on_its_own_line_keeps_the_name_masked() {
    let names = RestoreMap::new(vec![identity("PERSON_2", Category::Person, &["J. Smith"])]);
    let preview = preview_lines(
        "J.\nSmith signed the agreement with the employer today.\n",
        &names,
    )
    .unwrap();
    assert!(!preview.text.contains("Smith"), "{}", preview.text);
    assert!(
        preview.text.starts_with("PERSON_2 signed"),
        "{}",
        preview.text
    );
}

/// Review of #280: a `<br>` inside a line reads as a space, so a name it
/// breaks is masked whole; any other split that still spells a name out
/// gives no preview at all.
#[test]
fn a_break_inside_a_line_never_shows_an_original() {
    for text in [
        "PERSON_1 met Anna<br>Petrova about the agreement today.",
        "PERSON_1 met Anna<BR />Petrova about the agreement today.",
    ] {
        let preview = preview_of(text).unwrap();
        assert_eq!(
            preview.text, "PERSON_1 met PERSON_1 about the agreement today.",
            "{text}"
        );
    }
    assert!(
        preview_of("PERSON_1 met Anna<wbr>Petrova about the agreement today.").is_none(),
        "a split the masking misses spells the name: no preview"
    );
}
