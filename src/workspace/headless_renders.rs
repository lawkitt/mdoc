//! Dev-only screen evidence. TestPlatform owns every window: no native window,
//! application event loop, activation, clipboard or OS prompt is used.
//!
//! Scenes live in one module per feature; `scenarios` runs them in order, in
//! each theme. Every screen a scene captures is listed in
//! `tests/headless_screens.txt`.
mod cases;
mod comments;
mod comparison;
mod confirm;
mod decisions;
mod docx_source;
mod encoder;
mod folder_sheet;
mod import;
mod narrow;
mod navigation;
mod recents;
mod restore;
mod review;
mod search;
mod settings;
mod shelf;
mod shell;

use super::{Workspace, app_actions::bind as bind_app_actions};
use crate::{bind_all_keys, pii::ui::CompareMode};
use gpui::{Context, HeadlessAppContext, Keystroke, Window, prelude::*, px, size};
use image::ImageEncoder;
use mdoc_store::session_store::{Session, ThemeChoice};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, String>;
const WIDTH: f32 = 1100.;
const HEIGHT: f32 = 760.;
const MAX_TABS: usize = 128;

struct Driver {
    cx: HeadlessAppContext,
    window: gpui::AnyWindowHandle,
    /// This theme's output directory.
    out: PathBuf,
    /// One record per captured screen, in capture order.
    records: Vec<Value>,
    frames: Vec<Value>,
    focus_rings: Vec<Value>,
    /// PNG encoding overlaps the next frames on a background thread.
    pngs: encoder::Encoder,
}

impl Driver {
    /// Paired images of the same state: only keyboard focus changes. The
    /// contract test checks pixels, including that the control keeps its fill.
    fn focus_pair(&mut self, name: &str, target: gpui::FocusHandle) -> Result<()> {
        let out = self.out.clone();
        let rest = format!("focus-{name}-rest.png");
        let focused = format!("focus-{name}.png");
        self.change(|w, window, cx| window.focus(&w.focus, cx))?;
        self.settle()?;
        self.png(&out.join(&rest))?;
        self.change(|_, window, cx| window.focus(&target, cx))?;
        self.settle()?;
        self.png(&out.join(&focused))?;
        self.focus_rings
            .push(json!({"control": name, "rest": rest, "focused": focused}));
        Ok(())
    }

