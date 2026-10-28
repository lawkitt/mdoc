//! Dev-only screen evidence. TestPlatform owns every window: no native window,
//! application event loop, activation, clipboard or OS prompt is used.
#[cfg(target_os = "windows")]
mod encoder;

use super::{Workspace, app_actions::bind as bind_app_actions};
use crate::{
    bind_all_keys, comparison, comparison_ui,
    document_view::DocumentView,
    pii::ui::{PiiApplyAll, PiiClosePopup},
    search_session::CloseMarkdownSearch,
    settings_ui::Category,
};
use gpui::{
    Context, Entity, Focusable, HeadlessAppContext, Keystroke, PromptLevel, Window, prelude::*, px,
    size,
};
use image::ImageEncoder;
use mdoc_store::session_store::ThemeChoice;
use mdoc_store::{document::Document, session_store::Session, settings};
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
    frames: Vec<Value>,
    focus_rings: Vec<Value>,
    #[cfg(target_os = "windows")]
    pngs: encoder::Encoder,
}

impl Driver {
    /// Paired images of the same state: only keyboard focus changes. The
    /// contract test checks pixels, including that the control keeps its fill.
    fn focus_pair(&mut self, out: &Path, name: &str, target: gpui::FocusHandle) -> Result<()> {
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
        #[cfg(target_os = "windows")]
        self.pngs.write(path, image)?;
        #[cfg(target_os = "macos")]
        write_png(path, &image)?;
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
                    "shortcuts": w.shell.shortcuts.is_some(),
                    "settings": w.settings.read(cx).open,
                    "settings_category": w.settings.read(cx).category().label(),
                    "settings_theme_choice": w.settings.read(cx).selected_theme(),
                    "appearance_in_view": w.settings.read(cx).appearance_in_view(),
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
                    "case_rename": w.cases.renaming.is_some(),
                    "case_categories": w.cases.page.as_ref().is_some_and(|page| page.categories()),
                    "keychain_explainer": w.cases.explainer,
                    "folder_sheet": w.folder_sheets.sheet().map(|s| s.primary_label()),
                    "folder_sheet_ticked": w.folder_sheets.sheet().map(|s| s.ticked_count()),
                    "find_replace": w.active_view().is_some_and(|v| v.read(cx).search.open()),
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
        Ok(
            json!({"screen": name, "state": state, "png": format!("{name}.png"), "raster_readback_ms": raster_ms,
            "tab_order": sequence, "cycle_to": cycle, "truncated": cycle.is_none(), "frames": self.frames}),
        )
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

fn document(d: &mut Driver, path: &Path) -> Result<Entity<DocumentView>> {
    let document = Document::open(path.to_path_buf()).map_err(|e| e.to_string())?;
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        let view = w.active_view().unwrap();
        view.update(cx, |v, cx| {
            v.editor
                .update(cx, |e, cx| e.set_text(document.saved.clone(), cx));
            v.session.replace(document);
            v.lifecycle.keep_blank();
        });
        window.focus(&w.focus, cx);
        view
    })
}

