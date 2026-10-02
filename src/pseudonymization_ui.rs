use crate::{
    pseudonymization::{Category, Review},
    pseudonymization_detector as detector, *,
};
use gpui::{AnyElement, FocusHandle, Hsla, Pixels, Point, anchored, deferred};
use std::ops::Range;

pub(super) struct Popup {
    pub group: u64,
    pub mention: Range<usize>,
    pub all: bool,
}
struct ScanJob {
    cancel: Arc<AtomicBool>,
    revision: u64,
    generation: u64,
    config: settings::PiiConfig,
}
pub(super) struct ReviewUi {
    pub review: Review,
    pub popup: Option<Popup>,
    pub input: Entity<markdown_search::SearchInput>,
    pub focus: FocusHandle,
    pub category: Category,
    job: Option<ScanJob>,
    pub installing: bool,
    model_ready: bool,
    pub scans: Vec<settings::PiiConfig>,
    pub error: Option<String>,
    generation: u64,
}
impl Drop for ReviewUi {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl ReviewUi {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        Self {
            review: Review::default(),
            popup: None,
            input: cx.new(|cx| {
                markdown_search::SearchInput::new(cx).with_key_context("PseudonymReplacement")
            }),
            focus: cx.focus_handle(),
            category: Category::Person,
            job: None,
            installing: false,
            model_ready: false,
            scans: Vec::new(),
            error: None,
            generation: 0,
        }
    }
    fn cancel(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.generation = self.generation.wrapping_add(1);
    }
    fn scanning(&self) -> bool {
        self.job.is_some()
    }
}

pub(super) fn bind_keys(cx: &mut App) {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.bind_keys([
        KeyBinding::new(&format!("{modifier}-shift-p"), Pseudonymize, None),
        KeyBinding::new("alt-enter", ReviewCandidate, Some("Editor")),
        KeyBinding::new("alt-down", NextCandidate, Some("Editor")),
        KeyBinding::new("alt-up", PreviousCandidate, Some("Editor")),
        KeyBinding::new(
            &format!("{modifier}-alt-p"),
            AddPseudonymCandidate,
            Some("Editor"),
        ),
        KeyBinding::new("enter", AcceptPseudonymCandidate, Some("PseudonymReview")),
        KeyBinding::new("alt-k", KeepPseudonymCandidate, Some("PseudonymReview")),
        KeyBinding::new("escape", ClosePseudonymPopup, Some("PseudonymReview")),
    ]);
}