    fn change<R>(
        &mut self,
        f: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) -> R,
    ) -> Result<R> {
        self.window
            .downcast::<Workspace>()
            .expect("workspace window")
            .update(&mut self.cx, f)
            .map_err(|e| e.to_string())
    }

    fn draw(&mut self, phase: &str) -> Result<()> {
        let start = Instant::now();
        self.cx
            .update_window(self.window, |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .map_err(|e| e.to_string())?;
        self.frames.push(
            json!({"phase": phase, "layout_paint_ms": start.elapsed().as_secs_f64() * 1000.}),
        );
        Ok(())
    }

    fn settle(&mut self) -> Result<()> {
        for _ in 0..3 {
            self.cx.run_until_parked();
            self.cx.advance_clock(Duration::from_millis(25));
            self.draw("settle")?;
        }
        Ok(())
    }

    /// Resize the window; each capture records the size it was taken at.
    fn resize(&mut self, width: f32, height: f32) -> Result<()> {
        self.cx
            .update_window(self.window, |_, window, cx| {
                window.resize(size(px(width), px(height)));
                window.bounds_changed(cx);
            })
            .map_err(|e| e.to_string())?;
        self.settle()
    }

    fn key(&mut self, key: &str) -> Result<()> {
        self.cx
            .update_window(self.window, |_, window, cx| {
                window.dispatch_keystroke(Keystroke::parse(key).expect("static keystroke"), cx);
            })
            .map_err(|e| e.to_string())?;
        self.cx.run_until_parked();
        Ok(())
    }

    fn png(&mut self, path: &Path) -> Result<f64> {
        let start = Instant::now();
        let image = self
            .cx
            .capture_screenshot(self.window)
            .map_err(|e| e.to_string())?;
        let ms = start.elapsed().as_secs_f64() * 1000.;
        self.pngs.write(path, image)?;
        Ok(ms)
    }

    /// Settles, records the screen's state, its PNG and its Tab walk.
    fn capture(&mut self, name: &str) -> Result<()> {
        let out = self.out.clone();
        eprintln!(
            "Rendering {}/{name}",
            out.file_name().unwrap().to_string_lossy()
        );
        self.frames.clear();
        self.settle()?;
        let state = if self.window.downcast::<Workspace>().is_some() {
            self.change(|w, _, cx| {
                let members = w.cases.page.as_ref()
                    .and_then(|page| w.case_coordinator.case(page.id()))
                    .map(|case| case.members.iter().map(|path| {
                        let tab = w.find_path(path, None, cx);
                        let mentions = tab.and_then(|tab| w.tabs.get(tab)?.view()).map(|view| {
                            let review = view.read(cx).pii.review();
                            case.exported().iter().map(|identity| {
                                review.identity_by_alias(&identity.alias)
                                    .map_or(0, |id| review.identity_count(id))
                            }).collect::<Vec<_>>()
                        });
                        json!({"open": tab.is_some(), "mentions": mentions})
                    }).collect::<Vec<_>>());
                json!({
                    "home": w.showing_home(cx),
                    "sidebar_expanded": w.sidebar.visible(),
                    "sidebar_width": w.sidebar.width(),
                    "back_enabled": w.navigation_target(true, cx).is_some(),
                    "forward_enabled": w.navigation_target(false, cx).is_some(),
                    "back_label": w.navigation_label(true, cx),
                    "forward_label": w.navigation_label(false, cx),
                    "palette": w.shell.palette.is_some(),
                    "shortcuts": w.shell.shortcuts.is_some(),
                    "settings": w.settings.read(cx).open,
                    "settings_category": w.settings.read(cx).category().label(),
                    "settings_theme_choice": w.settings.read(cx).selected_theme(),
                    "appearance_in_view": w.settings.read(cx).appearance_in_view(),
                    "settings_scroll_cues": w.settings.read(cx).open.then(|| w.settings.read(cx).scroll_cues()),
                    "theme_choice": match w.theme_choice {
                        ThemeChoice::System => "system",
                        ThemeChoice::Light => "light",
                        ThemeChoice::Dark => "dark",
                    },
                    "dictionary": w.settings.read(cx).dictionary.is_some(),
                    "case_list": w.cases.list_open,
                    "case_page": w.cases.page.is_some(),
                    "case_page_locked": w.cases.page.as_ref().is_some_and(|page| w.case_coordinator.case(page.id()).is_none()),
                    "case_page_documents": w.cases.page.as_ref().and_then(|page| w.case_coordinator.case(page.id())).map(|case| case.members.len()),
                    "case_page_names": w.cases.page.as_ref().and_then(|page| w.case_coordinator.case(page.id())).map(|case| case.exported().len()),
                    "case_page_members": members,
                    "case_rename": w.cases.renaming.is_some(),
                    "case_categories": w.cases.page.as_ref().is_some_and(|page| page.categories()),
                    "keychain_explainer": w.cases.explainer,
                    "folder_sheet": w.folder_sheets.sheet().map(|s| s.primary_label()),
                    "folder_sheet_ticked": w.folder_sheets.sheet().map(|s| s.ticked_count()),
                    "find_replace": w.active_view().is_some_and(|v| v.read(cx).search.open()),
                    "reply_view": w.active_view().is_some_and(|v| v.read(cx).compare.as_ref().is_some_and(|c| c.mode == CompareMode::Reply)),
                    "reply_unresolved": w.active_view().map(|v| v.read(cx)).filter(|v| v.reply.exists()).map(|v| v.reply.restored().unresolved.len()),
                    "decision_history": w.active_view().is_some_and(|v| v.read(cx).history.open),
                    // Counts only: labels name originals, which What you
                    // did never writes to disk (ADR 0079).
                    "decisions": w.active_view().map(|v| {
                        use mdoc_pii::decisions::EntryState;
                        let v = v.read(cx);
                        let path = v.editor.read(cx).history_path();
                        let count = |wanted: EntryState| {
                            v.pii.decisions().rows(&path).filter(|(_, state)| *state == wanted).count()
                        };
                        json!({
                            "done": count(EntryState::Done),
                            "undone": count(EntryState::Undone),
                            "past_limit": count(EntryState::PastLimit),
                        })
                    }),
                    "merge_hints": w.active_view().map_or(0, |v| v.read(cx).pii.review().merge_hints().len()),
                    "scanning": w.active_view().is_some_and(|v| v.read(cx).pii.scanning()),
                    "ocr_consent": w.active_view().is_some_and(|v| {
                        let v = v.read(cx);
                        v.lifecycle.source_only() && v.import.ocr_required().is_some()
                    }),
                })
            })?
        } else if let Some(stage) = self.window.downcast::<shelf::Stage>() {
            self.cx
                .update(|cx| stage.read(cx).map(|stage| stage.state(cx)))
                .map_err(|e| e.to_string())?
        } else {
            json!({"model_comparison": true})
        };
        let viewport = self
            .cx
            .update_window(self.window, |_, window, _| window.viewport_size())
            .map_err(|e| e.to_string())?;
        let raster_ms = self.png(&out.join(format!("{name}.png")))?;
        let tab_dir = out.join(name);
        std::fs::create_dir(&tab_dir).map_err(|e| e.to_string())?;
        let mut seen = HashMap::new();
        let mut sequence = Vec::new();
        let mut cycle = None;
        for step in 0..MAX_TABS {
            let start = Instant::now();
            self.key("tab")?;
            let input_ms = start.elapsed().as_secs_f64() * 1000.;
            self.draw("tab")?;
            // Focus reveal defers the scroll until after prepaint. Let that
            // scroll and its resulting layout settle before rasterizing.
            self.settle()?;
            let (focus, contexts) = self
                .cx
                .update_window(self.window, |_, window, cx| {
                    (
                        window.focused(cx).map(|f| format!("{f:?}")),
                        format!("{:?}", window.context_stack()),
                    )
                })
                .map_err(|e| e.to_string())?;
            let file = format!("{name}/tab-{step:03}.png");
            let raster_ms = self.png(&out.join(&file))?;
            let first = seen.insert(focus.clone(), step);
            sequence.push(json!({"focus_id": focus, "key_contexts": contexts, "png": file, "input_ms": input_ms, "raster_readback_ms": raster_ms}));
            if let Some(first) = first {
                cycle = Some(first);
                break;
            }
        }
        self.records.push(
            json!({"screen": name, "state": state, "png": format!("{name}.png"), "raster_readback_ms": raster_ms,
            "viewport": [f32::from(viewport.width), f32::from(viewport.height)],
            "tab_order": sequence, "cycle_to": cycle, "truncated": cycle.is_none(), "frames": self.frames}),
        );
        Ok(())
    }
}