/// The folder sheet (ADR 0075) for synthetic folders in the temporary
/// profile: a small one, one over the 20-file cap, and Add to a loaded case.
/// Each is cancelled: nothing opens and no key is read.
fn folder_sheets(
    d: &mut Driver,
    out: &Path,
    document: &Path,
    case: &str,
    records: &mut Vec<Value>,
) -> Result<()> {
    let root = document.parent().ok_or("fixture directory")?;
    let write = |folder: &Path, names: &[String]| -> Result<()> {
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        for name in names {
            std::fs::write(folder.join(name), format!("# {name}\n")).map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    let matter = root.join("Petrova v Northwind");
    write(
        &matter,
        &[
            "Engagement letter.md",
            "README.md",
            "Timeline.md",
            "Witness statement.md",
        ]
        .map(String::from),
    )?;
    write(&matter.join("drafts"), &[])?;
    write(&matter, &[".notes.md".into()])?;
    let exhibits = root.join("Exhibits");
    write(
        &exhibits,
        &(1..=24)
            .map(|n| format!("Exhibit {n:02}.md"))
            .collect::<Vec<_>>(),
    )?;
    for (name, folder, case) in [
        ("folder-sheet", &matter, None),
        ("folder-sheet-over-cap", &exhibits, None),
        ("folder-sheet-add-to-case", &matter, Some(case.to_string())),
    ] {
        let folder = super::folder_sheet::DroppedFolder::read(folder, |p| {
            mdoc_store::document::is_markdown(p)
        })
        .ok_or("fixture folder")?;
        d.change(|w, window, cx| {
            let pending = super::folder_sheet::Pending {
                folder,
                case,
                at: None,
            };
            w.queue_folder(pending, window, cx)
        })?;
        records.push(d.capture(out, name)?);
        d.key("escape")?;
    }
    Ok(())
}

fn scenarios(
    d: &mut Driver,
    out: &Path,
    path: &Path,
    detections: &[mdoc_pii::Detection],
) -> Result<Vec<Value>> {
    let mut records = vec![d.capture(out, "home")?];
    let view = document(d, path)?;
    records.push(d.capture(out, "document")?);
    // Nothing is pseudonymized yet: Copy for AI says why it is off.
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.more_open = true;
            window.focus(&v.more_menu_focus, cx);
            cx.notify();
        })
    })?;
    records.push(d.capture(out, "more-menu")?);
    d.change(|_, window, cx| view.update(cx, |v, cx| v.close_more(window, cx)))?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            // A Tab in the editor edits text. Restore the fixture before review.
            let text = std::fs::read_to_string(path).expect("copied fixture");
            v.editor.update(cx, |e, cx| e.set_text(text.clone(), cx));
            v.pii
                .review_mut()
                .ingest(&text, detections.to_vec())
                .expect("fixture review");
            v.review_changed(cx);
            v.toggle_replacements_panel(window, cx);
        })
    })?;
    records.push(d.capture(out, "review")?);
    d.change(|_, window, cx| {
        let region = view.read(cx).toolbar_region.clone();
        window.focus(&region, cx);
    })?;
    d.key("tab")?;
    let toggle = d.change(|_, window, cx| {
        assert!(view.read(cx).replacements_open());
        window.focused(cx).expect("first toolbar control")
    })?;
    d.focus_pair(out, "toggle-on", toggle)?;
    let sidebar = d.change(|w, _, _| {
        w.sidebar_row_focus
            .borrow()
            .get(&w.tabs.active())
            .expect("selected sidebar row")
            .clone()
    })?;
    d.focus_pair(out, "sidebar-row", sidebar)?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.inspector_share = true;
            window.focus(&v.replacements_focus(), cx);
            cx.notify();
        });
    })?;
    d.settle()?;
    d.key("tab")?;
    let review_tab = d.change(|_, window, cx| window.focused(cx).expect("Review tab"))?;
    d.focus_pair(out, "beside-selected-tab", review_tab)?;
    d.change(|_, _, cx| {
        view.update(cx, |v, cx| {
            v.inspector_share = false;
            cx.notify();
        })
    })?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            let variant = &v.pii.review().variants()[0];
            let id = v
                .pii
                .review()
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
            v.pii.show_setup(true);
            window.focus(&v.replacements_focus(), cx);
            cx.notify();
        })
    })?;
    records.push(d.capture(out, "setup-consent")?);
    d.change(|_, _, cx| {
        view.update(cx, |v, cx| {
            v.preferences
                .borrow_mut()
                .current
                .as_mut()
                .expect("sandbox preferences")
                .pseudonymization
                .model = settings::PiiModel::Fp32;
            cx.notify();
        })
    })?;
    records.push(d.capture(out, "setup-consent-fp32")?);
    d.change(|_, _, cx| {
        view.update(cx, |v, cx| {
            v.preferences
                .borrow_mut()
                .current
                .as_mut()
                .expect("sandbox preferences")
                .pseudonymization
                .model = settings::PiiModel::Fp16;
            v.pii.show_setup(false);
            cx.notify();
        })
    })?;
    // show_settings activates native windows. Call the panel's state flow
    // directly, even though TestPlatform cannot show a native window.
    for (category, name) in [
        (Category::General, "settings-general"),
        (Category::Detection, "settings-detection"),
        (Category::Recognition, "settings-recognition"),
        (Category::Spelling, "settings-spelling"),
        (Category::Cases, "settings-cases"),
        (Category::About, "settings-about"),
    ] {
        d.change(|w, window, cx| {
            w.settings
                .update(cx, |p, cx| p.show_category(category, window, cx));
        })?;
        // Each baseline precedes its Tab walk, including Appearance in General.
        records.push(d.capture(out, name)?);
    }
    // Detection starts on its category tab; one Tab enters the first model radio.
    d.change(|w, window, cx| {
        w.settings
            .update(cx, |p, cx| p.show_category(Category::Detection, window, cx));
    })?;
    d.settle()?;
    d.key("tab")?;
    d.settle()?;
    let row = d.change(|_, window, cx| window.focused(cx).expect("first Settings row"))?;
    d.focus_pair(out, "settings-row", row)?;
    d.change(|w, window, cx| w.settings.update(cx, |p, cx| p.open_dictionary(window, cx)))?;
    records.push(d.capture(out, "dictionary")?);
    d.key("escape")?;
    d.change(|w, window, cx| w.settings.update(cx, |p, cx| p.close(window, cx)))?;
    d.change(|w, window, cx| w.open_palette(window, cx))?;
    records.push(d.capture(out, "command-palette")?);
    // Tab may leave a popover's key context. Reset state explicitly between
    // scenes rather than assuming Escape still reaches the previous overlay.
    d.change(|w, window, cx| w.close_palette(window, cx))?;
    // The keyboard shortcuts sheet, opened by its key as people do (ADR 0073).
    d.key(if cfg!(target_os = "macos") {
        "cmd-/"
    } else {
        "ctrl-/"
    })?;
    records.push(d.capture(out, "keyboard-shortcuts")?);
    d.change(|w, window, cx| w.close_shortcuts(window, cx))?;
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
    let id = d.change(|w, _, _| w.case_coordinator.cases()[0].id.clone())?;
    d.change(|w, window, cx| {
        w.join_case(w.tabs.active(), &id, window, cx);
        let closed = path.with_file_name("Witness statement.md");
        std::fs::write(&closed, "# Public witness statement\n").expect("temporary fixture");
        let (_, effect) = w
            .case_coordinator
            .edit(&id, |case| {
                case.created = 1791763200;
                case.members
                    .extend([closed, path.with_file_name("Missing exhibit.md")]);
                Some(())
            })
            .unwrap();
        w.carry_out_cases(effect.into_iter().collect(), cx);
        w.open_case(&id, window, cx);
    })?;
    records.push(d.capture(out, "case-page")?);
    // Leave only the cached public name/count, exactly as on a locked launch.
    let unlocked = d.change(|w, window, cx| {
        let case = w.case_coordinator.case(&id).unwrap().clone();
        w.case_coordinator
            .restore_summaries(vec![mdoc_store::cases::CaseSummary::of(&case)]);
        w.case_coordinator.hide_for_locked_render();
        w.open_case(&id, window, cx);
        case
    })?;
    records.push(d.capture(out, "case-page-locked")?);
    d.change(|w, window, cx| {
        w.case_coordinator.finish_load(vec![unlocked]);
        w.show_home(window, cx);
        w.activate(w.tabs.active(), window, cx);
        w.open_case(&id, window, cx);
    })?;
    d.change(|w, window, cx| w.start_rename_case(id.clone(), window, cx))?;
    records.push(d.capture(out, "rename-case")?);
    d.key("escape")?;
    d.change(|w, window, cx| {
        w.show_case_page(&id, super::case_page::Place::Categories, window, cx);
    })?;
    records.push(d.capture(out, "case-categories")?);
    d.change(|w, window, cx| w.activate(w.tabs.active(), window, cx))?;
    d.change(|w, _, cx| {
        // Preview only. Continue is never invoked and no OS key is read.
        w.cases.explainer = true;
        w.explainer_focus_pending = true;
        cx.notify();
    })?;
    records.push(d.capture(out, "keychain-explainer")?);
    d.change(|w, window, cx| w.cancel_unlock(window, cx))?;
    folder_sheets(d, out, path, &id, &mut records)?;
    d.change(|w, window, cx| w.delete_case(id, window, cx))?;
    records.push(d.capture(out, "confirm-delete-case")?);
    d.key("escape")?;
    d.change(|w, window, cx| w.close_tab(w.tabs.active(), window, cx))?;
    records.push(d.capture(out, "confirm-keep-mapping")?);
    d.key("escape")?;
    d.change(|w, window, cx| {
        w.case_coordinator.note_asked(w.tabs.active());
        w.close_tab(w.tabs.active(), window, cx);
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
        "../../tests/fixtures/restore/claude-en/identities.json"
    ))
    .map_err(|e| e.to_string())?;
    let identities = mapping
        .iter()
        .map(|identity| {
            Ok(mdoc_pii::restore::ExportedIdentity {
                alias: identity["alias"].as_str().ok_or("fixture alias")?.into(),
                previous: vec![],
                custom: false,
                category: mdoc_pii::Category::ALL
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
    let reply = crate::pii::ui::RestoredReply {
        text: include_str!("../../tests/fixtures/restore/claude-en/reply.md").into(),
        identities,
        source_name: "Claude public EN fixture".into(),
        case: None,
    };
    // A damaged DOCX: the conversion and preview failures as their panes
    // state them, set directly so no converter or worker runs.
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            let source = PathBuf::from("Damaged agreement.docx");
            v.lifecycle.show_source();
            v.preview.show_failure(
                source,
                crate::preview::docx_message(&mdoc_import::docx_preview::PreviewError::Package(
                    "invalid ZIP".into(),
                ))
                .to_string(),
            );
            v.error = Some(
                "This document could not be converted because its contents are damaged or incomplete."
                    .into(),
            );
            cx.notify();
        });
    })?;
    records.push(d.capture(out, "conversion-failed")?);
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view()
            .unwrap()
            .update(cx, |v, cx| v.start_restored(&reply, window, cx));
    })?;
    records.push(d.capture(out, "restored-reply")?);
    // The public DOCX comments fixture supplies the actual Original pane and
    // its full-width comments toggle, including the content card's clip.
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-preview/comments.docx");
    let preview = mdoc_import::docx_preview::render_in_process(
        &source,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .map_err(|e| e.to_string())?;
    let comments_view = d.change(|w, window, cx| {
        w.new_tab(window, cx);
        let view = w.active_view().unwrap();
        view.update(cx, |v, cx| {
            v.begin_preview(source);
            v.load_preview(preview.pdf_path.clone(), Some(preview), window, cx);
        });
        view
    })?;
    d.settle()?;
    d.change(|_, window, cx| {
        let view = comments_view.read(cx);
        assert!(
            view.preview.comment_panel().is_some(),
            "comments fixture loaded"
        );
        let region = view.comments_region.clone();
        window.focus(&region, cx);
    })?;
    d.key("tab")?;
    let toggle = d.change(|_, window, cx| window.focused(cx).expect("comments toggle"))?;
    d.focus_pair(out, "comments-toggle", toggle)?;
    records.push(d.capture(out, "comments")?);
    Ok(records)
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
            frames: Vec::new(),
            focus_rings: Vec::new(),
            #[cfg(target_os = "windows")]
            pngs: encoder::Encoder::new()?,
        };
        d.change(|w, window, cx| {
            w.theme_choice = theme;
            w.apply_theme(window, cx);
            w.show_home(window, cx);
            window.focus(w.focus_regions.home(), cx);
        })?;
        for mut record in scenarios(&mut d, &dir, &path, &detections)? {
            record["theme"] = json!(tag);
            screens.push(record);
        }
        for mut record in d.focus_rings.drain(..) {
            record["theme"] = json!(tag);
            focus_rings.push(record);
        }
        // No summary can pass while an encoder is still writing or has failed.
        #[cfg(target_os = "windows")]
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
