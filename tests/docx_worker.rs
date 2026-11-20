// Exercise the shipping worker rather than the headless UI test adapter.
use mdoc_import::{
    docx_preview::{PreviewError, read_source, render_with_executable},
    import::Source,
};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

#[test]
fn real_worker_renders_searchable_pdf_and_reports_failure() {
    let executable = Path::new(env!("CARGO_BIN_EXE_mdoc"));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import/text.docx");
    let original = std::fs::read(&source).unwrap();
    let preview = render_with_executable(
        &held(&source),
        Arc::new(AtomicBool::new(false)),
        executable,
        Duration::from_secs(30),
        Default::default(),
        None,
    )
    .unwrap();
    let pdf = gpui_pdf::parse(Arc::new(std::fs::read(&preview.pdf_path).unwrap())).unwrap();
    assert!(!pdf.pages().is_empty());
    let text = gpui_pdf::extract_page_text(&pdf, 0).unwrap().text();
    assert!(!text.trim().is_empty());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let artifact = preview.pdf_path.clone();
    drop(preview);
    let cleanup_started = std::time::Instant::now();
    while artifact.exists() {
        assert!(cleanup_started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }

    let result = read_source(Path::new("missing.docx"));
    assert!(matches!(result, Err(PreviewError::Read(_))));
    // The worker reports what kind of failure it was, so a damaged package
    // is not shown as a failed worker with Retry.
    let result = render_with_executable(
        &held(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/import/truncated--errors.docx"),
        ),
        Arc::new(AtomicBool::new(false)),
        executable,
        Duration::from_secs(30),
        Default::default(),
        None,
    );
    let error = result.err().unwrap();
    assert!(matches!(error, PreviewError::Package(_)), "{error}");
    assert!(!error.retryable());
    assert!(
        !std::process::Command::new(executable)
            .arg("--mdoc-docx-worker")
            .status()
            .unwrap()
            .success()
    );
}

#[cfg(unix)]
#[test]
fn timeout_and_cancellation_kill_and_reap_worker() {
    use std::{os::unix::fs::PermissionsExt, sync::atomic::Ordering, time::Instant};
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("worker");
    let pid = dir.path().join("pid");
    // The worker writes its pid before anything else, then hangs.
    std::fs::write(
        &executable,
        format!("#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n", pid.display()),
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    // Enough time for the shell to start on a loaded machine running test
    // binaries in parallel (nextest), so the kill usually hits a live worker.
    let result = render_with_executable(
        &unused(),
        Arc::new(AtomicBool::new(false)),
        &executable,
        Duration::from_secs(5),
        Default::default(),
        None,
    );
    assert!(matches!(result, Err(PreviewError::Timeout)));
    // Whenever the worker got as far as its pid, it must be dead and reaped
    // (a zombie still answers `kill -0`). No pid, or an empty file, means it
    // was killed before it started.
    let assert_dead = || {
        let worker_pid = std::fs::read_to_string(&pid).unwrap_or_default();
        let worker_pid = worker_pid.trim();
        assert!(
            worker_pid.is_empty()
                || !std::process::Command::new("kill")
                    .args(["-0", worker_pid])
                    .stderr(std::process::Stdio::null())
                    .status()
                    .unwrap()
                    .success()
        );
    };
    assert_dead();
    let _ = std::fs::remove_file(&pid);
    let cancelled = Arc::new(AtomicBool::new(false));
    std::thread::scope(|scope| {
        let cancel = cancelled.clone();
        let pid = &pid;
        scope.spawn(move || {
            let start = Instant::now();
            // Within the worker's 10 s timeout, which would end the run first.
            while !pid.exists() {
                assert!(start.elapsed() < Duration::from_secs(9));
                std::thread::sleep(Duration::from_millis(5));
            }
            cancel.store(true, Ordering::Relaxed);
        });
        let result = render_with_executable(
            &unused(),
            cancelled,
            &executable,
            Duration::from_secs(10),
            Default::default(),
            None,
        );
        assert!(matches!(result, Err(PreviewError::Cancelled)));
    });
    assert_dead();
}

#[cfg(unix)]
#[test]
fn crash_and_false_success_are_failures() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("worker");
    for status in [0, 9] {
        std::fs::write(&executable, format!("#!/bin/sh\nexit {status}\n")).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(matches!(
            render_with_executable(
                &unused(),
                Arc::new(AtomicBool::new(false)),
                &executable,
                Duration::from_secs(5),
                Default::default(),
                None,
            ),
            Err(PreviewError::Render(_))
        ));
    }
}

#[test]
fn oversize_input_is_rejected_in_the_worker() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("oversized.docx");
    std::fs::File::create(&source)
        .unwrap()
        .set_len(50 * 1024 * 1024 + 1)
        .unwrap();
    let result = render_with_executable(
        &held(&source),
        Arc::new(AtomicBool::new(false)),
        Path::new(env!("CARGO_BIN_EXE_mdoc")),
        Duration::from_secs(30),
        Default::default(),
        None,
    );
    assert!(matches!(result, Err(PreviewError::InputTooLarge)));
}