fn write_png(path: &Path, image: &image::RgbaImage) -> Result<()> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    image::codecs::png::PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| e.to_string())
}

/// Use the existing public annotation corpus directly, without a detector or
/// model download. Offsets are UTF-8 byte ranges in the unmarked document.
fn fixture() -> Result<(String, Vec<mdoc_pii::Detection>)> {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/pseudonymization/corpus.json"
    ))
    .map_err(|e| e.to_string())?;
    let annotated = corpus[0]["annotated"]
        .as_str()
        .ok_or("missing annotated fixture")?;
    let re = regex::Regex::new(r"⟦([^|]+)\|([^⟧]+)⟧").map_err(|e| e.to_string())?;
    let mut text = String::new();
    let mut detections = Vec::new();
    let mut end = 0;
    for m in re.captures_iter(annotated) {
        let whole = m.get(0).unwrap();
        text.push_str(&annotated[end..whole.start()]);
        let start = text.len();
        text.push_str(&m[2]);
        detections.push(mdoc_pii::Detection {
            range: start..text.len(),
            category: mdoc_pii::Category::ALL
                .into_iter()
                .find(|c| c.token() == &m[1])
                .ok_or("unknown fixture category")?,
            score: 1.,
            recognizer: mdoc_pii::Recognizer::Model,
        });
        end = whole.end();
    }
    text.push_str(&annotated[end..]);
    Ok((text, detections))
}

