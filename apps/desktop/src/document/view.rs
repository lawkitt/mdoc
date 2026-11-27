//! How a document tab draws: its header, the find bar with its controls,
//! the Markdown and Original panes side by side, the notices, and the
//! keys and menu commands the tab answers.
#[cfg(test)]
use super::RENDERS;
use super::{
    ClosePdf, DismissImportWarning, DocumentView, ExportMarkdown, ExtractNative, Inspector,
    OcrState, OpenSourceReply, RunOcr, Save, ShowEditView, ShowNotes, ShowOriginalView,
    ShowPseudonymizedView, ShowReplacements, TogglePreview, ZOOM_STEP, ZoomIn, ZoomOut, ZoomReset,
    text::{self, find_counter},
};
use crate::{
    document::search_session::{
        CloseMarkdownSearch, FindNextMarkdown, FindPreviousMarkdown, ReplaceAllMarkdown,
        ReplaceNextMarkdown, ToggleMarkdownMatchCase, ToggleMarkdownWholeWord,
    },
    review::{PiiNextCandidate, PiiPreviousCandidate, PiiReviewCandidate},
    ui,
    ui::style::{self, Theme},
    workspace::{self, New, RetryDocument},
};
use gpui::{Context, Focusable, Window, div, prelude::*, px};
use mdoc_import::{import, ocr};
use mdoc_store::settings;