#[test]
fn coverage_corpus_preserves_text_links_and_landscape() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-preview/coverage.docx");
    let started = std::time::Instant::now();
    let preview = render_with_executable(
        &held(&source),
        Arc::new(AtomicBool::new(false)),
        Path::new(env!("CARGO_BIN_EXE_mdoc")),
        Duration::from_secs(30),
        Default::default(),
        None,
    )
    .unwrap();
    let conversion = started.elapsed();
    let pdf = gpui_pdf::parse(Arc::new(std::fs::read(&preview.pdf_path).unwrap())).unwrap();
    let first_page = gpui_pdf::render_page(&pdf, 0, 1.0, gpui_pdf::PageTone::Original).unwrap();
    assert!(
        first_page
            .as_bytes(0)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[2] > 200 && p[0] < 50 && p[1] < 50),
        "embedded red image must be visible"
    );
    eprintln!(
        "coverage worker={conversion:?}, through first raster={:?}, pages={}",
        started.elapsed(),
        pdf.pages().len()
    );
    let text = (0..pdf.pages().len())
        .filter_map(|i| gpui_pdf::extract_page_text(&pdf, i))
        .map(|p| p.text())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "Qualification heading",
        "Привет мир",
        "Проверка поиска",
        "Merged horizontal",
        "Merged vertical",
        "Cell alpha",
        "Cell beta",
        "List level 0",
        "List level 1",
        "Example hyperlink",
        "Qualification header",
        "Qualification footer",
        "Footnote content",
        "Page marker 9",
        "Landscape final section",
    ] {
        assert!(
            text.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .contains(
                    &expected
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>()
                ),
            "missing {expected:?} in {text}"
        );
    }
    assert!(gpui_pdf::page_dims(&pdf).iter().any(|(w, h)| w > h));
    assert!(gpui_pdf::page_links(&pdf).iter().flatten().any(|link| matches!(&link.target, gpui_pdf::LinkTarget::Uri(uri) if uri == "https://example.com")));
    assert!(preview.warnings.is_empty(), "{:?}", preview.warnings);
}

#[test]
fn legal_opinion_justified_heading_preserves_word_spaces() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/docx-preview/legal-opinion.docx");
    let preview = render_with_executable(
        &held(&source),
        Arc::new(AtomicBool::new(false)),
        Path::new(env!("CARGO_BIN_EXE_mdoc")),
        Duration::from_secs(30),
        Default::default(),
        None,
    )
    .unwrap();
    let pdf = gpui_pdf::parse(Arc::new(std::fs::read(&preview.pdf_path).unwrap())).unwrap();
    let page = gpui_pdf::extract_page_text(&pdf, 0).unwrap();
    // Text alone is insufficient: invisible space glyphs can make extraction
    // correct even when the visible words touch. Check the glyph outlines too.
    let page_width = gpui_pdf::page_dims(&pdf)[0].0;
    for (left, right) in [("Investment", "and"), ("Refund", "Part")] {
        let before = page.find_matches(left)[0][0];
        let after = page
            .find_matches(right)
            .into_iter()
            .flatten()
            .filter(|r| r.x > before.x && (r.y - before.y).abs() < 0.002)
            .min_by(|a, b| a.x.total_cmp(&b.x))
            .unwrap();
        let gap = (after.x - before.x - before.w) * page_width;
        assert!(
            gap >= 2.0,
            "{left} / {right} has only {gap:.2} pt of visible space"
        );
    }
    let text = page.text();
    assert!(
        text.contains(
            "Partnership of Investment and Plan Collateral Arrangement in the Refund Part Transfer"
        ),
        "heading lost its word boundaries: {text}"
    );
}

