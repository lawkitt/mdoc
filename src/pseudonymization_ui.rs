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
    context_open: bool,
    links_open: bool,
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
    accept_all_bounds: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
    pub input: Entity<markdown_search::SearchInput>,
    pub focus: FocusHandle,
    pub category: Category,
    job: Option<ScanJob>,
    details: bool,
    pub scans: Vec<settings::PiiConfig>,
    pub error: Option<String>,
    generation: u64,
    menu_open: bool,
    menu_focus: FocusHandle,
    menu_previous: Option<FocusHandle>,
    menu_bounds: Rc<Cell<gpui::Bounds<Pixels>>>,
    popup_previous: Option<FocusHandle>,
    popup_scroll: gpui::ScrollHandle,
    popup_controls: std::cell::RefCell<std::collections::HashMap<gpui::ElementId, FocusHandle>>,
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
            accept_all_bounds: Rc::new(Cell::new(None)),
            input: cx.new(|cx| {
                markdown_search::SearchInput::new(cx).with_key_context("PseudonymReplacement")
            }),
            focus: cx.focus_handle(),
            category: Category::Person,
            job: None,
            details: false,
            scans: Vec::new(),
            error: None,
            generation: 0,
            menu_open: false,
            menu_focus: cx.focus_handle(),
            menu_previous: None,
            menu_bounds: Rc::new(Cell::new(gpui::Bounds::default())),
            popup_previous: None,
            popup_scroll: gpui::ScrollHandle::new(),
            popup_controls: Default::default(),
        }
    }
    fn reveal_popup_control(
        &self,
        id: impl Into<gpui::ElementId>,
        control: gpui::Stateful<gpui::Div>,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let focus = self
            .popup_controls
            .borrow_mut()
            .entry(id.into())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        ui::reveal_focus(
            control.track_focus(&focus),
            focus,
            self.popup_scroll.clone(),
        )
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
        KeyBinding::new(
            "enter",
            AcceptPseudonymCandidate,
            Some("PseudonymReview && !UiControl && !UiMenu"),
        ),
        KeyBinding::new("alt-k", KeepPseudonymCandidate, Some("PseudonymReview")),
        KeyBinding::new("escape", ClosePseudonymPopup, Some("PseudonymReview")),
    ]);
}

