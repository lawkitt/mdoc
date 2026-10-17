//! Explicit host CPU measurements, not display latency or visual acceptance.
use super::*;
use crate::document_view_tests::{boot, close_document, open_document};
use gpui::{TestAppContext, VisualTestContext};
use std::time::Instant;

pub(crate) fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

pub(crate) fn report(label: &str, values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    eprintln!(
        "HOST_PERF {label} median_ms={:.3} p95_ms={:.3}",
        values[values.len() / 2],
        values[(values.len() * 95).div_ceil(100).saturating_sub(1)]
    );
    values[values.len() / 2]
}

// Debug-profile regression ceilings, rounded up from the recorded host runs
// with at least 20% headroom: scroll, typing, search burst, navigation and
// common-word navigation. These are not interactive frame-time targets.
fn budget(name: &str) -> Option<[f64; 5]> {
    match name {
        "ordinary" => Some([7., 7., 89., 8., 4.]),
        "contract" => Some([11., 12., 216., 12., 7.]),
        "legal_opinion" => Some([13., 15., 259., 7., 8.]),
        "large" => Some([31., 35., 393., 34., 17.]),
        "background" => Some([36., 40., 606., 38., 19.]),
        "long_paragraph" => Some([19., 25., 502., 36., 11.]),
        "tables" => Some([11., 12., 140., 12., 6.]),
        "dense_matches" => Some([19., 22., 1237., 78., 12.]),
        _ => None,
    }
}

/// A synthetic ~30-page agreement shaped like converted legal documents:
/// numbered articles, nested clauses, defined terms, a schedule table and long
/// wrapped paragraphs. No real names; parties are generic.
pub(crate) fn contract_markdown() -> String {
    let paragraph = "The Parties acknowledge that the **Confidential Information** \
        disclosed under this Agreement remains the property of the Disclosing Party, \
        and that nothing in this Agreement grants the Receiving Party any licence, \
        right or interest in it, except the limited right to use it for the \
        *Purpose* during the Term and in accordance with Clause 7.2 and Schedule 1.";
    let mut out = String::from("# Settlement and Services Agreement\n\n");
    out.push_str("This Agreement is made between **Alpha Holdings Ltd** (the \"Company\") and **Beta Services LLC** (the \"Contractor\").\n\n");
    for article in 1..=24 {
        out.push_str(&format!("## {article}. Article {article}\n\n"));
        out.push_str(paragraph);
        out.push_str("\n\n");
        for clause in 1..=6 {
            out.push_str(&format!("{clause}. {paragraph}\n"));
            if clause % 3 == 0 {
                for sub in ["first", "second", "third"] {
                    out.push_str(&format!(
                        "   - the {sub} condition in Clause {article}.{clause} is satisfied;\n"
                    ));
                }
            }
        }
        out.push('\n');
        if article % 8 == 0 {
            out.push_str("| Milestone | Amount | Due date |\n| --- | --- | --- |\n");
            for row in 1..=6 {
                out.push_str(&format!(
                    "| Payment {row} | USD {row}0,000 | 2026-0{row}-15 |\n"
                ));
            }
            out.push('\n');
        }
    }
    out
}

/// The scrubbed 37-page legal opinion (`tests/fixtures/docx-preview/README.md`),
/// as the DOCX importer produces it.
pub(crate) fn legal_opinion_markdown() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/docx-preview/legal-opinion.docx");
    import::convert(&path).unwrap().markdown
}

pub(super) fn rss_kib() -> Option<u64> {
    // Optional observation only: allocators and global text/GPU caches retain
    // memory, so RSS is not an assertion about document-owned resource release.
    #[cfg(unix)]
    {
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        String::from_utf8(output.stdout).ok()?.trim().parse().ok()
    }
    #[cfg(not(unix))]
    {
        None
    }
}

