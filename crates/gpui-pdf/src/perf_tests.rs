//! Explicit performance harness; supply locally converted fixtures with PDF_PERF_DIR.
//! Measures CPU work and raster availability, not display presentation latency.
use super::*;

#[gpui::test]
#[ignore]
fn long_document_scroll_budget(cx: &mut gpui::TestAppContext) {
    let directory = std::env::var_os("PDF_PERF_DIR").expect("set PDF_PERF_DIR");
    let mut paths = std::fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
        .collect::<Vec<_>>();
    if let Ok(filter) = std::env::var("PDF_PERF_FILE") {
        paths.retain(|p| p.file_name().unwrap().to_string_lossy().contains(&filter));
    }
    paths.sort();
    assert!(!paths.is_empty());
    let mut failures = Vec::new();
    for path in paths {
        let started = std::time::Instant::now();
        let (view, cx) = cx.add_window_view(|_, cx| {
            PdfView::new(
                path.clone(),
                Rc::new(PdfStyle::default),
                Rc::new(|| 1.0),
                cx,
            )
        });
        cx.simulate_resize(gpui::size(px(550.), px(750.)));
        if std::env::var_os("PDF_PERF_FIT").is_some() {
            view.update(cx, |v, cx| v.fit_width(cx));
        }
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        settle_rasters(&view, cx);
        let initial_ms = started.elapsed().as_secs_f64() * 1000.;
        let mut frames = Vec::new();
        let mut settles = Vec::new();
        let (pages, page_h) = cx.update(|_, cx| {
            let v = view.read(cx);
            assert!(v.is_loaded());
            (
                v.dims.len(),
                display_height(v.dims[0], v.page_width()) + PAGE_GAP,
            )
        });
        // Visit every page in both directions, including mixed-size documents.
        for (frame, page) in (0..pages).chain((0..pages).rev()).enumerate() {
            let offset = cx.update(|_, cx| {
                let v = view.read(cx);
                page_top_y(&v.dims, v.page_width(), page)
            });
            let started = std::time::Instant::now();
            cx.update(|window, cx| {
                view.update(cx, |v, cx| {
                    v.scroll.set_offset(point(px(0.), px(-offset)));
                    cx.notify();
                });
                window.refresh();
                window.draw(cx).clear(cx);
            });
            let draw_ms = started.elapsed().as_secs_f64() * 1000.;
            settle_rasters(&view, cx);
            frames.push(draw_ms);
            settles.push(started.elapsed().as_secs_f64() * 1000.);
            cx.update(|_, cx| {
                let v = view.read(cx);
                if page > 0 {
                    assert!(
                        f32::from(v.scroll.offset().y) < -100.,
                        "viewport must actually scroll"
                    );
                }
                assert!(
                    v.pages[page].image.is_some(),
                    "page {page} never rasterized at step {frame}"
                );
            });
        }
        let (offset, renders) = cx.update(|_, cx| {
            let v = view.read(cx);
            (f32::from(v.scroll.offset().y), v.render_requests)
        });
        assert!(offset <= 0.);
        frames.sort_by(f64::total_cmp);
        settles.sort_by(f64::total_cmp);
        let p95 = frames[frames.len() * 95 / 100];
        let settle = settles[settles.len() * 95 / 100];
        eprintln!(
            "PDF_PERF file={} pages={pages} initial_ms={initial_ms:.2} page_h={page_h:.2} frame_median_ms={:.2} frame_p95_ms={p95:.2} settle_p95_ms={settle:.2} raster_requests={renders}",
            path.file_name().unwrap().to_string_lossy(),
            frames[frames.len() / 2]
        );
        if p95 > 16.667 || settle > 100. {
            failures.push(path.file_name().unwrap().to_owned());
        }
        view.update_in(cx, |v, w, cx| v.release(w, cx));
    }
    assert!(
        failures.is_empty(),
        "scroll CPU/raster availability budget exceeded: {failures:?}"
    );
}