impl Workspace {
    pub(super) fn reset_pseudonymization(&mut self, cx: &mut Context<Self>) {
        self.pseudonymization.cancel();
        self.pseudonymization.review = Review::default();
        self.pseudonymization.scans.clear();
        self.pseudonymization.details = false;
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
        if !self.can_copy_markdown() {
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
                self.model_panel.update(cx, |panel, cx| {
                    let model = settings::Model::Pii(config.model);
                    let index = settings::Model::ALL
                        .iter()
                        .position(|m| *m == model)
                        .unwrap();
                    panel.statuses[index] = settings_ui::Status::Ready;
                    cx.notify();
                });
                self.pseudonymization.scans.push(config);
                self.sync_annotations(cx);
            }
            Err(error) => {
                self.pseudonymization.error = Some(error);
            }
        }
        cx.notify();
    }

    fn setup_pseudonyms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !detector::SUPPORTED {
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
        self.model_panel.update(cx, |panel, cx| {
            panel.setup(settings::Model::Pii(config.model), false, cx);
            panel.show(window, cx);
        });
    }

    fn leave_pseudonyms(&mut self, cx: &mut Context<Self>) {
        self.pseudonymization.cancel();
        self.pseudonymization.review.open = false;
        self.pseudonymization.menu_open = false;
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
            context_open: self.editor.read(cx).annotation_is_hidden(id),
            links_open: false,
        });
        self.pseudonymization
            .popup_scroll
            .set_offset(gpui::point(px(0.), px(0.)));
        self.pseudonymization.popup_previous = window.focused(cx);
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
    pub(super) fn accept_all_pseudonyms(
        &mut self,
        _: &AcceptAllPseudonyms,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.pseudonymization.review.open
            || !self.can_copy_markdown()
            || self.pseudonymization.scanning()
            || self.pseudonymization.review.remaining() == 0
        {
            return;
        }
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        let draft = self.pseudonymization.popup.as_ref().map(|popup| {
            (
                popup.group,
                self.pseudonymization.input.read(cx).value().to_owned(),
            )
        });
        match self.pseudonymization.review.plan_all(
            editor.text(),
            draft
                .as_ref()
                .map(|(id, replacement)| (*id, replacement.as_str())),
        ) {
            Ok(edits) => {
                if self
                    .editor
                    .update(cx, |editor, cx| editor.replace_ranges(revision, &edits, cx))
                {
                    if let Some((id, replacement)) = draft {
                        self.pseudonymization
                            .review
                            .set_replacement(id, &replacement);
                    }
                    self.pseudonymization
                        .review
                        .refresh_after_edits(self.editor.read(cx).text(), &edits);
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
        if let Some(focus) = self.pseudonymization.popup_previous.take() {
            window.focus(&focus, cx);
        } else {
            window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }
    fn close_review_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pseudonymization.menu_open = false;
        if let Some(focus) = self.pseudonymization.menu_previous.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }
    pub(super) fn pseudonym_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.pseudonymization.review.open || !self.can_copy_markdown() {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let scanning = self.pseudonymization.scanning();
        let remaining = self.pseudonymization.review.remaining();
        let can_accept_all = remaining > 0 && !scanning;
        let accept_all_bounds = self.pseudonymization.accept_all_bounds.clone();
        let menu_bounds = self.pseudonymization.menu_bounds.clone();
        let selected = self
            .preferences
            .borrow()
            .snapshot()
            .ok()
            .map(|p| p.pseudonymization.model);
        let needs_setup = selected.is_some_and(|model| {
            let model = settings::Model::Pii(model);
            let index = settings::Model::ALL
                .iter()
                .position(|m| *m == model)
                .unwrap();
            !matches!(
                self.model_panel.read(cx).statuses[index],
                settings_ui::Status::Ready
            )
        });
        Some(div().id("pseudonym-review-bar").relative().flex().flex_col().gap_1().px_2().py_1().text_size(px(12.)).border_b_1().border_color(p.border)
            .child(div().flex().items_center().gap_1()
                .child(div().flex_1().min_w_0().truncate().font_weight(gpui::FontWeight::SEMIBOLD).child(if scanning { "Scanning…".to_owned() } else { format!("{remaining} candidates") }))
                .child(button("Previous", PreviousCandidate, theme))
                .child(button("Next", NextCandidate, theme))
                .child(ui::control("review-rescan", if scanning { "Cancel" } else { "Rescan" }, theme, true)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.pseudonymization.scanning() { this.pseudonymization.cancel(); cx.notify(); }
                        else { this.pseudonymize(&Pseudonymize, window, cx); }
                    })))
                .child(ui::control("review-menu", "More", theme, true)
                    .child(gpui::canvas(move |bounds, _, _| menu_bounds.set(bounds), |_, _, _, _| {}).absolute().inset_0())
                    .relative()
                    .when(cfg!(test), |v| v.debug_selector(|| "review-menu".into()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.pseudonymization.menu_open { this.close_review_menu(window, cx); } else {
                            this.pseudonymization.menu_previous = window.focused(cx);
                            this.pseudonymization.menu_open = true;
                            window.focus(&this.pseudonymization.menu_focus, cx); cx.notify();
                        }
                    })))
                .child(ui::control("leave-pseudonyms", "Close review", theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "leave-pseudonyms".into()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.leave_pseudonyms(cx);
                        window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                    }))))
            .child(div().flex().items_center().gap_1().text_color(p.header_muted)
                .child(div().flex_1().min_w_0().text_size(px(11.)).child("Experimental · May miss identifying details. Review all Markdown before sharing."))
                .child(ui::control("pseudonym-details", if self.pseudonymization.details { "Less" } else { "Details" }, theme, true)
                    .on_click(cx.listener(|this, _, _, cx| { this.pseudonymization.details = !this.pseudonymization.details; cx.notify(); }))))
            .when(self.pseudonymization.details, |v| v.child(div().text_size(px(11.)).text_color(p.header_muted)
                .child(format!("Russian and hidden-source details may be missed. Completed scans: {}{}", self.pseudonymization.scans.iter().map(|c| format!("{} (threshold {})", c.model.name(), c.threshold)).collect::<Vec<_>>().join(", "), if self.pseudonymization.review.skipped_syntax_spans > 0 { ". Some spans cross Markdown syntax; add a narrower selection manually." } else { "" }))))
            .when_some(self.pseudonymization.error.clone(), |v, error| v.child(div().text_color(style::markdown_style(theme).alert_warning).child(error)))
            .when(self.pseudonymization.menu_open, |v| v.child(deferred(div().absolute().top(px(34.)).right(px(8.)).w(px(230.)).p_1()
                .id("review-command-menu").key_context("UiPanel UiMenu").track_focus(&self.pseudonymization.menu_focus).tab_group().tab_stop(false)
                .occlude().rounded_md().shadow_md().bg(p.bg).border_1().border_color(p.border).flex().flex_col()
                .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| { ui::cycle(window, cx, Some(&this.pseudonymization.menu_focus), false); cx.stop_propagation(); }))
                    .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| { ui::cycle(window, cx, Some(&this.pseudonymization.menu_focus), true); cx.stop_propagation(); }))
                    .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| this.close_review_menu(window, cx)))
                .on_mouse_down_out(cx.listener(|this, _: &gpui::MouseDownEvent, window, cx| this.close_review_menu(window, cx)))
                .child(ui::control("pseudonym-category", format!("Category: {}", self.pseudonymization.category.label()), theme, true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let index = Category::ALL.iter().position(|c| *c == this.pseudonymization.category).unwrap_or(0);
                        this.pseudonymization.category = Category::ALL[(index + 1) % Category::ALL.len()]; cx.notify();
                    })))
                .child(ui::control("add-pseudonym-selection", "Add selection", theme, true)
                    .on_click(cx.listener(|this, _, window, cx| { this.close_review_menu(window, cx); this.add_pseudonym(&AddPseudonymCandidate, window, cx); })))
                .child(ui::control("accept-all-pseudonyms", "Accept all", theme, can_accept_all).relative()
                    .when(cfg!(test), |v| v.debug_selector(|| "accept-all-pseudonyms".into()))
                    .child(gpui::canvas(move |bounds, _, _| accept_all_bounds.set(Some(bounds)), |_, _, _, _| {}).absolute().inset_0())
                    .on_click(cx.listener(move |this, _, window, cx| { if can_accept_all { this.close_review_menu(window, cx); this.accept_all_pseudonyms(&AcceptAllPseudonyms, window, cx); } })))
                .child(ui::control("review-settings", "Settings", theme, true)
                    .on_click(cx.listener(|this, _, window, cx| { this.close_review_menu(window, cx); this.model_panel.update(cx, |panel, cx| panel.show(window, cx)); })))
                .when(needs_setup, |v| v.child(ui::control("review-download-model", format!("Download model ({} MB)", selected.map(detector::download_megabytes).unwrap_or(0)), theme, !model_work::busy())
                    .when(cfg!(test), |v| v.debug_selector(|| "review-download-model".into()))
                    .on_click(cx.listener(|this, _, window, cx| { if !model_work::busy() { this.close_review_menu(window, cx); this.setup_pseudonyms(window, cx); } }))))
            )))
            .into_any_element())
    }
    pub(super) fn pseudonym_popup(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
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
                        .track_scroll(&self.pseudonymization.popup_scroll)
                        .key_context("PseudonymReview UiPanel")
                        .tab_group()
                        .tab_stop(false)
                        .track_focus(&self.pseudonymization.focus)
                        .occlude()
                        .w(px(340.))
                        .max_w_full()
                        .max_h((window.viewport_size().height - px(40.)).max(px(120.)))
                        .overflow_y_scroll()
                        .p_3()
                        .rounded_md()
                        .shadow_md()
                        .bg(palette.bg)
                        .border_1()
                        .border_color(palette.border)
                        .text_size(px(13.))
                        .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                            ui::cycle(window, cx, Some(&this.pseudonymization.focus), false);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                            ui::cycle(window, cx, Some(&this.pseudonymization.focus), true);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(Self::accept_pseudonym))
                        .on_action(cx.listener(Self::keep_pseudonym))
                        .on_action(cx.listener(Self::close_pseudonym_popup))
                        .on_mouse_down_out(cx.listener(
                            |this, event: &gpui::MouseDownEvent, _, cx| {
                                // Keep the replacement draft until the bulk button's
                                // click handler can validate and apply it.
                                if this.pseudonymization.menu_open
                                    || this
                                        .pseudonymization
                                        .menu_bounds
                                        .get()
                                        .contains(&event.position)
                                    || this
                                        .pseudonymization
                                        .accept_all_bounds
                                        .get()
                                        .is_some_and(|bounds| bounds.contains(&event.position))
                                {
                                    return;
                                }
                                this.pseudonymization.popup = None;
                                cx.notify();
                            },
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(format!("{} · {count} occurrences", group.category.label()))
                                .child(div().flex_1())
                                .child(self.pseudonymization.reveal_popup_control(
                                    "popup-close",
                                    button("×", ClosePseudonymPopup, self.theme.get()),
                                    cx,
                                )),
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
                                        .child(self.pseudonymization.input.clone())
                                        .map(|v| {
                                            ui::reveal_focus(
                                                v,
                                                self.pseudonymization
                                                    .input
                                                    .read(cx)
                                                    .focus_handle(cx),
                                                self.pseudonymization.popup_scroll.clone(),
                                            )
                                        }),
                                ),
                        )
                        .when_some(self.pseudonymization.error.clone(), |view, error| {
                            view.child(div().py_1().text_color(palette.header_muted).child(error))
                        })
                        .child(
                            div()
                                .id("pseudonym-scope")
                                .map(|v| {
                                    self.pseudonymization.reveal_popup_control(
                                        "pseudonym-scope",
                                        v,
                                        cx,
                                    )
                                })
                                .aria_label("Apply to all exact occurrences")
                                .focus_visible(|s| s.bg(palette.placeholder_bg))
                                .key_context("UiControl")
                                .tab_index(0)
                                .role(gpui::Role::CheckBox)
                                .aria_toggled(if all {
                                    gpui::Toggled::True
                                } else {
                                    gpui::Toggled::False
                                })
                                .cursor_pointer()
                                .py_2()
                                .child(if all {
                                    "☑ Apply to all exact occurrences"
                                } else {
                                    "☐ Apply to all exact occurrences"
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
                                .child(self.pseudonymization.reveal_popup_control(
                                    "Accept",
                                    button("Accept", AcceptPseudonymCandidate, self.theme.get()),
                                    cx,
                                ))
                                .child(self.pseudonymization.reveal_popup_control(
                                    "Keep",
                                    button("Keep", KeepPseudonymCandidate, self.theme.get()),
                                    cx,
                                )),
                        )
                        .child(
                            ui::control(
                                "candidate-context",
                                if popup.context_open {
                                    "Hide source context"
                                } else {
                                    "Source context"
                                },
                                self.theme.get(),
                                true,
                            )
                            .map(|v| {
                                self.pseudonymization.reveal_popup_control(
                                    "candidate-context",
                                    v,
                                    cx,
                                )
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(popup) = &mut this.pseudonymization.popup {
                                    popup.context_open = !popup.context_open;
                                }
                                cx.notify();
                            })),
                        )
                        .when(popup.context_open, |v| {
                            v.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(palette.header_muted)
                                    .child("Source fragment (includes hidden Markdown):"),
                            )
                            .child(div().py_1().child(
                                source_fragment(self.editor.read(cx).text(), &popup.mention),
                            ))
                        })
                        .when(mappings.len() > 1, |v| {
                            v.child(
                                ui::control(
                                    "candidate-links",
                                    if popup.links_open {
                                        "Hide linking"
                                    } else {
                                        "Link to existing placeholder"
                                    },
                                    self.theme.get(),
                                    true,
                                )
                                .map(|v| {
                                    self.pseudonymization.reveal_popup_control(
                                        "candidate-links",
                                        v,
                                        cx,
                                    )
                                })
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        if let Some(popup) = &mut this.pseudonymization.popup {
                                            popup.links_open = !popup.links_open;
                                        }
                                        cx.notify();
                                    },
                                )),
                            )
                        })
                        .when(popup.links_open && mappings.len() > 1, |view| {
                            view.child(
                                div()
                                    .id("pseudonym-links")
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
                                                    .when(
                                                        cfg!(test) && replacement == "PERSON_30",
                                                        |v| {
                                                            v.debug_selector(|| {
                                                                "last-candidate-link".into()
                                                            })
                                                        },
                                                    )
                                                    .role(gpui::Role::Button)
                                                    .aria_label(format!("Link to {replacement}"))
                                                    .focus_visible(|s| s.bg(palette.placeholder_bg))
                                                    .key_context("UiControl")
                                                    .tab_index(0)
                                                    .map(|v| {
                                                        self.pseudonymization.reveal_popup_control(
                                                            gpui::SharedString::from(format!(
                                                                "link-{replacement}"
                                                            )),
                                                            v,
                                                            cx,
                                                        )
                                                    })
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
                        }),
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
    use gpui::InputEvent;
    #[gpui::test]
    fn accept_all_button_applies_pending_replacements_as_one_undo_step(
        cx: &mut gpui::TestAppContext,
    ) {
        let (app, cx) = crate::ui_tests::boot(cx);
        let source = "**Анна** Acme Анна [mail](anna@example.invalid) Bob";
        app.update(cx, |app, cx| {
            app.editor
                .update(cx, |editor, cx| editor.set_text(source, cx));
        });
        cx.run_until_parked();
        app.update_in(cx, |app, window, cx| {
            let review = &mut app.pseudonymization.review;
            review.open = true;
            let anna = review.add_manual(source, 2..10, Category::Person).unwrap();
            let acme = source.find("Acme").unwrap();
            review
                .add_manual(source, acme..acme + 4, Category::Organization)
                .unwrap();
            let email = source.find("anna@example.invalid").unwrap();
            review
                .add_manual(source, email..email + 20, Category::Email)
                .unwrap();
            let bob = source.find("Bob").unwrap();
            let kept = review
                .add_manual(source, bob..bob + 3, Category::Person)
                .unwrap();
            review.keep(anna, Some(2..10));
            review.keep(kept, None);
            app.sync_annotations(cx);
            app.activate_annotation(anna << 32, window, cx);
            app.pseudonymization
                .input
                .update(cx, |input, cx| input.set_value("PERSON_CUSTOM".into(), cx));
            window.focus(&app.focus, cx);
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let more = cx.debug_bounds("review-menu").unwrap();
        cx.simulate_click(more.center(), Default::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = cx.debug_bounds("accept-all-pseudonyms").unwrap();
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            assert_eq!(
                app.editor.read(cx).text(),
                "**Анна** ORG_1 PERSON_CUSTOM [mail](EMAIL_1) Bob"
            );
            assert_eq!(app.pseudonymization.review.remaining(), 0);
            assert!(app.pseudonymization.popup.is_none());
            assert!(app.dirty(cx));
            assert!(app.pseudonymization.error.is_none());
        });
        // With no pending suggestions, another click must not add an undo step.
        let revision = app.read_with(cx, |app, cx| app.editor.read(cx).revision());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let more = cx.debug_bounds("review-menu").unwrap();
        cx.simulate_click(more.center(), Default::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = cx.debug_bounds("accept-all-pseudonyms").unwrap();
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        app.update_in(cx, |app, window, cx| {
            assert_eq!(app.editor.read(cx).revision(), revision);
            window.focus(&app.editor.read(cx).focus_handle(cx), cx);
        });
        cx.dispatch_action(mdoc_editor::Undo);
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            assert_eq!(app.editor.read(cx).text(), source);
            assert_eq!(app.pseudonymization.review.remaining(), 3);
        });
        cx.dispatch_action(mdoc_editor::Redo);
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            assert_eq!(
                app.editor.read(cx).text(),
                "**Анна** ORG_1 PERSON_CUSTOM [mail](EMAIL_1) Bob"
            );
            assert_eq!(app.pseudonymization.review.remaining(), 0);
        });
    }
    #[gpui::test]
    fn keyboard_reveals_long_candidate_links_without_editing_source(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::ui_tests::boot(cx);
        cx.simulate_resize(gpui::size(px(640.), px(480.)));
        let source = (0..30)
            .map(|i| format!("Name{i:02}"))
            .collect::<Vec<_>>()
            .join(" ");
        app.update_in(cx, |app, window, cx| {
            app.editor
                .update(cx, |editor, cx| editor.set_text(&source, cx));
            app.pseudonymization.review.open = true;
            for i in 0..30 {
                app.pseudonymization
                    .review
                    .add_manual(&source, i * 7..i * 7 + 6, Category::Person)
                    .unwrap();
            }
            app.sync_pseudonym_theme(cx);
            let id = app.pseudonymization.review.groups[0].id;
            app.activate_annotation(id << 32, window, cx);
            app.pseudonymization.popup.as_mut().unwrap().links_open = true;
            cx.notify();
        });
        cx.run_until_parked();
        let target = gpui::ElementId::from(gpui::SharedString::from("link-PERSON_30"));
        let mut reached = false;
        for _ in 0..50 {
            cx.simulate_keystrokes("tab");
            for _ in 0..3 {
                cx.update(|window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                });
            }
            reached = cx.update(|window, cx| {
                app.read(cx)
                    .pseudonymization
                    .popup_controls
                    .borrow()
                    .get(&target)
                    .is_some_and(|focus| focus.is_focused(window))
            });
            if reached {
                break;
            }
        }
        assert!(reached, "Tab must reach all disclosed linking choices");
        let last = cx.debug_bounds("last-candidate-link").unwrap();
        cx.update(|window, cx| {
            let app = app.read(cx);
            let viewport = app.pseudonymization.popup_scroll.bounds();
            assert!(last.top() >= viewport.top() && last.bottom() <= viewport.bottom());
            let offset = app.pseudonymization.popup_scroll.offset();
            assert!(offset.y < px(0.), "focus must reveal the lower controls");
            assert!(viewport.bottom() <= window.viewport_size().height);
            assert_eq!(app.editor.read(cx).text(), source);
            assert!(app.pseudonymization.popup.is_some());
        });
        cx.simulate_keystrokes("space");
        cx.update(|window, cx| {
            window.dispatch_event(
                gpui::KeyUpEvent {
                    keystroke: gpui::Keystroke::parse("space").unwrap(),
                }
                .to_platform_input(),
                cx,
            );
        });
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            assert_eq!(app.pseudonymization.input.read(cx).value(), "PERSON_30");
            assert_eq!(app.editor.read(cx).text(), source);
        });
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let close = cx.debug_bounds("leave-pseudonyms").unwrap();
        cx.simulate_click(close.center(), Default::default());
        app.update_in(cx, |app, window, cx| {
            assert!(!app.pseudonymization.review.open);
            assert!(app.editor.read(cx).focus_handle(cx).is_focused(window));
            assert_eq!(app.pseudonymization.review.mappings().len(), 30);
            assert_eq!(app.editor.read(cx).text(), source);
        });
    }

    #[gpui::test]
    fn accept_all_refuses_pending_scans_and_invalid_popup_tokens(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::ui_tests::boot(cx);
        let source = "Alice Acme";
        app.update(cx, |app, cx| {
            app.editor
                .update(cx, |editor, cx| editor.set_text(source, cx));
        });
        cx.run_until_parked();
        app.update_in(cx, |app, window, cx| {
            app.pseudonymization.review.open = true;
            let alice = app
                .pseudonymization
                .review
                .add_manual(source, 0..5, Category::Person)
                .unwrap();
            app.pseudonymization
                .review
                .add_manual(source, 6..10, Category::Organization)
                .unwrap();
            let revision = app.editor.read(cx).revision();
            app.pseudonymization.job = Some(ScanJob {
                cancel: Arc::new(AtomicBool::new(false)),
                revision,
                generation: 1,
                config: settings::PiiConfig::default(),
            });
            app.accept_all_pseudonyms(&AcceptAllPseudonyms, window, cx);
            assert_eq!(app.editor.read(cx).revision(), revision);
            assert_eq!(app.editor.read(cx).text(), source);
            app.pseudonymization.cancel();
            app.activate_annotation(alice << 32, window, cx);
            app.pseudonymization
                .input
                .update(cx, |input, cx| input.set_value("invalid token".into(), cx));
            app.accept_all_pseudonyms(&AcceptAllPseudonyms, window, cx);
            assert_eq!(app.editor.read(cx).revision(), revision);
            assert_eq!(app.editor.read(cx).text(), source);
            assert_eq!(app.pseudonymization.review.remaining(), 2);
            assert!(app.pseudonymization.error.is_some());
        });
    }
    #[gpui::test]
    fn successful_scan_removes_setup_prompt_only_for_the_scanned_model(
        cx: &mut gpui::TestAppContext,
    ) {
        let (app, cx) = crate::ui_tests::boot(cx);
        let panel = cx.update(|_, cx| app.read(cx).model_panel.clone());
        app.update(cx, |app, cx| {
            app.editor
                .update(cx, |editor, cx| editor.set_text("Alice", cx));
            app.pseudonymization.review.open = true;
            let revision = app.editor.read(cx).revision();
            app.pseudonymization.job = Some(ScanJob {
                cancel: Arc::new(AtomicBool::new(false)),
                revision,
                generation: 7,
                config: settings::PiiConfig::default(),
            });
            app.complete_pseudonym_scan(
                7,
                app.session.generation,
                revision,
                Ok(vec![pseudonymization::Detection {
                    range: 0..5,
                    category: Category::Person,
                    score: 0.9,
                }]),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let more = cx.debug_bounds("review-menu").unwrap();
        cx.simulate_click(more.center(), Default::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("review-download-model").is_none());
        cx.update(|_, cx| {
            let index = settings::Model::ALL
                .iter()
                .position(|m| *m == settings::Model::Pii(settings::PiiModel::Fp16))
                .unwrap();
            assert!(matches!(
                panel.read(cx).statuses[index],
                settings_ui::Status::Ready
            ));
        });
        app.update(cx, |app, cx| {
            app.preferences
                .borrow_mut()
                .current
                .as_mut()
                .unwrap()
                .pseudonymization
                .model = settings::PiiModel::Fp32;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("review-download-model").is_some());
        panel.update(cx, |panel, _| {
            let index = settings::Model::ALL
                .iter()
                .position(|m| *m == settings::Model::Pii(settings::PiiModel::Fp16))
                .unwrap();
            panel.statuses[index] = settings_ui::Status::Missing;
        });
        app.update(cx, |app, cx| {
            app.preferences
                .borrow_mut()
                .current
                .as_mut()
                .unwrap()
                .pseudonymization
                .model = settings::PiiModel::Fp16;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("review-download-model").is_some());
    }
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