/// Every scene in order. Later scenes build on earlier state: the fixture
/// document, its review and its case.
fn scenarios(d: &mut Driver, path: &Path, detections: &[mdoc_pii::Detection]) -> Result<()> {
    let view = navigation::scenes(d, path)?;
    review::scenes(d, &view, path, detections)?;
    settings::scenes(d)?;
    shell::scenes(d)?;
    search::scenes(d, &view)?;
    review::before_after(d, &view)?;
    review::applied_alias(d, &view)?;
    restore::reply_view(d, &view)?;
    decisions::scenes(d, &view)?;
    let case = cases::scenes(d, path)?;
    folder_sheet::scenes(d, path, &case)?;
    cases::delete(d, case)?;
    confirm::scenes(d)?;
    comparison::scenes(d, &view)?;
    import::scenes(d)?;
    restore::restored_reply(d)?;
    comments::scenes(d)?;
    docx_source::scenes(d)?;
    review::merge_hint(d)?;
    review::long_counts(d)?;
    review::scanning(d)?;
    recents::scenes(d, path)?;
    narrow::scenes(d, &view, path.parent().ok_or("fixture folder")?)?;
    // Last: they leave cases locked.
    shelf::scenes(d)?;
    cases::locked_home(d)
}

pub(crate) fn run(out: &Path) -> Result<()> {
    // Refuse existing output, so stale pictures cannot masquerade as a successful run.
    std::fs::create_dir(out).map_err(|e| format!("create {}: {e}", out.display()))?;
    let profile = tempfile::tempdir().map_err(|e| e.to_string())?;
    mdoc_store::app_paths::isolate(profile.path().to_path_buf()).map_err(|e| e.to_string())?;
    let data_directory = mdoc_store::app_paths::data_local_dir().ok_or("missing data directory")?;
    let cache_directory = mdoc_store::app_paths::cache_dir().ok_or("missing cache directory")?;
    if !data_directory.starts_with(profile.path()) || !cache_directory.starts_with(profile.path()) {
        return Err("headless profile escaped temporary directory".into());
    }
    // Exercise both OS-key entry points before building app state. Both must
    // refuse without touching the OS key store or any profile file.
    let cases = data_directory.join("mdoc/cases");
    for result in [
        mdoc_store::cases::Store::open(cases.clone()).map(|_| ()),
        mdoc_store::cases::Store::open_quietly(cases).map(|_| ()),
    ] {
        if !matches!(result, Err(mdoc_store::cases::CaseError::KeysDisabled)) {
            return Err("OS case-key guard is not active".into());
        }
    }
    let network_probe = mdoc_import::model_download::fetch(
        "https://invalid.invalid/headless-network-probe",
        1,
        "",
        &profile.path().join("never-downloaded"),
        &mdoc_import::model_download::Progress::default(),
    );
    if !matches!(
        network_probe,
        Err(mdoc_import::model_download::DownloadError::NetworkDisabled)
    ) {
        return Err("network guard is not active".into());
    }
    #[cfg(target_os = "macos")]
    let text_system = gpui_platform::current_platform(true).text_system();
    #[cfg(target_os = "windows")]
    let text_system = gpui_windows::headless_text_system().map_err(|e| e.to_string())?;
    let (text, detections) = fixture()?;
    let path = profile.path().join("Services Agreement.md");
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    let mut screens = Vec::new();
    let mut focus_rings = Vec::new();
    for (theme, tag) in [(ThemeChoice::Light, "light"), (ThemeChoice::Dark, "dark")] {
        let mut cx = HeadlessAppContext::with_platform(text_system.clone(), Arc::new(()), || {
            #[cfg(target_os = "macos")]
            {
                gpui_platform::current_headless_renderer()
            }
            #[cfg(target_os = "windows")]
            {
                Some(Box::new(
                    gpui_windows::DirectXHeadlessRenderer::new().expect("WARP renderer"),
                ))
            }
        });
        cx.update(|cx| {
            cx.set_reduce_motion(true);
            bind_all_keys(cx);
            bind_app_actions(cx);
        });
        let window = cx
            .open_window(size(px(WIDTH), px(HEIGHT)), |window, cx| {
                cx.new(|cx| {
                    let mut workspace = Workspace::empty(window, cx);
                    let store = mdoc_store::cases::Store::with_key(
                        profile.path().join(tag).join("cases"),
                        &[3; 32],
                    );
                    let case = mdoc_store::cases::Case::new("Public fixture".into(), &[], None);
                    store.save(&case).expect("temporary fixture case");
                    workspace.use_case_store(store);
                    workspace.case_coordinator.finish_load(vec![case]);
                    workspace.restore(
                        Session {
                            theme_choice: theme,
                            ..Session::default()
                        },
                        window,
                        cx,
                    );
                    workspace.install_prompts(cx);
                    workspace
                })
            })
            .map_err(|e| e.to_string())?;
        let dir = out.join(tag);
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let mut d = Driver {
            cx,
            window: window.into(),
            out: dir,
            records: Vec::new(),
            frames: Vec::new(),
            focus_rings: Vec::new(),
            pngs: encoder::Encoder::new()?,
        };
        d.change(|w, window, cx| {
            w.theme_choice = theme;
            w.apply_theme(window, cx);
            w.show_home(window, cx);
            window.focus(w.focus_regions.home(), cx);
        })?;
        scenarios(&mut d, &path, &detections)?;
        for mut record in d.records.drain(..) {
            record["theme"] = json!(tag);
            screens.push(record);
        }
        for mut record in d.focus_rings.drain(..) {
            record["theme"] = json!(tag);
            focus_rings.push(record);
        }
        // No summary can pass while an encoder is still writing or has failed.
        d.pngs.finish()?;
    }
    // DOCX fixture previews remove their backing directories off-thread.
    // Both TestWindow contexts have dropped before this cleanup barrier.
    mdoc_import::docx_preview::shutdown_workers();
    let renderer = if cfg!(target_os = "windows") {
        "headless-directx-warp"
    } else {
        "headless-metal"
    };
    let summary = json!({"schema_version": 1, "platform": std::env::consts::OS, "renderer": renderer, "viewport": [WIDTH, HEIGHT],
        "profile_directory": profile.path(), "screens": screens, "focus_rings": focus_rings,
        "isolation": {"data_directory": data_directory, "cache_directory": cache_directory,
            "os_case_keys_disabled": true, "network_requests_disabled": true},
        "timing_scope": "CPU layout/paint and synchronous GPU raster/readback measured separately; PNG encoding excluded",
        "tab_scope": "Actual Tab dispatch until first repeated focus handle, or 128 presses. Focus IDs are run-local; each step includes a PNG with the focus ring. A cycle can be a local panel loop, not whole-window coverage."});
    std::fs::write(
        out.join("summary.json"),
        serde_json::to_vec_pretty(&summary).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Headless renders: {}", out.display());
    Ok(())
}
