//! Dev-only screen evidence. TestPlatform owns every window: no NSWindow,
//! application event loop, activation, clipboard or OS prompt is used.
use super::*;
use gpui::{HeadlessAppContext, Keystroke};
use image::ImageEncoder;
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path, time::Instant};

type Result<T> = std::result::Result<T, String>;
const WIDTH: f32 = 1100.;
const HEIGHT: f32 = 760.;
const MAX_TABS: usize = 128;

struct Driver {
    cx: HeadlessAppContext,
    window: gpui::AnyWindowHandle,
    frames: Vec<Value>,
}

impl Driver {
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
        .map_err(|e| e.to_string())?;
        Ok(ms)
    }

    fn capture(&mut self, out: &Path, name: &str) -> Result<Value> {
        eprintln!(
            "Rendering {}/{name}",
            out.file_name().unwrap().to_string_lossy()
        );
        self.frames.clear();
        self.settle()?;
        let state = if self.window.downcast::<Workspace>().is_some() {
            self.change(|w, _, cx| {
                json!({
                    "home": w.showing_home(cx),
                    "palette": w.shell.palette.is_some(),
                    "settings": w.settings.read(cx).open,
                    "dictionary": w.settings.read(cx).dictionary.is_some(),
                    "case_list": w.cases.list_open,
                    "case_rename": w.cases.renaming.is_some(),
                    "case_categories": w.cases.categories_open.is_some(),
                    "keychain_explainer": w.cases.explainer,
                    "find_replace": w.active_view().is_some_and(|v| v.read(cx).search.open),
                })
            })?
        } else {
            json!({"model_comparison": true})
        };
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
        Ok(
            json!({"screen": name, "state": state, "png": format!("{name}.png"), "raster_readback_ms": raster_ms,
            "tab_order": sequence, "cycle_to": cycle, "truncated": cycle.is_none(), "frames": self.frames}),
        )
    }
}

/// Use the existing public annotation corpus directly, without a detector or
/// model download. Offsets are UTF-8 byte ranges in the unmarked document.
fn fixture() -> Result<(String, Vec<pii::Detection>)> {
    let corpus: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/pseudonymization/corpus.json"
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
        detections.push(pii::Detection {
            range: start..text.len(),
            category: pii::Category::ALL
                .into_iter()
                .find(|c| c.token() == &m[1])
                .ok_or("unknown fixture category")?,
            score: 1.,
            recognizer: pii::Recognizer::Model,
        });
        end = whole.end();
    }
    text.push_str(&annotated[end..]);
    Ok((text, detections))
}

fn document(d: &mut Driver, path: &Path) -> Result<Entity<DocumentView>> {
    let document = Document::open(path.to_path_buf()).map_err(|e| e.to_string())?;
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        let view = w.active_view().unwrap();
        view.update(cx, |v, cx| {
            v.editor
                .update(cx, |e, cx| e.set_text(document.saved.clone(), cx));
            v.session.replace(document);
            v.blank_disposable = false;
        });
        window.focus(&w.focus, cx);
        view
    })
}