#[test]
fn worker_preserves_comments_and_source() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-preview/comments.docx");
    let original = std::fs::read(&path).unwrap();
    let preview = render_with_executable(
        &held(&path),
        Arc::new(AtomicBool::new(false)),
        Path::new(env!("CARGO_BIN_EXE_mdoc")),
        Duration::from_secs(30),
        Default::default(),
        None,
    )
    .unwrap();
    // The cards are AnyDoc's reading of the source, as its import reads it;
    // the worker gave only where they sit.
    assert_eq!(
        *preview.comments,
        mdoc_import::import::source_comments(&held(&path))
    );
    assert_eq!(preview.comments.len(), 6);
    let c = &preview.comments[0];
    assert_eq!(c.author, "Reviewer");
    assert_eq!(c.date, "2026-09-19T10:00:00Z");
    assert_eq!(
        c.text,
        "Please check this clause.\nSecond line: & <literal>."
    );
    assert_eq!(
        c.anchor_text,
        "First quoted paragraph.\nSecond quoted paragraph."
    );
    assert_eq!(preview.comments[1].author, "Ирина");
    assert_eq!(preview.comments[1].text, "Проверить условия.");
    assert_eq!(preview.comments[1].anchor_text, "Second quoted paragraph.");
    assert_eq!(preview.comments[2].anchor_text, "Point comment context.");
    assert_eq!(preview.comments[3].anchor_text, "");
    assert!(preview.comments[4].resolved);
    assert_eq!(preview.comments[5].parent.as_deref(), Some("0"));
    assert!(
        !preview
            .warnings
            .iter()
            .any(|w| w.contains("Comments are omitted"))
    );
    let pdf = gpui_pdf::parse(Arc::new(std::fs::read(&preview.pdf_path).unwrap())).unwrap();
    assert!(gpui_pdf::render_page(&pdf, 0, 1.0, gpui_pdf::PageTone::Original).is_ok());
    let text = gpui_pdf::extract_page_text(&pdf, 0).unwrap().text();
    assert!(text.contains("First"));
    assert!(text.contains("Second"));
    // Hidden text is counted, and drawn only when the pane shows it.
    assert_eq!(preview.hidden_text, 1);
    assert!(!text.contains("drafting note"), "{text}");
    let shown = render_with_executable(
        &held(&path),
        Arc::new(AtomicBool::new(false)),
        Path::new(env!("CARGO_BIN_EXE_mdoc")),
        Duration::from_secs(30),
        mdoc_import::docx_preview::RenderOptions {
            show_hidden_text: true,
        },
        None,
    )
    .unwrap();
    assert_eq!(shown.hidden_text, 1);
    assert!(shown.options.show_hidden_text);
    let pdf = gpui_pdf::parse(Arc::new(std::fs::read(&shown.pdf_path).unwrap())).unwrap();
    let text = gpui_pdf::extract_page_text(&pdf, 0).unwrap().text();
    assert!(text.contains("drafting note"), "{text}");
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
#[ignore = "requires locally uploaded examples"]
fn uploaded_comments_preserved() {
    for (name, count) in [
        (
            "adcourt-legal-opinion-meruna-trading-tax-and-14-pages.docx",
            1,
        ),
        ("cloud-request-double-cloud-2022-01-27.docx", 5),
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/docx-preview")
            .join(name);
        let preview = render_with_executable(
            &held(&path),
            Arc::new(AtomicBool::new(false)),
            Path::new(env!("CARGO_BIN_EXE_mdoc")),
            Duration::from_secs(30),
            Default::default(),
            None,
        )
        .unwrap();
        assert_eq!(preview.comments.len(), count);
        // Empty comments are legal (the cloud-request example contains two).
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(std::fs::read(&path).unwrap())).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("word/comments.xml").unwrap(), &mut xml)
            .unwrap();
        let original = roxmltree::Document::parse(&xml).unwrap();
        let ns = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        for comment in &*preview.comments {
            let node = original
                .descendants()
                .find(|n| {
                    n.has_tag_name((ns, "comment"))
                        && n.attribute((ns, "id")) == Some(comment.id.as_str())
                })
                .unwrap();
            let body = node
                .descendants()
                .filter(|n| n.has_tag_name((ns, "t")))
                .filter_map(|n| n.text())
                .collect::<String>();
            assert_eq!(compact(&comment.text), compact(&body));
            assert_eq!(
                comment.author,
                node.attribute((ns, "author")).unwrap_or_default()
            );
        }
        let pdf = gpui_pdf::parse(Arc::new(std::fs::read(&preview.pdf_path).unwrap())).unwrap();
        assert!(gpui_pdf::render_page(&pdf, 0, 1.0, gpui_pdf::PageTone::Original).is_ok());
    }
}