pub(crate) fn button(label: &'static str, action: impl gpui::Action, theme: Theme) -> ui::Control {
    ui::control(label, label, theme, true)
        .when(cfg!(test), |view| view.debug_selector(move || label.into()))
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

/// The widest the Markdown reading column gets at 100%.
const READING_WIDTH: f32 = 760.;

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        RENDERS.with(|renders| renders.set(renders.get() + 1));
        // The before/after view follows the applied replacements.
        self.refresh_compare(window, cx);
        // The Share panel's reply box restores against the live mapping.
        if self.inspector == Inspector::Share && self.replacements_open() {
            self.ensure_reply_box(window, cx);
            self.refresh_reply();
        }
        if self.inspector == Inspector::Notes && self.replacements_open() {
            self.ensure_notes_editor(window, cx);
        }
        self.release_closed_popup_focus(window, cx);
        // Derived display state; set_outlines only notifies on change.
        self.sync_scope_outlines(cx);
        // Loads and Settings changes reach spellcheck here (no-op otherwise).
        self.ensure_spellcheck(cx);
        // A finished or abandoned OS drag leaves no feedback behind.
        if !cx.has_active_drag() {
            self.file_drag = None;
        }
        let empty_page = self.shows_empty_page(cx);
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let search_focused = self
            .markdown_search
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let query = self.markdown_search.read(cx).value().to_owned();
        let (count, spoken_count) = find_counter(
            query.is_empty(),
            self.search.running(),
            self.search.active(),
            self.search.matches().len(),
        );
        let navigation_disabled = self.search.matches().is_empty();
        let replace_focused = self
            .markdown_replace
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let field = |id: &'static str, focused: bool| {
            div()
                .id(id)
                .when(cfg!(test), |v| v.debug_selector(move || id.into()))
                .flex()
                .flex_1()
                // Keep room for text after padding, even beside the inspector.
                .min_w(px(96.))
                .h(px(28.))
                .px_2()
                .items_center()
                .bg(palette.bg)
                .border_1()
                .border_color(if focused {
                    theme.search_accent()
                } else {
                    palette.border
                })
                .rounded_md()
        };
        let replace_toggle = search_bar_button(
            "markdown-search-replace-toggle",
            if self.search.replace_open() {
                "⌄"
            } else {
                "›"
            },
            text::REPLACE,
            chord("⌥⌘F", "Ctrl+H"),
            theme,
        )
        .w(px(20.))
        .aria_expanded(self.search.replace_open())
        .on_click(cx.listener(|this, _, window, cx| this.toggle_markdown_replace(window, cx)));
        let match_case = search_bar_button(
            "markdown-search-match-case",
            "Aa",
            text::MATCH_CASE,
            chord("⌥⌘C", "Alt+C"),
            theme,
        )
        .aria_toggled(toggled(self.search.match_case()))
        .when(self.search.match_case(), |view| {
            view.bg(palette.placeholder_bg)
                .text_color(theme.search_accent())
        })
        .on_click(cx.listener(|this, _, window, cx| {
            this.toggle_markdown_match_case(&ToggleMarkdownMatchCase, window, cx)
        }));
        let whole_word = search_bar_button(
            "markdown-search-whole-word",
            div().underline().child("ab"),
            text::WHOLE_WORD,
            chord("⌥⌘W", "Alt+W"),
            theme,
        )
        .aria_toggled(toggled(self.search.whole_word()))
        .when(self.search.whole_word(), |view| {
            view.bg(palette.placeholder_bg)
                .text_color(theme.search_accent())
        })
        .on_click(cx.listener(|this, _, window, cx| {
            this.toggle_markdown_whole_word(&ToggleMarkdownWholeWord, window, cx)
        }));
        // One live node for the count, "Searching…" and replace results, so
        // each change is announced.
        let status = self.search.status().cloned();
        let spoken_count = match (&status, self.search.running()) {
            (Some(status), false) => status.clone(),
            _ => spoken_count,
        };
        let count = ui::live_text("markdown-search-count", spoken_count)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "markdown-search-count".into())
            })
            .flex_shrink_0()
            .max_w_full()
            .px_2()
            .text_size(px(12.))
            .text_color(palette.header_muted)
            .child(if self.search.running() {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(ui::spinner("search-activity", theme))
                    .child(div().min_w_0().child(count))
                    .into_any_element()
            } else if let Some(status) = status {
                div().child(status).into_any_element()
            } else {
                div().child(count).into_any_element()
            });
        let previous = search_bar_button(
            "markdown-search-previous",
            "↑",
            text::PREVIOUS_MATCH,
            chord("⇧⌘G", "Shift+Ctrl+G"),
            theme,
        )
        .w(px(28.))
        .justify_center()
        .when(navigation_disabled, |view| {
            view.text_color(palette.header_muted)
        })
        .on_click(cx.listener(|this, _, window, cx| {
            this.find_previous_markdown(&FindPreviousMarkdown, window, cx)
        }));
        let next =
            search_bar_button(
                "markdown-search-next",
                "↓",
                text::NEXT_MATCH,
                chord("⌘G", "Ctrl+G"),
                theme,
            )
            .w(px(28.))
            .justify_center()
            .when(navigation_disabled, |view| {
                view.text_color(palette.header_muted)
            })
            .on_click(cx.listener(|this, _, window, cx| {
                this.find_next_markdown(&FindNextMarkdown, window, cx)
            }));
        let close = search_bar_button(
            "markdown-search-close",
            "×",
            text::CLOSE_MARKDOWN_SEARCH,
            "Esc",
            theme,
        )
        .w(px(28.))
        .justify_center()
        .on_click(cx.listener(|this, _, window, cx| {
            this.close_markdown_search(&CloseMarkdownSearch, window, cx)
        }));
        let replace_row = self.search.replace_open().then(|| {
            div()
                .id("markdown-replace-row")
                .flex()
                .flex_wrap()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_1()
                .pl(px(24.))
                .child(
                    field("markdown-replace-input", replace_focused)
                        .aria_label(text::REPLACE_WITH)
                        .child(self.markdown_replace.clone()),
                )
                .child(
                    search_bar_button(
                        "markdown-replace-next",
                        text::REPLACE,
                        text::REPLACE,
                        "Enter",
                        theme,
                    )
                    .when(navigation_disabled, |view| {
                        view.text_color(palette.header_muted)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.replace_next_markdown(&ReplaceNextMarkdown, window, cx)
                    })),
                )
                .child(
                    search_bar_button(
                        "markdown-replace-all",
                        text::REPLACE_ALL_BUTTON,
                        text::REPLACE_ALL,
                        chord("⌘Enter", "Ctrl+Alt+Enter"),
                        theme,
                    )
                    .when(navigation_disabled, |view| {
                        view.text_color(palette.header_muted)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.replace_all_markdown(&ReplaceAllMarkdown, window, cx)
                    })),
                )
        });
        let search_bar = div()
            .id("markdown-search-bar")
            .track_focus(&self.find_region)
            .on_action(cx.listener(|this, action: &mdoc_editor::Undo, window, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.undo(action, window, cx))
            }))
            .on_action(cx.listener(|this, action: &mdoc_editor::Redo, window, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.redo(action, window, cx))
            }))
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .py_1()
            .text_size(px(13.))
            .flex_shrink_0()
            .border_b_1()
            .border_color(palette.border)
            .bg(palette.bg)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_1()
                    .child(replace_toggle)
                    .child(
                        field("markdown-search-input", search_focused)
                            .aria_label(text::SEARCH_MARKDOWN)
                            .child(self.markdown_search.clone()),
                    )
                    .child(match_case)
                    .child(whole_word)
                    .child(count)
                    .child(previous)
                    .child(next)
                    .child(close),
            )
            .children(replace_row);
        let markdown_scrollbar = gpui_pdf::scrollbar::overlay_scrollbar(
            "markdown-scrollbar",
            &self.scroll,
            false,
            palette.header_muted,
            cx,
        );
        let import_notice =
            self.session.warning.clone().map(|detail| {
                self.notice("import-notice", text::REVIEW_CONVERSION, detail, false, cx)
            });
        let ocr_notice = (self.import.ocr_required().is_some()
            && !self.import.ocr_notice_dismissed())
        .then(|| {
            self.notice(
                "ocr-notice",
                text::TEXT_RECOGNITION_REQUIRED,
                format!("{}.", self.ocr_pages_title()),
                false,
                cx,
            )
        });
        let preview_notice = self.preview.message().cloned().map(|detail| {
            self.notice(
                "preview-notice",
                text::PREVIEW,
                detail,
                self.preview.failed(),
                cx,
            )
        });
        let setup_notice = match &self.ocr_state {
            OcrState::Failed(error) if !self.import.setup_error_dismissed() => {
                Some(self.notice("setup-notice", text::OCR_SETUP, error.clone(), true, cx))
            }
            _ => None,
        };
        let clipboard_notice = self
            .clipboard_notice
            .text()
            .map(str::to_owned)
            .map(|detail| self.notice("clipboard-notice", text::CLIPBOARD, detail, false, cx));
        let width = self.chrome_width(window);
        let content_width = (width - f32::from(self.replacements_panel_width(window))).max(1.);
        let narrow_preview = self.preview.visible() && content_width < 620.;
        let show_original = self.preview.visible() && (!narrow_preview || self.original_selected);
        let show_markdown = !narrow_preview || !self.original_selected;
        self.panes_drawn.set((show_markdown, show_original));
        // The conversion card states a conversion error itself, and the OCR
        // card its recognition error; a notice would only repeat it.
        let card_shows_error = self.lifecycle.source_only()
            && show_markdown
            && !self.import.busy()
            && !self.import.auto_pending()
            && (self.import.ocr_required().is_none()
                || ocr::SUPPORTED
                    && self.error.as_ref().is_some_and(|error| {
                        [self.import.ocr_failure(), self.import.ocr_error()].contains(&Some(error))
                            || matches!(&self.ocr_state, OcrState::Failed(e) if e == error)
                    }));
        // The OCR setup notice may already say the same thing.
        let setup_says_error = !self.import.setup_error_dismissed()
            && !(show_markdown
                && self.lifecycle.source_only()
                && self.import.ocr_required().is_some())
            && matches!(&self.ocr_state, OcrState::Failed(e) if Some(e) == self.error.as_ref());
        let error_notice = self
            .error
            .clone()
            // A cancel is not an error: the card says so, and with the card
            // hidden nothing needs attention (#950 review).
            .filter(|_| !card_shows_error && !setup_says_error && !self.conversion_cancelled())
            .map(|detail| self.notice("error-notice", text::NEEDS_ATTENTION, detail, true, cx));
        let refusal_notice = (self.copy_refusal.as_ref())
            .map(|refusal| refusal.reason.clone())
            .map(|detail| self.notice("copy-refusal-notice", text::NOT_COPIED, detail, false, cx));
        let minimum = (280. / content_width.max(620.)).min(0.5);
        let split = self
            .preview
            .split_ratio()
            .unwrap_or(0.5)
            .clamp(minimum, 1. - minimum);
        let measured_bounds = self.view_bounds.clone();
        let measured_owner = cx.entity().downgrade();
        let detached = self.title.detached.get();
        let body = div().relative().size_full().flex().flex_col().bg(palette.bg).text_color(palette.header_fg).text_size(px(16.))
            .child(gpui::canvas(move |bounds, _, cx| { if measured_bounds.replace(bounds) != bounds { let _ = measured_owner.update(cx, |_, cx| cx.notify()); } }, |_, _, _, _| {}).absolute().inset_0())
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                // Repaint so a replacement tooltip goes when the pointer leaves it.
                if this.editor.read(cx).hovered_annotation().is_some() { cx.notify(); }
                if event.pressed_button != Some(gpui::MouseButton::Left) { this.split_dragging = false; }
                if this.split_dragging {
                    let bounds = this.view_bounds.get();
                    let panel = this.replacements_panel_width(window);
                    let width = f32::from(bounds.size.width - panel).max(1.);
                    let ratio = f32::from(event.position.x - bounds.left() - panel) / width;
                    this.preview.set_split(ratio);
                    cx.notify();
                }
            }))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| { this.split_dragging = false; cx.notify(); }))
            // While the empty page shows, the whole document view accepts OS file drops.
            .when(empty_page, |v| v
                .on_drag_move(cx.listener(|this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                    let over = event.bounds.contains(&event.event.position);
                    let state = over.then(|| event.drag(cx).paths().iter().any(|path| import::droppable(path)));
                    if this.file_drag != state { this.file_drag = state; cx.notify(); }
                }))
                .can_drop(|drag, _, _| drag.downcast_ref::<gpui::ExternalPaths>().is_some_and(|drag| drag.paths().iter().any(|path| import::droppable(path))))
                .on_drop(cx.listener(|this, drag: &gpui::ExternalPaths, window, cx| {
                    this.file_drag = None;
                    workspace::activate_for_drop(window, cx);
                    cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::Open(drag.paths().to_vec())));
                    cx.notify();
                })))
            .when(!detached, |v| v.child(div().track_focus(&self.toolbar_region).key_context("ToolbarRegion").child(self.header(window, cx))))
            .child(div().track_focus(&self.controls_region).key_context("ToolbarRegion").flex().flex_col().flex_shrink_0()
            .when(!self.lifecycle.source_only() && self.import.busy(), |v| v.child(
                ui::activity("document-work-activity", if matches!(self.ocr_state, OcrState::Installing) || self.import.setting_up() { text::SETTING_UP_TEXT_RECOGNITION } else if self.import.recognizing() { text::RECOGNIZING_TEXT } else { text::CONVERTING_TO_MARKDOWN }, theme)
                    .px_3().py_2().border_b_1().border_color(palette.border)))
            .when_some(self.session.ocr_configuration.clone(),|v,config|v.child(div().px_3().py_1().text_size(px(11.)).text_color(palette.header_muted).child(text::recognized_by(crate::app::models::short_name(settings::Model::Ocr(config.model)),config.dpi,config.minimum_confidence))))
            .children(import_notice).children(clipboard_notice).children(ocr_notice.filter(|_| !show_markdown))
            .when(narrow_preview, |v| v.child(self.pane_switch(cx))))
            .child(div().flex().flex_1().min_h_0()
                .when(!self.lifecycle.source_only() && show_markdown, |row| row.child(div().relative().flex().flex_1().min_w_0().h_full().flex().flex_col().text_size(px(16. * self.zoom))
                    .child({ let column = self.switcher.column.clone(); gpui::canvas(move |bounds, _, _| column.set(bounds), |_, _, _, _| {}).absolute().inset_0() })
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| this.switcher_pointer(event.position, cx)))
                    .when(self.search.open(), |column| column.child(search_bar))
                    .child(if self.lifecycle.loading() || self.lifecycle.unavailable() {
                        div().p_6().child(if self.lifecycle.loading() { ui::activity("markdown-loading", text::LOADING_MARKDOWN, theme).into_any_element() } else { div().child(text::MARKDOWN_UNAVAILABLE).into_any_element() })
                            .when(self.lifecycle.unavailable(), |view| view.child(button(text::RETRY, RetryDocument, theme))).into_any_element()
                    } else if let Some(compare) = self.compare_view(content_width, cx) {
                        compare
                    } else { div().relative().flex_1().min_w_0().min_h_0()
                        // Absolute, so the column's intrinsic sizing never measures the
                        // editor: each probe width (search-bar min-content, max content,
                        // 0 px) shaped it again, and a changing one missed every cache.
                        .child(div().id("document-scroll").absolute().inset_0().overflow_y_scroll().track_scroll(&self.scroll).p_6().pb(px(72.))
                            .on_scroll(cx.listener(|this, _, _, cx| { this.wake_switcher(cx); cx.notify(); }))
                            // A click in the blank page under the last line puts the caret
                            // at the end, as in other editors; it used to take focus away
                            // from the editor, so typing went nowhere.
                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| this.click_below_text(event, window, cx)))
                            // A centred reading column.
                            .child(div().relative().w_full().max_w(px(READING_WIDTH * self.zoom)).mx_auto().child(self.editor.clone())
                                // The mention bar scrolls with its mention.
                                .children(self.mention_popup_layer(window, cx))))
                        .child(markdown_scrollbar).children((empty_page && self.file_drag.is_some()).then(|| self.empty_page(cx))).into_any_element() })
                    .children(self.view_switcher(window, cx).filter(|_| !self.lifecycle.loading() && !self.lifecycle.unavailable()))))
                // Replacements sit beside the text they describe, before Original.
                .children(self.replacements_panel(window, cx))
                .when(self.lifecycle.source_only() && show_markdown, |row| row.child(div().id("conversion-pane").flex_1().min_w_0().p_6().flex().items_center().justify_center().overflow_y_scroll()
                    .child(self.conversion_card(window, cx))))
                .when(self.preview.visible() && !narrow_preview, |row| row.child(div().id("preview-divider").when(cfg!(test), |v| v.debug_selector(|| "preview-divider".into())).w(px(6.)).h_full().flex_shrink_0().cursor(gpui::CursorStyle::ResizeLeftRight).bg(palette.border)
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| { this.split_dragging = true; cx.notify(); }))))
                .when(show_original && self.preview.pdf().is_none(), |row| row.child(div().track_focus(&self.preview_region).when(!narrow_preview, |v| v.w(px(content_width * (1. - split)))).when(narrow_preview, |v| v.flex_1()).h_full().border_l_1().border_color(palette.border)
                    .flex().flex_col().items_center().justify_center().text_color(palette.header_muted)
                    .child(match self.preview.password() { Some(password) => self.original_password_card(password, window, cx), None => self.preview_status(theme) })))
                .when_some(self.preview.pdf().cloned().filter(|_| show_original), |row, pdf| row.child(div().track_focus(&self.preview_region).on_action(cx.listener(Self::next_comment)).on_action(cx.listener(Self::previous_comment)).when(!narrow_preview, |v| v.w(px(content_width * (1. - split)))).when(narrow_preview, |v| v.flex_1()).flex_shrink_0().min_w_0().h_full().flex().flex_col().border_l_1().border_color(palette.border)
                    .child(div().flex_1().min_h_0().child(if pdf.read(cx).is_locked() { div().p_6().child(text::PDF_PASSWORD_PROTECTED).into_any_element() } else { pdf.into_any_element() }))
                    .children(preview_notice)))
)
            .children(setup_notice.filter(|_| !(show_markdown && self.lifecycle.source_only() && self.import.ocr_required().is_some()))).children(error_notice).children(refusal_notice)
            // The word popup anchors to the editor, hidden under before/after.
            .children(self.pii_popup(cx).filter(|_| self.compare.is_none()))
            .children(self.replacement_tip(window, cx).filter(|_| self.compare.is_none()));
        // Sidebar collapsed: the header joins the title row and the document
        // sits in its own card below it.
        let content = if detached {
            div()
                .size_full()
                .flex()
                .flex_col()
                .text_color(palette.header_fg)
                .child(
                    div()
                        .track_focus(&self.toolbar_region)
                        .key_context("ToolbarRegion")
                        .child(self.header(window, cx)),
                )
                .child(
                    ui::card_surface(div().flex_1().min_h_0().ml(px(style::CARD_MARGIN)), theme)
                        .child(body),
                )
                .into_any_element()
        } else {
            body.into_any_element()
        };
        // Both header layouts share the document's focus and action scope.
        div()
            .size_full()
            .flex()
            .flex_col()
            // Tab between controls is the workspace's: it knows the F6
            // regions.
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(
                cx.listener(|this, _: &crate::review::ExpandAllIdentities, _, cx| {
                    if this.replacements_open() {
                        this.set_all_identities_expanded(true, cx)
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::review::CollapseAllIdentities, _, cx| {
                    if this.replacements_open() {
                        this.set_all_identities_expanded(false, cx)
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &TogglePreview, window, cx| {
                if this.compare.is_none() {
                    this.toggle_preview(window, cx)
                }
            }))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::import))
            .on_action(cx.listener(Self::copy_markdown))
            .on_action(cx.listener(Self::copy_for_ai))
            .on_action(cx.listener(Self::paste_ai_response))
            .on_action(
                cx.listener(|this, _: &crate::document::ToggleCompare, window, cx| {
                    this.toggle_compare(window, cx)
                }),
            )
            // View › Edit, Original and Pseudonymized: the view switcher's
            // segments.
            .on_action(cx.listener(|this, _: &ShowEditView, window, cx| {
                this.set_view_mode(None, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowOriginalView, window, cx| {
                this.set_view_mode(Some(crate::review::CompareMode::Original), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowPseudonymizedView, window, cx| {
                this.set_view_mode(Some(crate::review::CompareMode::Pseudonymized), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowReplacements, window, cx| {
                this.show_replacements(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowNotes, window, cx| {
                if this.can_copy_markdown() && this.notes_available() {
                    this.show_notes(window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &OpenSourceReply, _, cx| this.open_source_reply(cx)))
            .on_action(cx.listener(Self::pseudonymize_clipboard))
            .on_action(cx.listener(Self::restore_clipboard))
            .on_action(cx.listener(Self::pseudonymize))
            .on_action(cx.listener(Self::add_pii_candidate))
            .on_action(cx.listener(Self::add_pii_candidate_as))
            .on_action(cx.listener(Self::apply_all_pii))
            .on_action(cx.listener(|this, _: &PiiReviewCandidate, window, cx| {
                this.step_pii_highlight(false, true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PiiNextCandidate, window, cx| {
                this.step_pii_highlight(false, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PiiPreviousCandidate, window, cx| {
                this.step_pii_highlight(true, false, window, cx)
            }))
            .on_action(cx.listener(Self::setup_ocr))
            .on_action(
                cx.listener(|this, _: &RunOcr, window, cx| this.ocr_action(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ExtractNative, window, cx| {
                    this.ocr_action(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &DismissImportWarning, _, cx| {
                this.session.warning = None;
                cx.notify();
            }))
            .on_action(cx.listener(|_, _: &New, _, cx| {
                cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::New))
            }))
            .on_action(cx.listener(|this, _: &Save, _, cx| this.save_now(cx)))
            .on_action(
                cx.listener(|this, _: &ExportMarkdown, window, cx| {
                    this.export_markdown(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &ClosePdf, window, cx| {
                this.toggle_preview(window, cx);
                if this.lifecycle.source_only() {
                    window.focus(&this.focus, cx);
                } else {
                    window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                }
                cx.notify();
            }))
            .on_action(cx.listener(Self::find_markdown))
            .on_action(cx.listener(Self::find_replace_markdown))
            .on_action(cx.listener(Self::replace_next_markdown))
            .on_action(cx.listener(Self::replace_all_markdown))
            .on_action(cx.listener(Self::toggle_markdown_match_case))
            .on_action(cx.listener(Self::toggle_markdown_whole_word))
            .on_action(cx.listener(Self::next_misspelling))
            .on_action(cx.listener(Self::previous_misspelling))
            .on_action(cx.listener(Self::toggle_document_spelling))
            .on_action(cx.listener(Self::find_next_markdown))
            .on_action(cx.listener(Self::find_previous_markdown))
            .on_action(cx.listener(Self::close_markdown_search))
            .on_action(cx.listener(Self::retry_preview))
            .on_action(
                cx.listener(|this, _: &ZoomIn, _, cx| this.set_zoom(this.zoom + ZOOM_STEP, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ZoomOut, _, cx| this.set_zoom(this.zoom - ZOOM_STEP, cx)),
            )
            .on_action(cx.listener(|this, _: &ZoomReset, _, cx| this.set_zoom(1., cx)))
            .child(content)
    }
}

/// A find-bar control: a focusable button named for assistive technology,
/// whose tooltip shows its shortcut.
pub(super) fn search_bar_button(
    id: &'static str,
    content: impl IntoElement,
    label: &'static str,
    shortcut: &'static str,
    theme: Theme,
) -> ui::Control {
    let palette = theme.pdf_style();
    crate::ui::focus_ring(id, theme)
        .when(cfg!(test), |v| v.debug_selector(move || id.into()))
        .role(gpui::Role::Button)
        .key_context("UiControl")
        .tab_index(0)
        .aria_label(label)
        .aria_keyshortcuts(shortcut)
        .tooltip(style::tooltip(format!("{label}  {shortcut}"), theme))
        .flex_shrink_0()
        .rounded_md()
        .px_2()
        .h(px(28.))
        .flex()
        .items_center()
        .cursor_pointer()
        .text_color(palette.header_muted)
        .hover(|view| view.bg(palette.placeholder_bg))
        .child(content)
}

/// The macOS or the Windows/Linux spelling of a shortcut.
pub(super) const fn chord(mac: &'static str, other: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        mac
    } else {
        other
    }
}

pub(super) fn toggled(on: bool) -> gpui::Toggled {
    if on {
        gpui::Toggled::True
    } else {
        gpui::Toggled::False
    }
}