#[gpui::test]
#[ignore = "host performance matrix; run serially on an idle machine"]
fn host_performance_matrix(cx: &mut TestAppContext) {
    let (mut app, cx) = boot(cx);
    // Fixed outer window; log the actual reading-column and pane widths below.
    cx.simulate_resize(gpui::size(px(1332.), px(750.)));
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("pixel.png");
    // A local image makes image resolution/decoding part of the workload.
    image::RgbaImage::from_pixel(32, 32, image::Rgba([90, 120, 160, 255]))
        .save(&image)
        .unwrap();
    let mixed = "# Heading\n\nalpha **beta** Русский 😀 [link](https://example.com)\n\n![local](pixel.png)\n\n```rust\nlet alpha = 1;\n```\n\n";
    let mut workloads = vec![
        ("ordinary", mixed.repeat(16)),
        ("contract", contract_markdown()),
        ("legal_opinion", legal_opinion_markdown()),
        ("large", mixed.repeat(512)),
        ("background", mixed.repeat(600)),
        ("long_paragraph", "alpha beta words ".repeat(3000)),
        (
            "tables",
            format!(
                "<!-- table:grid cols=180,180 -->\n| alpha | beta |\n| --- | --- |\n{}",
                "| alpha **beta** words | Русский 😀 |\n".repeat(256)
            ),
        ),
        (
            "dense_matches",
            format!("```\n{}\n```", "alpha ".repeat(10_000)),
        ),
    ];
    // Optional local corpus; only labels and measurements are logged.
    if let Some(path) = std::env::var_os("MDOC_PERF_MARKDOWN") {
        workloads.push(("local", std::fs::read_to_string(path).unwrap()));
    }
    // Optional comma-separated subset, for profiling one workload.
    if let Ok(only) = std::env::var("MDOC_PERF_WORKLOADS") {
        workloads.retain(|(name, _)| only.split(',').any(|w| w.trim() == *name));
    }
    let mut failures = Vec::new();
    for (name, source) in workloads {
        let path = if name == "local" {
            PathBuf::from(std::env::var_os("MDOC_PERF_MARKDOWN").unwrap())
        } else {
            let path = dir.path().join(format!("{name}.md"));
            std::fs::write(&path, &source).unwrap();
            path
        };
        let started = Instant::now();
        app = open_document(&app, path, cx);
        cx.run_until_parked();
        draw(cx);
        eprintln!(
            "HOST_PERF {name} bytes={} open_ms={:.3} profile={} pane_width={} editor_width={:?}",
            source.len(),
            started.elapsed().as_secs_f64() * 1000.,
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            app.read_with(cx, |v, _| f32::from(v.scroll.bounds().size.width)),
            app.read_with(cx, |v, cx| v.editor.read(cx).layout_width().map(f32::from)),
        );
        let mut scroll = Vec::new();
        for frame in 0..20 {
            let started = Instant::now();
            app.update(cx, |app, cx| {
                app.scroll.set_offset(gpui::point(
                    px(0.),
                    -app.scroll.max_offset().y.abs() * ((frame % 10) as f32 / 9.),
                ));
                cx.notify();
            });
            draw(cx);
            scroll.push(started.elapsed().as_secs_f64() * 1000.);
        }
        assert!(
            cx.update(|_, cx| app.read(cx).scroll.offset().y < px(-100.)),
            "{name} must actually scroll"
        );
        let scroll_ms = report(&format!("{name} scroll"), &mut scroll);
        app.update_in(cx, |app, window, cx| {
            app.editor.update(cx, |editor, cx| {
                editor.set_cursor(0, cx);
                editor.focus(window, cx);
            });
        });
        let mut typing = Vec::new();
        for _ in 0..7 {
            let started = Instant::now();
            cx.simulate_input("x");
            cx.run_until_parked();
            draw(cx);
            typing.push(started.elapsed().as_secs_f64() * 1000.);
        }
        assert_eq!(
            cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
            format!("xxxxxxx{source}")
        );
        let typing_ms = report(&format!("{name} typing"), &mut typing);
        let started = Instant::now();
        cx.dispatch_action(FindMarkdown);
        for query in ["a", "al", "alp", "alph", "alpha"] {
            cx.dispatch_action(FindMarkdown);
            cx.simulate_input(query);
        }
        cx.run_until_parked();
        draw(cx);
        let burst_ms = started.elapsed().as_secs_f64() * 1000.;
        eprintln!("HOST_PERF {name} search_burst_and_paint_ms={burst_ms:.3}");
        cx.update(|_, cx| {
            let app = app.read(cx);
            assert!(app.search.task.is_none());
            let expected =
                SearchIndex::from_markdown(app.editor.read(cx).text()).find("alpha", false);
            assert_eq!(app.search.matches, expected, "latest query must finish");
        });
        let mut highlights = Vec::new();
        for _ in 0..7 {
            let started = Instant::now();
            cx.dispatch_action(FindNextMarkdown);
            draw(cx);
            highlights.push(started.elapsed().as_secs_f64() * 1000.);
        }
        let navigation_ms = report(&format!("{name} highlight_navigation"), &mut highlights);
        // A common word, as people search real documents: many matches, some in
        // tables. Each step scrolls over up to three frames (search refinement).
        cx.dispatch_action(FindMarkdown);
        cx.simulate_input("the");
        cx.run_until_parked();
        draw(cx);
        let common = cx.update(|_, cx| app.read(cx).search.matches.len());
        let mut frames = Vec::new();
        for _ in 0..12 {
            let mut started = Instant::now();
            cx.dispatch_action(FindNextMarkdown);
            for _ in 0..3 {
                draw(cx);
                frames.push(started.elapsed().as_secs_f64() * 1000.);
                started = Instant::now();
            }
        }
        eprintln!("HOST_PERF {name} common_word_matches={common}");
        let common_ms = report(&format!("{name} common_word_navigation_frame"), &mut frames);
        if let Some(limits) = budget(name) {
            for ((label, measured), limit) in [
                ("scroll", scroll_ms),
                ("typing", typing_ms),
                ("burst", burst_ms),
                ("navigation", navigation_ms),
                ("common_word_navigation", common_ms),
            ]
            .into_iter()
            .zip(limits)
            {
                if measured > limit {
                    failures.push(format!("{name} {label}: {measured:.3}ms exceeds {limit}ms"));
                }
            }
        }
        cx.dispatch_action(CloseMarkdownSearch);
        app = close_document(&app, cx);
        cx.run_until_parked();
    }
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let large = dir.path().join("large.md");
    // A filtered run without `large` skips the preview switch cycles.
    let cycles = if large.exists() { 12 } else { 0 };
    for cycle in 0..cycles {
        let started = Instant::now();
        app = open_document(&app, large.clone(), cx);
        app.update_in(cx, |app, window, cx| {
            if cycle % 2 == 0 {
                app.open_pdf(base.join("tests/fixtures/reference.pdf"), window, cx);
            } else {
                app.open_docx(
                    base.join("tests/fixtures/docx-preview/comments.docx"),
                    window,
                    cx,
                );
            }
        });
        cx.run_until_parked();
        draw(cx);
        eprintln!(
            "HOST_PERF switch_cycle={cycle} open_preview_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.
        );
        let (old_pdf, old_comments, old_temp) = cx.update(|_, cx| {
            let preview = &app.read(cx).preview;
            assert!(!preview.loading);
            assert!(!preview.retryable, "preview must load, not silently fail");
            (
                preview.pdf.as_ref().unwrap().downgrade(),
                preview.comment_panel.as_ref().map(Entity::downgrade),
                preview.docx.as_ref().map(|docx| docx.pdf_path.clone()),
            )
        });
        cx.dispatch_action(FindMarkdown);
        cx.simulate_input("alpha");
        cx.run_until_parked();
        app.update_in(cx, |app, window, cx| {
            app.close_preview(window, cx);
        });
        cx.run_until_parked();
        draw(cx);
        app = close_document(&app, cx);
        assert!(old_pdf.upgrade().is_none(), "old PDF entity retained");
        assert!(
            old_comments.is_none_or(|panel| panel.upgrade().is_none()),
            "old comments retained"
        );
        assert!(
            old_temp.is_none_or(|path| !path.exists()),
            "temporary PDF retained"
        );
        eprintln!("HOST_PERF switch_cycle={cycle} rss_kib={:?}", rss_kib());
    }
    assert!(
        failures.is_empty(),
        "host regression budgets exceeded: {failures:#?}"
    );
}