/// The worker renders the bytes it is given and the cards read them too:
/// no file needs to exist for either.
#[test]
fn worker_renders_held_bytes_without_their_file() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-preview/comments.docx");
    let held = Source::new(
        Path::new("never-written.docx").into(),
        None,
        std::fs::read(&path).unwrap(),
    );
    let preview = render_with_executable(
        &held,
        Arc::new(AtomicBool::new(false)),
        Path::new(env!("CARGO_BIN_EXE_mdoc")),
        Duration::from_secs(30),
        Default::default(),
        None,
    )
    .unwrap();
    assert_eq!(preview.comments.len(), 6);
    let pdf = gpui_pdf::parse(Arc::new(std::fs::read(&preview.pdf_path).unwrap())).unwrap();
    assert!(
        gpui_pdf::extract_page_text(&pdf, 0)
            .unwrap()
            .text()
            .contains("First")
    );
}

/// The bytes of the file at `path`, as an import holds them.
fn held(path: &Path) -> Source {
    Source::read(path).unwrap()
}

/// A source whose bytes the worker never reads.
fn unused() -> Source {
    Source::new("unused.docx".into(), None, Vec::new())
}

/// The real worker on `path`, with its PDF parsed.
fn render_parsed(
    path: &Path,
) -> (
    mdoc_import::docx_preview::DocxPreview,
    Arc<gpui_pdf::Document>,
) {
    let preview = render_with_executable(
        &held(path),
        Arc::new(AtomicBool::new(false)),
        Path::new(env!("CARGO_BIN_EXE_mdoc")),
        Duration::from_secs(30),
        Default::default(),
        None,
    )
    .unwrap();
    let pdf = gpui_pdf::parse(Arc::new(std::fs::read(&preview.pdf_path).unwrap())).unwrap();
    (preview, pdf)
}

/// A copy of a DOCX fixture with each XML part passed through `edit`.
fn edited_copy(
    source: &Path,
    dir: &Path,
    name: &str,
    edit: impl Fn(&str, String) -> String,
) -> std::path::PathBuf {
    use std::io::{Read, Write};
    let mut input = zip::ZipArchive::new(std::fs::File::open(source).unwrap()).unwrap();
    let path = dir.join(name);
    let mut output = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for i in 0..input.len() {
        let mut entry = input.by_index(i).unwrap();
        let entry_name = entry.name().unwrap().to_string();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if entry_name.ends_with(".xml") {
            data = edit(&entry_name, String::from_utf8(data).unwrap()).into_bytes();
        }
        output
            .start_file(entry_name, zip::write::SimpleFileOptions::default())
            .unwrap();
        output.write_all(&data).unwrap();
    }
    output.finish().unwrap();
    path
}