fn blank_document(pages: usize) -> tempfile::NamedTempFile {
    use std::io::Write;
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        format!(
            "<< /Type /Pages /Count {pages} /Kids [{}] >>",
            (0..pages)
                .map(|p| format!("{} 0 R", p + 3))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    ];
    for _ in 0..pages {
        objects.push(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> >>".into(),
        );
    }
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        write!(bytes, "{} 0 obj\n{object}\nendobj\n", i + 1).unwrap();
    }
    let xref = bytes.len();
    write!(bytes, "xref\n0 {}\n0000000000 65535 f \n", offsets.len()).unwrap();
    for offset in &offsets[1..] {
        writeln!(bytes, "{offset:010} 00000 n ").unwrap();
    }
    write!(
        bytes,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        offsets.len()
    )
    .unwrap();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&bytes).unwrap();
    file
}

#[gpui::test]
fn fast_scroll_bounds_raster_work(cx: &mut gpui::TestAppContext) {
    let file = blank_document(40);
    let (view, cx) = cx.add_window_view(|_, cx| {
        PdfView::new(
            file.path().to_owned(),
            Rc::new(PdfStyle::default),
            Rc::new(|| 1.0),
            cx,
        )
    });
    cx.simulate_resize(gpui::size(px(550.), px(750.)));
    cx.run_until_parked();
    for page in [8, 16, 24, 32] {
        cx.update(|window, cx| {
            view.update(cx, |v, cx| v.go_to_page(page, cx));
            window.refresh();
            window.draw(cx).clear(cx);
        });
    }
    cx.update(|_, cx| {
        let active = view
            .read(cx)
            .pages
            .iter()
            .filter(|p| p.loading.is_some())
            .count();
        assert!(
            active <= 2,
            "fast scrolling left {active} raster jobs queued/running"
        );
    });
    settle_rasters(&view, cx);
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(v.current_page_index(), 32);
        assert!(v.pages[32].image.is_some());
        assert_eq!(v.active_renders, 0);
    });
    // Invalidate while replacements are queued, then release before completion.
    cx.update(|window, cx| {
        view.update(cx, |v, cx| v.set_zoom(1.25, cx));
        window.refresh();
        window.draw(cx).clear(cx);
        view.update(cx, |v, cx| v.release(window, cx));
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(v.active_renders, 0);
        assert!(v.pages.is_empty());
        assert!(v.pending_drops.is_empty());
    });
}

#[gpui::test]
fn fit_width_converges_without_render_storm(cx: &mut gpui::TestAppContext) {
    let file = blank_document(2);
    let (view, cx) = cx.add_window_view(|_, cx| {
        PdfView::new(
            file.path().to_owned(),
            Rc::new(PdfStyle::default),
            Rc::new(|| 1.0),
            cx,
        )
    });
    cx.simulate_resize(gpui::size(px(550.), px(750.)));
    cx.run_until_parked();
    view.update(cx, |v, cx| v.fit_width(cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert!(
            v.generation <= 2,
            "fit needed {} scale generations",
            v.generation
        );
        assert!(f32::from(v.scroll.bounds().size.width) <= 550.);
    });
    view.update_in(cx, |v, w, cx| v.release(w, cx));
}

#[gpui::test]
fn initial_fit_waits_for_measured_viewport(cx: &mut gpui::TestAppContext) {
    let file = blank_document(4);
    let document = prepare(Arc::new(std::fs::read(file.path()).unwrap()), "").unwrap();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = PdfView::new(
            file.path().to_owned(),
            Rc::new(PdfStyle::default),
            Rc::new(|| 1.0),
            cx,
        );
        view.install_document(document, cx);
        view.fit_width(cx);
        view.ensure_window(window, cx);
        assert_eq!(
            view.render_requests, 0,
            "raster requested before fit can measure the pane"
        );
        view
    });
    cx.simulate_resize(gpui::size(px(550.), px(750.)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.render_requests > 0);
        assert!(view.page_width() < PAGE_WIDTH);
    });
    view.update_in(cx, |v, w, cx| v.release(w, cx));
}

fn settle_rasters(view: &gpui::Entity<PdfView>, cx: &mut gpui::VisualTestContext) {
    for _ in 0..64 {
        cx.run_until_parked();
        let ready = cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let v = view.read(cx);
            let (start, end) = keep_window(
                &v.dims,
                v.page_width(),
                f32::from(-v.scroll.offset().y),
                f32::from(v.scroll.bounds().size.height),
            );
            v.active_renders == 0
                && !v.pages.is_empty()
                && v.pages[start..=end]
                    .iter()
                    .all(|slot| slot.image.is_some() && slot.image_gen == v.generation)
        });
        if ready {
            return;
        }
    }
    panic!("viewport rasters did not settle");
}