#[gpui::test]
#[ignore = "local corpus; set MDOC_PERF_PREVIEW to a PDF or DOCX, run serially"]
fn local_preview_performance(cx: &mut TestAppContext) {
    let path = PathBuf::from(
        std::env::var_os("MDOC_PERF_PREVIEW")
            .expect("set MDOC_PERF_PREVIEW to a local PDF or DOCX"),
    );
    assert!(
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf")
                || extension.eq_ignore_ascii_case("docx"))
    );
    let (app, cx) = boot(cx);
    // Fixed outer window; log the actual reading-column and pane widths below.
    cx.simulate_resize(gpui::size(px(1332.), px(750.)));
    let mut times = Vec::new();
    for cycle in 0..3 {
        let started = Instant::now();
        app.update_in(cx, |app, window, cx| {
            app.open_path(path.clone(), window, cx)
        });
        cx.run_until_parked();
        draw(cx);
        times.push(started.elapsed().as_secs_f64() * 1000.);
        let (pdf, backing) = cx.update(|_, cx| {
            let preview = &app.read(cx).preview;
            assert!(
                !preview.loading && !preview.retryable,
                "local preview failed"
            );
            (
                preview.pdf.as_ref().unwrap().downgrade(),
                preview.docx.as_ref().map(|docx| docx.pdf_path.clone()),
            )
        });
        app.update_in(cx, |app, window, cx| app.close_preview(window, cx));
        cx.run_until_parked();
        draw(cx);
        assert!(pdf.upgrade().is_none());
        assert!(backing.is_none_or(|path| !path.exists()));
        eprintln!(
            "HOST_PERF local_preview cycle={cycle} rss_kib={:?}",
            rss_kib()
        );
    }
    report("local_preview open", &mut times);
}