/// The fixture's body font replaced by `font`.
fn in_font(name: &str, xml: String, font: &str) -> String {
    if name == "word/styles.xml" {
        xml.replace(
            r#"w:ascii="Arial" w:hAnsi="Arial""#,
            &format!(r#"w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}""#),
        )
    } else {
        xml
    }
}

/// Every comment shape the render copy marks has its place on the page,
/// marks stay out of the clickable links, and a link under a comment stays
/// clickable (ROADMAP Comments item 1; the rail of item 3 builds on these).
#[test]
fn comment_anchors_cover_every_range_shape() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/docx-preview/comment-anchors.docx");
    let (preview, pdf) = render_parsed(&source);
    let anchors = |id: &str| -> Vec<_> { preview.anchors.iter().filter(|a| a.id == id).collect() };
    let one = |id: &str| {
        let found = anchors(id);
        assert_eq!(found.len(), 1, "comment {id}: {found:?}");
        found[0].clone()
    };
    // Several lines: a rectangle per line, all from the left margin but the last.
    let lines = one("0");
    assert_eq!(lines.rects.len(), 3);
    assert!(lines.rects.windows(2).all(|r| r[0].y < r[1].y));
    assert!(lines.rects[..2].iter().all(|r| r.w > 0.7));
    // Two paragraphs, a cell, Russian text: one rectangle per line.
    assert_eq!(one("1").rects.len(), 2);
    assert_eq!(one("7").rects.len(), 1);
    assert_eq!(one("8").rects.len(), 1);
    // Overlapping comments share "gamma": each range overhangs the other.
    let (a, b) = (one("2").rects[0], one("3").rects[0]);
    assert!((a.y - b.y).abs() < 1e-4);
    assert!(a.x < b.x && b.x < a.x + a.w && a.x + a.w < b.x + b.w);
    // A comment over a link and one inside a link, and the link stays clickable.
    let links = gpui_pdf::page_links(&pdf);
    let register = gpui_pdf::LinkTarget::Uri("https://example.com/register".into());
    let clickable = |r: &mdoc_import::docx_preview::AnchorRect| {
        links[0].iter().any(|l| {
            l.target == register && (l.y - r.y).abs() < 1e-4 && l.x < r.x + r.w && r.x < l.x + l.w
        })
    };
    assert!(clickable(&one("4").rects[0]));
    assert!(clickable(&one("11").rects[0]));
    // No mark is a link of its own; the commented cross-reference loses its
    // jump in the preview (a gpui-pdf link cannot resolve `#clause5`).
    assert!(links.iter().flatten().all(|l| l.target == register));
    assert_eq!(one("9").rects.len(), 1);
    // A point comment and an empty range: a zero-width spot on their line.
    for id in ["5", "6"] {
        let point = one(id);
        assert!(point.point && point.rects.len() == 1 && point.rects[0].w == 0.0);
    }
    let point = one("5").rects[0];
    let text = gpui_pdf::extract_page_text(&pdf, 0).unwrap();
    let here = text.find_matches("Point comment here")[0][0];
    assert!(
        (here.x + here.w - point.x).abs() < 0.005,
        "{here:?} {point:?}"
    );
    assert!(point.y > here.y && point.y < here.y + here.h);
    // Across a page break: an anchor on each page.
    let pages: Vec<_> = anchors("12").iter().map(|a| a.page).collect();
    assert_eq!(pages, [0, 1]);
    // No range, no reference: no place on the page.
    assert!(anchors("10").is_empty());
    // In document order.
    let order: Vec<_> = preview.anchors.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(
        order,
        [
            "0", "1", "2", "3", "4", "11", "5", "6", "7", "8", "9", "12", "12"
        ]
    );
}