fn scenarios(
    d: &mut Driver,
    out: &Path,
    path: &Path,
    detections: &[pii::Detection],
) -> Result<Vec<Value>> {
    let mut records = vec![d.capture(out, "home")?];
    let view = document(d, path)?;
    records.push(d.capture(out, "document")?);
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            // A Tab in the editor edits text. Restore the fixture before review.
            let text = std::fs::read_to_string(path).expect("copied fixture");
            v.editor.update(cx, |e, cx| e.set_text(text.clone(), cx));
            v.pii
                .review
                .ingest(&text, detections.to_vec())
                .expect("fixture review");
            v.review_changed(cx);
            v.toggle_replacements_panel(window, cx);
        })
    })?;
    records.push(d.capture(out, "review")?);
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            let variant = &v.pii.review.variants()[0];
            let id = v
                .pii
                .review
                .annotation_id(variant.id, &variant.mentions[0])
                .unwrap();
            v.activate_annotation(id, window, cx);
        })
    })?;
    records.push(d.capture(out, "mention-popup")?);
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| v.close_pii_popup(&PiiClosePopup, window, cx))
    })?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.pii.setup = true;
            window.focus(&v.replacements_focus(), cx);
            cx.notify();
        })
    })?;
    records.push(d.capture(out, "setup-consent")?);
    d.change(|_, _, cx| {
        view.update(cx, |v, cx| {
            v.pii.setup = false;
            cx.notify();
        })
    })?;
    d.change(|w, window, cx| {
        // show_settings activates native windows. Call the panel's state flow
        // directly, even though TestPlatform cannot show a native window.
        w.settings.update(cx, |p, cx| p.show(window, cx));
    })?;
    records.push(d.capture(out, "settings")?);
    d.change(|w, window, cx| w.settings.update(cx, |p, cx| p.open_dictionary(window, cx)))?;
    records.push(d.capture(out, "dictionary")?);
    d.key("escape")?;
    d.change(|w, window, cx| w.settings.update(cx, |p, cx| p.close(window, cx)))?;
    d.change(|w, window, cx| w.open_palette(window, cx))?;
    records.push(d.capture(out, "command-palette")?);
    // Tab may leave a popover's key context. Reset state explicitly between
    // scenes rather than assuming Escape still reaches the previous overlay.
    d.change(|w, window, cx| w.close_palette(window, cx))?;
    d.change(|_, window, cx| view.update(cx, |v, cx| v.open_markdown_search(true, window, cx)))?;
    records.push(d.capture(out, "find-replace")?);
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.close_markdown_search(&CloseMarkdownSearch, window, cx)
        })
    })?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.apply_all_pii(&PiiApplyAll, window, cx);
            v.toggle_compare(window, cx);
        })
    })?;
    records.push(d.capture(out, "before-after")?);
    d.change(|_, window, cx| view.update(cx, |v, cx| v.close_compare(window, cx)))?;
    d.change(|w, window, cx| w.show_case_list(window, cx))?;
    records.push(d.capture(out, "open-case")?);
    d.change(|w, window, cx| w.close_case_list(window, cx))?;
    let id = d.change(|w, _, _| w.cases.cases[0].id.clone())?;
    d.change(|w, window, cx| w.start_rename_case(id.clone(), window, cx))?;
    records.push(d.capture(out, "rename-case")?);
    d.key("escape")?;
    d.change(|w, window, cx| {
        w.cases.categories_open = Some(id.clone());
        window.focus(&w.case_categories_focus, cx);
        cx.notify();
    })?;
    records.push(d.capture(out, "case-categories")?);
    d.change(|w, window, cx| w.close_case_categories(window, cx))?;
    d.change(|w, _, cx| {
        // Preview only. Continue is never invoked and no OS key is read.
        w.cases.explainer = true;
        w.explainer_focus_pending = true;
        cx.notify();
    })?;
    records.push(d.capture(out, "keychain-explainer")?);
    d.change(|w, window, cx| w.cancel_unlock(window, cx))?;
    d.change(|w, window, cx| w.delete_case(id, window, cx))?;
    records.push(d.capture(out, "confirm-delete-case")?);
    d.key("escape")?;
    d.change(|w, window, cx| w.close_tab(w.active, window, cx))?;
    records.push(d.capture(out, "confirm-keep-mapping")?);
    d.key("escape")?;
    d.change(|w, window, cx| {
        w.cases.asked.insert(w.active);
        w.close_tab(w.active, window, cx);
    })?;
    records.push(d.capture(out, "confirm-unsaved")?);
    d.key("escape")?;
    d.cx.update_window(d.window, |_, window, cx| {
        // Render the shared confirm component without starting a removal job.
        drop(window.prompt(
            PromptLevel::Warning,
            &format!(
                "Remove {} model files?",
                settings::Model::Pii(settings::PiiModel::Fp16).name()
            ),
            Some("Shared runtimes and documents are retained."),
            &["Remove", "Cancel"],
            cx,
        ));
    })
    .map_err(|e| e.to_string())?;
    records.push(d.capture(out, "confirm-remove-model")?);
    d.key("escape")?;
    // The comparison uses a second TestWindow, never a native window.
    let (owner, panel, theme, defaults, input) = d.change(|w, _, cx| {
        (
            cx.entity().downgrade(),
            w.settings.clone(),
            w.theme.clone(),
            w.preferences
                .borrow()
                .snapshot()
                .expect("temporary preferences"),
            comparison::Input::Markdown(Arc::new(view.read(cx).editor.read(cx).text().to_owned())),
        )
    })?;
    let hash = input.hash();
    let comparison =
        d.cx.open_window(size(px(WIDTH), px(HEIGHT)), |window, cx| {
            let view = cx.new(|cx| {
                comparison_ui::View::new((input, hash), defaults, theme, owner, panel, vec![], cx)
            });
            window.focus(&view.read(cx).focus_handle(cx), cx);
            view
        })
        .map_err(|e| e.to_string())?;
    let workspace_window = d.window;
    d.window = comparison.into();
    records.push(d.capture(out, "model-comparison")?);
    d.cx.update_window(d.window, |_, window, _| window.remove_window())
        .map_err(|e| e.to_string())?;
    d.window = workspace_window;
    let mapping: Vec<Value> = serde_json::from_str(include_str!(
        "../tests/fixtures/restore/claude-en/identities.json"
    ))
    .map_err(|e| e.to_string())?;
    let identities = mapping
        .iter()
        .map(|identity| {
            Ok(pii::restore::ExportedIdentity {
                alias: identity["alias"].as_str().ok_or("fixture alias")?.into(),
                previous: vec![],
                custom: false,
                category: pii::Category::ALL
                    .into_iter()
                    .find(|c| Some(c.token()) == identity["category"].as_str())
                    .ok_or("fixture category")?,
                originals: identity["originals"]
                    .as_array()
                    .ok_or("fixture originals")?
                    .iter()
                    .map(|v| v.as_str().map(Arc::from).ok_or("fixture original"))
                    .collect::<std::result::Result<_, _>>()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let reply = pii::ui::RestoredReply {
        text: include_str!("../tests/fixtures/restore/claude-en/reply.md").into(),
        identities,
        source_name: "Claude public EN fixture".into(),
        case: None,
    };
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view()
            .unwrap()
            .update(cx, |v, cx| v.start_restored(&reply, window, cx));
    })?;
    records.push(d.capture(out, "restored-reply")?);
    Ok(records)
}

pub(crate) fn run(out: &Path) -> Result<()> {
    // Refuse existing output, so stale pictures cannot masquerade as a successful run.
    std::fs::create_dir(out).map_err(|e| format!("create {}: {e}", out.display()))?;
    let profile = tempfile::tempdir().map_err(|e| e.to_string())?;
    crate::app_paths::isolate(profile.path().to_path_buf())?;
    let platform = gpui_platform::current_platform(true);
    let (text, detections) = fixture()?;
    let path = profile.path().join("Services Agreement.md");
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    let mut screens = Vec::new();
    for (theme, tag) in [
        (style::ThemeChoice::Light, "light"),
        (style::ThemeChoice::Dark, "dark"),
    ] {
        let mut cx = HeadlessAppContext::with_platform(
            platform.text_system(),
            Arc::new(()),
            gpui_platform::current_headless_renderer,
        );
        cx.update(|cx| {
            cx.set_reduce_motion(true);
            mdoc_editor::bind_keys(cx);
            markdown_search::bind_keys(cx);
            pii::ui::bind_keys(cx);
            bind_case_keys(cx);
            bind_shell_keys(cx);
            settings_ui::bind_keys(cx);
            ui::bind_keys(cx);
            bind_markdown_search_keys(cx);
            bind_app_keys(cx);
            bind_app_actions(cx);
            cx.bind_keys([KeyBinding::new(
                "escape",
                comparison_ui::CloseComparison,
                Some("ModelComparison"),
            )]);
        });
        let window = cx
            .open_window(size(px(WIDTH), px(HEIGHT)), |window, cx| {
                cx.new(|cx| {
                    let mut workspace = Workspace::empty(window, cx);
                    let store = crate::cases::Store::with_key(
                        profile.path().join(tag).join("cases"),
                        &[3; 32],
                    );
                    let case = crate::cases::Case::new("Public fixture".into(), &[], None);
                    store.save(&case).expect("temporary fixture case");
                    workspace.use_case_store(store);
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
        let mut d = Driver {
            cx,
            window: window.into(),
            frames: Vec::new(),
        };
        let dir = out.join(tag);
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        d.change(|w, window, cx| {
            w.theme_choice = theme;
            w.apply_theme(window, cx);
            w.show_home(window, cx);
            window.focus(&w.home_region, cx);
        })?;
        for mut record in scenarios(&mut d, &dir, &path, &detections)? {
            record["theme"] = json!(tag);
            screens.push(record);
        }
    }
    let summary = json!({"schema_version": 1, "platform": "macos", "renderer": "headless-metal", "viewport": [WIDTH, HEIGHT],
        "profile_directory": profile.path(), "screens": screens,
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