#[gpui::test]
#[ignore = "2 MiB / 20,000 annotation CPU matrix; run serially on an idle machine"]
fn pii_annotation_geometry_matrix(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    cx.simulate_resize(gpui::size(px(1332.), px(750.)));
    for (name, seed) in [
        ("prose", "PERSON ordinary prose.\n"),
        ("long_line", "PERSON ordinary prose. "),
        ("hidden", "[x](https://example.invalid/PERSON)\n"),
        ("utf8_table", "| PERSON | Русский текст |\n"),
        (
            "dense_hidden_wrapped",
            "Visible link [a label](https://x.invalid/PERSON) ",
        ),
    ] {
        let mut source = if name == "utf8_table" {
            format!(
                "| Name | Contact |\n| ---- | ---- |\n{}",
                seed.repeat(20000)
            )
        } else {
            seed.repeat(20000)
        };
        source.push_str(&" ".repeat(2097152 - source.len()));
        let annotations: Vec<_> = source
            .match_indices("PERSON")
            .enumerate()
            .map(|(id, (at, _))| mdoc_editor::SourceAnnotation {
                id: id as u64,
                range: at..at + 6,
                color: gpui::rgba(0xffaa0022).into(),
                active_color: gpui::rgba(0xffaa0055).into(),
                border: gpui::transparent_black(),
                text_color: None,
                underline: None,
                dotted: false,
            })
            .collect();
        app.update(cx, |app, cx| {
            app.editor.update(cx, |e, cx| {
                e.set_text(source.clone(), cx);
                e.set_cursor(e.text().len(), cx);
                e.set_annotations(e.revision(), vec![], cx);
            });
            app.scroll.set_offset(gpui::point(px(0.), px(0.)));
        });
        draw(cx);
        draw(cx);
        let mut baseline = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            draw(cx);
            baseline.push(start.elapsed().as_secs_f64() * 1000.);
        }
        app.update(cx, |app, cx| {
            app.editor.update(cx, |e, cx| {
                e.set_annotations(e.revision(), annotations.clone(), cx)
            })
        });
        let start = Instant::now();
        draw(cx);
        let cold = start.elapsed().as_secs_f64() * 1000.;
        let mut annotated = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            draw(cx);
            annotated.push(start.elapsed().as_secs_f64() * 1000.);
        }
        let count = app.read_with(cx, |app, cx| {
            (0..20000)
                .filter(|&id| app.editor.read(cx).annotation_bounds(id).is_some())
                .count()
        });
        assert!(count > 0 && count < 500, "viewport geometry count {count}");
        eprintln!(
            "PII_GEOMETRY workload={name} bytes={} occurrences=20000 cold_ms={cold:.3} visible_bounds={count} rss_kib={:?}",
            source.len(),
            rss_kib()
        );
        let base = report(&format!("pii_{name}_unannotated"), &mut baseline);
        let warm = report(&format!("pii_{name}_annotated"), &mut annotated);
        app.update(cx, |app, _| {
            app.scroll
                .set_offset(gpui::point(px(0.), -app.scroll.max_offset().y / 2.))
        });
        draw(cx);
        draw(cx);
        let middle_count = app.read_with(cx, |app, cx| {
            (0..20000)
                .filter(|&id| app.editor.read(cx).annotation_bounds(id).is_some())
                .count()
        });
        assert!(
            middle_count < 500,
            "scrolled viewport geometry count {middle_count}"
        );
        eprintln!(
            "PII_GEOMETRY workload={name} added_median_ms={:.3} middle_visible_bounds={middle_count}",
            warm - base
        );
    }
}