/// The marker links never move a line, also where the font lacks the point
/// marker's zero-width space (Caladea): the render copy prints as Word does.
#[test]
fn comment_marks_never_move_a_line() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/docx-preview/comment-anchors.docx");
    let dir = tempfile::tempdir().unwrap();
    let strip = |name: &str, xml: String| -> String {
        if name == "word/comments.xml" {
            return r#"<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#.into();
        }
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let mut ranges: Vec<_> = doc
            .descendants()
            .filter(|n| n.tag_name().name().starts_with("comment"))
            .map(|n| n.range())
            .collect();
        ranges.reverse();
        let mut out = xml.clone();
        for range in ranges {
            out.replace_range(range, "");
        }
        out
    };
    for font in ["Arial", "Caladea"] {
        let marked = edited_copy(
            &fixture,
            dir.path(),
            &format!("marked-{font}.docx"),
            |n, x| in_font(n, x, font),
        );
        let plain = edited_copy(
            &fixture,
            dir.path(),
            &format!("plain-{font}.docx"),
            |n, x| strip(n, in_font(n, x, font)),
        );
        let (marked_preview, marked) = render_parsed(&marked);
        let (plain_preview, plain) = render_parsed(&plain);
        assert!(!marked_preview.anchors.is_empty() && plain_preview.anchors.is_empty());
        assert_eq!(marked.pages().len(), plain.pages().len());
        for page in 0..marked.pages().len() {
            let (a, b) = (
                gpui_pdf::extract_page_text(&marked, page).unwrap(),
                gpui_pdf::extract_page_text(&plain, page).unwrap(),
            );
            for line in a.text().lines().filter(|l| !l.trim().is_empty()) {
                let needle: String = line.chars().take(12).collect();
                let (Some(m), Some(p)) = (
                    a.find_matches(&needle).first().cloned(),
                    b.find_matches(&needle).first().cloned(),
                ) else {
                    panic!("{font} page {page}: {needle:?} is missing");
                };
                // A tenth of a point on a Letter page.
                assert!((m[0].y - p[0].y).abs() < 1.3e-4, "{font}: {needle:?} moved");
                assert!((m[0].x - p[0].x).abs() < 1.7e-4, "{font}: {needle:?} moved");
            }
        }
    }
}

/// The bundled fonts load, below installed fonts: a document in Calibri uses
/// an installed Calibri and falls to Carlito only without one, and one in
/// Liberation Serif, which no system ships, uses the bundle.
#[test]
fn bundled_fonts_rank_below_installed_ones() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-preview/comments.docx");
    let dir = tempfile::tempdir().unwrap();
    let embedded = |font: &str| {
        let copy = edited_copy(&fixture, dir.path(), &format!("{font}.docx"), |n, x| {
            in_font(n, x, font)
        });
        let (preview, _) = render_parsed(&copy);
        let pdf = std::fs::read(&preview.pdf_path).unwrap();
        // The fonts' ToUnicode maps, outside the compressed object streams.
        let key = b"/CMapName /";
        let mut names: Vec<String> = pdf
            .windows(key.len())
            .enumerate()
            .filter(|(_, w)| *w == key)
            .map(|(i, _)| {
                pdf[i + key.len()..]
                    .iter()
                    .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-')
                    .map(|b| char::from(*b))
                    .collect()
            })
            .collect();
        names.sort();
        names.dedup();
        names
    };
    let calibri_installed = cfg!(windows) || mac_has_font("calibri");
    let expected = if calibri_installed {
        "Calibri"
    } else {
        "Carlito"
    };
    assert_eq!(embedded("Calibri"), [format!("{expected}-UTF16")]);
    let serif = embedded("Liberation Serif");
    assert_eq!(serif.len(), 1);
    assert!(serif[0].starts_with("LiberationSerif"), "{serif:?}");
}

/// Whether a font file whose name starts with `stem` sits where docxide looks
/// on macOS (Office keeps its own copies elsewhere, which docxide never reads).
fn mac_has_font(stem: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    [
        "/Library/Fonts",
        "/System/Library/Fonts",
        "/System/Library/Fonts/Supplemental",
    ]
    .iter()
    .map(std::path::PathBuf::from)
    .chain([Path::new(&home).join("Library/Fonts")])
    .filter_map(|dir| std::fs::read_dir(dir).ok())
    .flatten()
    .flatten()
    .any(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .to_lowercase()
            .starts_with(stem)
    })
}