impl Workspace {
    pub(super) fn reset_pseudonymization(&mut self, cx: &mut Context<Self>) {
        self.pseudonymization.cancel();
        self.pseudonymization.review = Review::default();
        self.pseudonymization.scans.clear();
        self.pseudonymization.popup = None;
        self.pseudonymization.error = None;
        self.sync_annotations(cx);
    }
    pub(super) fn pseudonymization_edited(&mut self, cx: &mut Context<Self>) {
        if !self.pseudonymization.review.open
            && self.pseudonymization.review.groups.is_empty()
            && !self.pseudonymization.scanning()
        {
            return;
        }
        let was_scanning = self.pseudonymization.scanning();
        self.pseudonymization.cancel();
        self.pseudonymization.popup = None;
        if was_scanning {
            self.pseudonymization.error =
                Some("Document changed; scan cancelled. Rescan when ready.".into());
        }
        self.pseudonymization
            .review
            .refresh(self.editor.read(cx).text());
        self.sync_annotations(cx);
    }
    pub(super) fn sync_pseudonym_theme(&mut self, cx: &mut Context<Self>) {
        self.sync_annotations(cx);
    }
    fn sync_annotations(&mut self, cx: &mut Context<Self>) {
        let accent = style::markdown_style(self.theme.get()).alert_warning;
        let annotations = if self.pseudonymization.review.open {
            self.pseudonymization
                .review
                .groups
                .iter()
                .flat_map(|group| {
                    group
                        .mentions
                        .iter()
                        .enumerate()
                        .map(move |(index, range)| mdoc_editor::SourceAnnotation {
                            id: (group.id << 32) | index as u64,
                            range: range.clone(),
                            color: Hsla { a: 0.14, ..accent },
                            active_color: Hsla { a: 0.3, ..accent },
                        })
                })
                .collect()
        } else {
            Vec::new()
        };
        self.editor.update(cx, |editor, cx| {
            editor.set_annotations(editor.revision(), annotations, cx)
        });
    }
    pub(super) fn pseudonymize(
        &mut self,
        _: &Pseudonymize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy_markdown() {
            return;
        }
        self.pseudonymization.review.open = true;
        self.pseudonymization
            .review
            .refresh(self.editor.read(cx).text());
        self.sync_annotations(cx);
        // Setup is a separate explicit network action. Pseudonymize attempts an
        // offline scan; a missing install leaves manual review fully available.
        self.scan_pseudonyms(cx);
        cx.notify();
    }
    pub(super) fn scan_pseudonyms(&mut self, cx: &mut Context<Self>) {
        if !self.can_copy_markdown() || self.pseudonymization.installing {
            return;
        }
        self.pseudonymization.cancel();
        self.pseudonymization.error = None;
        self.pseudonymization.popup = None;
        self.pseudonymization.review.open = true;
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        let source = editor.text().to_owned();
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.pseudonymization,
            Err(e) => {
                self.pseudonymization.error = Some(e);
                cx.notify();
                return;
            }
        };
        let generation = self.pseudonymization.generation;
        let identity = self.session.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.pseudonymization.job = Some(ScanJob {
            cancel: cancel.clone(),
            revision,
            generation,
            config: config.clone(),
        });
        let task = cx
            .background_executor()
            .spawn(async move { detector::scan_config(&source, &cancel, &config) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.complete_pseudonym_scan(generation, identity, revision, result, cx)
            });
        })
        .detach();
        cx.notify();
    }
    fn complete_pseudonym_scan(
        &mut self,
        generation: u64,
        identity: u64,
        revision: u64,
        result: Result<Vec<pseudonymization::Detection>, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(job) = &self.pseudonymization.job else {
            return;
        };
        if job.generation != generation
            || job.revision != revision
            || self.session.generation != identity
            || self.editor.read(cx).revision() != revision
            || job.cancel.load(Ordering::Relaxed)
        {
            return;
        }
        let config = job.config.clone();
        self.pseudonymization.job = None;
        let source = self.editor.read(cx).text().to_owned();
        match result.and_then(|detections| self.pseudonymization.review.ingest(&source, detections))
        {
            Ok(()) => {
                self.pseudonymization.error = None;
                self.pseudonymization.model_ready = true;
                self.pseudonymization.scans.push(config);
                self.sync_annotations(cx);
            }
            Err(error) => {
                if error.contains("Set up") {
                    self.pseudonymization.model_ready = false;
                }
                self.pseudonymization.error = Some(error);
            }
        }
        cx.notify();
    }

    fn setup_pseudonyms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pseudonymization.installing || !detector::SUPPORTED {
            return;
        }
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.pseudonymization,
            Err(e) => {
                self.pseudonymization.error = Some(e);
                cx.notify();
                return;
            }
        };
        if let Some(panel) = self.model_panel.clone() {
            panel.update(cx, |panel, cx| {
                panel.setup(settings::Model::Pii(config.model), false, cx);
                panel.show(window, cx);
            });
            return;
        }
        self.pseudonymization.cancel();
        self.pseudonymization.installing = true;
        self.pseudonymization.error = None;
        let task = cx.background_executor().spawn(async move {
            detector::setup_config(&config, &model_download::Progress::default())
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.pseudonymization.installing = false;
                this.pseudonymization.model_ready = result.is_ok();
                this.pseudonymization.error = result.err();
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn leave_pseudonyms(&mut self, cx: &mut Context<Self>) {
        self.pseudonymization.cancel();
        self.pseudonymization.review.open = false;
        self.pseudonymization.popup = None;
        self.sync_annotations(cx);
        cx.notify();
    }
    pub(super) fn activate_annotation(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.pseudonymization.review.open {
            return;
        }
        let group_id = id >> 32;
        let index = (id & u32::MAX as u64) as usize;
        let Some(group) = self.pseudonymization.review.group(group_id) else {
            return;
        };
        let Some(mention) = group.mentions.get(index).cloned() else {
            return;
        };
        let replacement = group.replacement.clone();
        self.pseudonymization.error = None;
        self.pseudonymization.popup = Some(Popup {
            group: group_id,
            mention,
            all: true,
        });
        self.pseudonymization
            .input
            .update(cx, |input, cx| input.set_value(replacement, cx));
        window.focus(&self.pseudonymization.input.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    pub(super) fn step_pseudonym(
        &mut self,
        backwards: bool,
        at_caret: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.pseudonymization.review.open {
            return;
        }
        let cursor = self.editor.read(cx).cursor();
        let mut mentions: Vec<_> = self
            .pseudonymization
            .review
            .groups
            .iter()
            .flat_map(|group| {
                group
                    .mentions
                    .iter()
                    .enumerate()
                    .map(move |(index, range)| ((group.id << 32) | index as u64, range.clone()))
            })
            .collect();
        mentions.sort_by_key(|(_, range)| range.start);
        let choice = if at_caret {
            mentions
                .iter()
                .find(|(_, range)| range.start <= cursor && cursor < range.end)
        } else if backwards {
            mentions
                .iter()
                .rev()
                .find(|(_, range)| range.start < cursor)
                .or(mentions.last())
        } else {
            mentions
                .iter()
                .find(|(_, range)| range.start > cursor)
                .or(mentions.first())
        };
        if let Some((id, range)) = choice {
            let (id, offset) = (*id, range.start);
            self.editor
                .update(cx, |editor, cx| editor.set_cursor(offset, cx));
            if let Some(top) = self.editor.read(cx).offset_screen_top(offset) {
                let viewport = self.scroll.bounds();
                let delta = top - viewport.top() - px(24.);
                let mut scroll = self.scroll.offset();
                scroll.y = (scroll.y - delta).clamp(-self.scroll.max_offset().y, px(0.));
                self.scroll.set_offset(scroll);
            }
            self.activate_annotation(id, window, cx);
            let weak = cx.entity().downgrade();
            window.on_next_frame(move |window, _| {
                window.on_next_frame(move |_, cx| {
                    let _ = weak.update(cx, |_, cx| cx.notify());
                });
            });
        }
    }
    pub(super) fn add_pseudonym(
        &mut self,
        _: &AddPseudonymCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy_markdown() {
            return;
        }
        let editor = self.editor.read(cx);
        let source = editor.text().to_owned();
        let range = editor.selection();
        self.pseudonymization.review.open = true;
        match self.pseudonymization.review.add_manual(
            &source,
            range.clone(),
            self.pseudonymization.category,
        ) {
            Ok(id) => {
                self.pseudonymization.error = None;
                self.sync_annotations(cx);
                let index = self
                    .pseudonymization
                    .review
                    .group(id)
                    .unwrap()
                    .mentions
                    .iter()
                    .position(|mention| *mention == range)
                    .unwrap_or(0);
                self.activate_annotation((id << 32) | index as u64, window, cx);
            }
            Err(error) => self.pseudonymization.error = Some(error),
        }
        cx.notify();
    }
    pub(super) fn accept_pseudonym(
        &mut self,
        _: &AcceptPseudonymCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = &self.pseudonymization.popup else {
            return;
        };
        let id = popup.group;
        let single = (!popup.all).then(|| popup.mention.clone());
        let replacement = self.pseudonymization.input.read(cx).value().to_owned();
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        let plan = self
            .pseudonymization
            .review
            .plan(editor.text(), id, single, &replacement);
        match plan {
            Ok(edits) => {
                if self
                    .editor
                    .update(cx, |editor, cx| editor.replace_ranges(revision, &edits, cx))
                {
                    self.pseudonymization
                        .review
                        .set_replacement(id, &replacement);
                    self.pseudonymization_edited(cx);
                    self.pseudonymization.error = None;
                    window.focus(&self.editor.read(cx).focus_handle(cx), cx);
                }
            }
            Err(error) => self.pseudonymization.error = Some(error),
        }
        cx.notify();
    }
    pub(super) fn keep_pseudonym(
        &mut self,
        _: &KeepPseudonymCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.pseudonymization.popup.take() else {
            return;
        };
        self.pseudonymization
            .review
            .keep(popup.group, (!popup.all).then_some(popup.mention));
        self.sync_annotations(cx);
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    pub(super) fn close_pseudonym_popup(
        &mut self,
        _: &ClosePseudonymPopup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pseudonymization.popup = None;
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    pub(super) fn pseudonym_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.pseudonymization.review.open || !self.can_copy_markdown() {
            return None;
        }
        let palette = self.theme.get().pdf_style();
        let selected = self
            .preferences
            .borrow()
            .snapshot()
            .map(|p| settings::Model::Pii(p.pseudonymization.model));
        let installing = self.pseudonymization.installing
            || self.model_panel.as_ref().is_some_and(|p| {
                selected
                    .as_ref()
                    .is_ok_and(|m| p.read(cx).working == Some(*m))
            });
        let model_ready =
            self.model_panel
                .as_ref()
                .map_or(self.pseudonymization.model_ready, |p| {
                    selected.as_ref().is_ok_and(|m| {
                        matches!(
                            p.read(cx).statuses
                                [settings::Model::ALL.iter().position(|v| v == m).unwrap()],
                            settings_ui::Status::Ready
                        )
                    })
                });
        Some(div().flex().flex_col().gap_1().px_3().py_2().text_size(px(12.)).border_b_1().border_color(palette.border)
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(format!("Pseudonymize · {} candidates", self.pseudonymization.review.remaining()))
                .child(if installing { "Downloading model…" } else if self.pseudonymization.scanning() { "Scanning…" } else { "" })
                .when(!self.pseudonymization.scanning() && !installing && detector::SUPPORTED, |bar| bar.child(
                    div().id("scan-pseudonyms").cursor_pointer().px_2().py_1().rounded_md().hover(|v| v.bg(palette.placeholder_bg)).child("Rescan").on_click(cx.listener(|this, _, _, cx| this.scan_pseudonyms(cx)))))
                .when(self.pseudonymization.scanning(), |bar| bar.child(div().id("cancel-pseudonyms").cursor_pointer().px_2().py_1().child("Cancel").on_click(cx.listener(|this, _, _, cx| { this.pseudonymization.cancel(); cx.notify(); }))))
                .child(div().id("pseudonym-category").cursor_pointer().px_2().py_1().child(format!("Selection type: {} ↻", self.pseudonymization.category.token())).on_click(cx.listener(|this, _, _, cx| {
                    let index = Category::ALL.iter().position(|category| *category == this.pseudonymization.category).unwrap_or(0);
                    this.pseudonymization.category = Category::ALL[(index + 1) % Category::ALL.len()]; cx.notify();
                })))
                .child(button("Add selection", AddPseudonymCandidate, self.theme.get()))
                .child(button("Previous", PreviousCandidate, self.theme.get())).child(button("Next", NextCandidate, self.theme.get()))
                .child(div().flex_1())
                .child(div().id("leave-pseudonyms").cursor_pointer().px_2().py_1().child("Done").on_click(cx.listener(|this, _, _, cx| this.leave_pseudonyms(cx)))))
            .child(div().text_color(palette.header_muted).child("Experimental · May miss identifying information, especially Russian and hidden source. Review the complete Markdown before sharing."))
            .child(button("Settings",Settings,self.theme.get()))
            .when(!self.pseudonymization.scans.is_empty(),|bar|bar.child(div().text_color(palette.header_muted).child(format!("Completed scans: {}",self.pseudonymization.scans.iter().map(|c|format!("{} (threshold {})",c.model.name(),c.threshold)).collect::<Vec<_>>().join(", ")))))
            .when(self.pseudonymization.review.skipped_syntax_spans > 0, |bar| bar.child(div().text_color(palette.header_muted).child("Some detector spans cross source syntax. Add a narrower selection manually.")))
            .when_some(self.pseudonymization.error.clone(), |bar, error| bar.child(div().child(error)))
            .when(!installing && !self.pseudonymization.scanning() && detector::SUPPORTED && !model_ready, |bar| bar.child(div().id("setup-pseudonyms").cursor_pointer().py_1()
                .child(format!("Download experimental model (up to {} MB) · local scans afterwards", detector::download_megabytes(self.preferences.borrow().snapshot().map(|p|p.pseudonymization.model).unwrap_or_default())))
                .on_click(cx.listener(|this, _, window, cx| this.setup_pseudonyms(window,cx)))))
            .into_any_element())
    }
    pub(super) fn pseudonym_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let popup = self.pseudonymization.popup.as_ref()?;
        let group = self.pseudonymization.review.group(popup.group)?;
        let original = group.original.clone();
        let count = group.mentions.len();
        let index = group
            .mentions
            .iter()
            .position(|range| *range == popup.mention)?;
        let anchor = self
            .editor
            .read(cx)
            .annotation_bounds((group.id << 32) | index as u64);
        let position: Point<Pixels> = anchor
            .map(|bounds| gpui::point(bounds.left(), bounds.bottom() + px(4.)))
            .unwrap_or_else(|| self.scroll.bounds().origin + gpui::point(px(24.), px(24.)));
        let palette = self.theme.get().pdf_style();
        let all = popup.all;
        let mappings = self.pseudonymization.review.mappings();
        Some(
            deferred(
                anchored().position(position).snap_to_window().child(
                    div()
                        .id("pseudonym-popup")
                        .key_context("PseudonymReview")
                        .track_focus(&self.pseudonymization.focus)
                        .occlude()
                        .w(px(340.))
                        .max_w_full()
                        .p_3()
                        .rounded_md()
                        .shadow_md()
                        .bg(palette.bg)
                        .border_1()
                        .border_color(palette.border)
                        .text_size(px(13.))
                        .on_action(cx.listener(Self::accept_pseudonym))
                        .on_action(cx.listener(Self::keep_pseudonym))
                        .on_action(cx.listener(Self::close_pseudonym_popup))
                        .on_mouse_down_out(cx.listener(|this, _: &gpui::MouseDownEvent, _, cx| {
                            this.pseudonymization.popup = None;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(format!("{} · {count} occurrences", group.category.token()))
                                .child(div().flex_1())
                                .child(button("×", ClosePseudonymPopup, self.theme.get())),
                        )
                        .child(
                            div()
                                .id("pseudonym-original")
                                .max_h(px(80.))
                                .overflow_y_scroll()
                                .py_2()
                                .child(original),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(palette.header_muted)
                                .child("Source fragment (includes hidden Markdown):"),
                        )
                        .child(
                            div().py_1().child(source_fragment(
                                self.editor.read(cx).text(),
                                &popup.mention,
                            )),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .py_2()
                                .child("Replacement")
                                .child(
                                    div()
                                        .id("pseudonym-replacement")
                                        .flex_1()
                                        .min_w_0()
                                        .border_1()
                                        .border_color(palette.border)
                                        .rounded_sm()
                                        .p_1()
                                        .child(self.pseudonymization.input.clone()),
                                ),
                        )
                        .when_some(self.pseudonymization.error.clone(), |view, error| {
                            view.child(div().py_1().text_color(palette.header_muted).child(error))
                        })
                        .when(mappings.len() > 1, |view| {
                            view.child(
                                div()
                                    .id("pseudonym-links")
                                    .max_h(px(100.))
                                    .overflow_y_scroll()
                                    .text_size(px(11.))
                                    .child("Link variant to existing placeholder:")
                                    .children(
                                        mappings
                                            .into_iter()
                                            .filter(|(_, replacement)| {
                                                *replacement != group.replacement
                                            })
                                            .map(|(original, replacement)| {
                                                div()
                                                    .id(gpui::SharedString::from(format!(
                                                        "link-{replacement}"
                                                    )))
                                                    .py_1()
                                                    .cursor_pointer()
                                                    .child(format!("{replacement} · {original}"))
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.pseudonymization.input.update(
                                                            cx,
                                                            |input, cx| {
                                                                input.set_value(
                                                                    replacement.clone(),
                                                                    cx,
                                                                )
                                                            },
                                                        );
                                                    }))
                                            }),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .id("pseudonym-scope")
                                .cursor_pointer()
                                .py_2()
                                .child(if all {
                                    "✓ All exact occurrences · click for this occurrence only"
                                } else {
                                    "This occurrence only · click for all exact occurrences"
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(popup) = &mut this.pseudonymization.popup {
                                        popup.all = !popup.all;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(button("Accept", AcceptPseudonymCandidate, self.theme.get()))
                                .child(button("Keep", KeepPseudonymCandidate, self.theme.get())),
                        ),
                ),
            )
            .into_any_element(),
        )
    }
}

fn source_fragment(source: &str, range: &Range<usize>) -> String {
    let start = source.floor_char_boundary(range.start.min(source.len()).saturating_sub(32));
    let end = source.ceil_char_boundary(range.end.saturating_add(32).min(source.len()));
    source[start..end].replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn cancelled_edited_and_replaced_document_results_are_rejected(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::ui_tests::boot(cx);
        app.update(cx, |app, cx| {
            app.editor
                .update(cx, |editor, cx| editor.set_text("Alice", cx));
            let result = || {
                Ok(vec![pseudonymization::Detection {
                    range: 0..5,
                    category: Category::Person,
                    score: 0.9,
                }])
            };
            let identity = app.session.generation;
            let revision = app.editor.read(cx).revision();
            let install_job = |app: &mut Workspace| {
                app.pseudonymization.job = Some(ScanJob {
                    cancel: Arc::new(AtomicBool::new(false)),
                    revision,
                    generation: 7,
                    config: settings::PiiConfig::default(),
                });
            };
            install_job(app);
            app.pseudonymization.cancel();
            app.complete_pseudonym_scan(7, identity, revision, result(), cx);
            assert!(app.pseudonymization.review.groups.is_empty());
            install_job(app);
            app.editor
                .update(cx, |editor, cx| editor.replace_range(0..5, "Betty", cx));
            app.complete_pseudonym_scan(7, identity, revision, result(), cx);
            assert!(app.pseudonymization.review.groups.is_empty());
            app.editor
                .update(cx, |editor, cx| editor.set_text("Alice", cx));
            let revision = app.editor.read(cx).revision();
            app.pseudonymization.job = Some(ScanJob {
                cancel: Arc::new(AtomicBool::new(false)),
                revision,
                generation: 9,
                config: settings::PiiConfig::default(),
            });
            app.session.replace(Document::default());
            app.complete_pseudonym_scan(9, identity, revision, result(), cx);
            assert!(app.pseudonymization.review.groups.is_empty());
            app.pseudonymization.cancel();
        });
    }
}
