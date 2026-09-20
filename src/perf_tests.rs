//! Explicit host CPU measurements, not display latency or visual acceptance.
use super::*;
use crate::ui_tests::boot;
use gpui::{TestAppContext, VisualTestContext};
use std::time::Instant;

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn report(label: &str, values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    eprintln!(
        "HOST_PERF {label} median_ms={:.3} p95_ms={:.3}",
        values[values.len() / 2],
        values[(values.len() * 95).div_ceil(100).saturating_sub(1)]
    );
    values[values.len() / 2]
}

// Debug-profile regression ceilings, rounded up from the recorded host runs
// with at least 20% headroom. These are not interactive frame-time targets.
fn budget(name: &str) -> Option<[f64; 4]> {
    match name {
        "ordinary" => Some([12., 12., 140., 13.]),
        "large" => Some([320., 320., 3400., 310.]),
        "background" => Some([380., 380., 5700., 370.]),
        "long_paragraph" => Some([50., 65., 800., 62.]),
        "tables" => Some([62., 64., 780., 68.]),
        "dense_matches" => Some([50., 60., 1400., 110.]),
        _ => None,
    }
}

fn rss_kib() -> Option<u64> {
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
    let (app, cx) = boot(cx);
    cx.simulate_resize(gpui::size(px(1100.), px(750.)));
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("pixel.png");
    // A local image makes image resolution/decoding part of the workload.
    image::RgbaImage::from_pixel(32, 32, image::Rgba([90, 120, 160, 255]))
        .save(&image)
        .unwrap();
    let mixed = "# Heading\n\nalpha **beta** Русский 😀 [link](https://example.com)\n\n![local](pixel.png)\n\n```rust\nlet alpha = 1;\n```\n\n";
    let mut workloads = vec![
        ("ordinary", mixed.repeat(16)),
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
    for (name, source) in workloads {
        let path = if name == "local" {
            PathBuf::from(std::env::var_os("MDOC_PERF_MARKDOWN").unwrap())
        } else {
            let path = dir.path().join(format!("{name}.md"));
            std::fs::write(&path, &source).unwrap();
            path
        };
        let started = Instant::now();
        app.update_in(cx, |app, window, cx| {
            app.proceed(Next::Open(path), window, cx)
        });
        cx.run_until_parked();
        draw(cx);
        eprintln!(
            "HOST_PERF {name} bytes={} open_ms={:.3}",
            source.len(),
            started.elapsed().as_secs_f64() * 1000.
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
        if let Some(limits) = budget(name) {
            for ((label, measured), limit) in [
                ("scroll", scroll_ms),
                ("typing", typing_ms),
                ("burst", burst_ms),
                ("navigation", navigation_ms),
            ]
            .into_iter()
            .zip(limits)
            {
                assert!(
                    measured <= limit,
                    "{name} {label}: {measured:.3}ms exceeds {limit}ms host regression budget"
                );
            }
        }
        cx.dispatch_action(CloseMarkdownSearch);
        app.update_in(cx, |app, window, cx| app.proceed(Next::New, window, cx));
        cx.run_until_parked();
    }
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let large = dir.path().join("large.md");
    for cycle in 0..12 {
        let started = Instant::now();
        app.update_in(cx, |app, window, cx| {
            app.proceed(Next::Open(large.clone()), window, cx);
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
            app.proceed(Next::New, window, cx);
        });
        cx.run_until_parked();
        draw(cx);
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
    cx.simulate_resize(gpui::size(px(1100.), px(750.)));
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
