//! A file-based Markdown editor with a side-by-side PDF viewer.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
mod document;
mod images;
mod import;
mod style;
#[cfg(test)]
mod ui_tests;

use document::Document;
use gpui::{
    App, Bounds, Context, Entity, Focusable, KeyBinding, Menu, MenuItem, PathPromptOptions,
    PromptLevel, ScrollHandle, Subscription, Window, WindowBounds, WindowOptions, actions, div,
    prelude::*, px, size,
};
use gpui_pdf::PdfView;
use mdoc_editor::{EditorEvent, EditorState};
use std::{cell::Cell, path::PathBuf, rc::Rc};
use style::Theme;

actions!(
    mdoc,
    [
        New,
        Open,
        Import,
        DismissImportWarning,
        Save,
        SaveAs,
        Close,
        ClosePdf,
        ToggleTheme
    ]
);

#[derive(Clone)]
enum Next {
    New,
    Open(PathBuf),
    Import(import::Imported),
    Close,
}

struct Workspace {
    theme: Rc<Cell<Theme>>,
    editor: Entity<EditorState>,
    document: Document,
    pdf: Option<Entity<PdfView>>,
    scroll: ScrollHandle,
    error: Option<String>,
    prompting: bool,
    importing: bool,
    document_generation: u64,
    pending_import: Option<(u64, Result<import::Imported, String>)>,
    import_source: Option<PathBuf>,
    import_warning: Option<String>,
    _subscription: Subscription,
    pdf_subscription: Option<Subscription>,
}

impl Workspace {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            let mut editor =
                EditorState::new(window, cx).with_placeholder("Start writing Markdown…");
            editor.set_markdown_style(style::markdown_style(Theme::default()), cx);
            editor.set_block_chip_provider(|src| {
                gpui_pdf::is_pdf(src).then(|| src.to_owned().into())
            });
            editor
        });
        images::install(&editor, Document::default().directory(), cx);
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        let subscription =
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| match event {
                EditorEvent::Changed => {
                    this.update_title(window, cx);
                    cx.notify();
                }
                EditorEvent::OpenLink(src) => {
                    if src.starts_with("https://")
                        || src.starts_with("http://")
                        || src.starts_with("mailto:")
                    {
                        cx.open_url(src);
                    } else if let Some(path) = document::local_path(src, &this.save_directory()) {
                        this.open_path(path, window, cx);
                    }
                }
                _ => {}
            });
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                if this.dirty(cx) {
                    this.request(Next::Close, window, cx);
                    false
                } else {
                    true
                }
            })
            .unwrap_or(true)
        });
        Self {
            theme: Rc::new(Cell::new(Theme::default())),
            editor,
            document: Document::default(),
            pdf: None,
            scroll: ScrollHandle::new(),
            error: None,
            prompting: false,
            importing: false,
            document_generation: 0,
            pending_import: None,
            import_source: None,
            import_warning: None,
            _subscription: subscription,
            pdf_subscription: None,
        }
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, _: &mut Window, cx: &mut Context<Self>) {
        let theme = self.theme.get().toggle();
        self.theme.set(theme);
        self.editor.update(cx, |editor, cx| {
            editor.set_markdown_style(style::markdown_style(theme), cx)
        });
        if let Some(pdf) = &self.pdf {
            pdf.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
    }

    fn dirty(&self, cx: &App) -> bool {
        (self.document.path.is_none() && self.import_source.is_some())
            || self.editor.read(cx).text() != self.document.saved
    }

    fn update_title(&self, window: &mut Window, cx: &App) {
        let suggested = self.import_source.as_ref().map(|p| p.with_extension("md"));
        let name = self
            .document
            .path
            .as_deref()
            .or(suggested.as_deref())
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.md".into());
        window.set_window_title(&format!(
            "{}{} — mdoc",
            if self.dirty(cx) { "• " } else { "" },
            name
        ));
    }

    fn request(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompting {
            return;
        }
        if !self.dirty(cx) {
            self.proceed(next, window, cx);
            return;
        }
        self.prompting = true;
        let answer = window.prompt(
            PromptLevel::Warning,
            "Save changes to this document?",
            Some("Unsaved changes will be lost if you discard them."),
            &["Save", "Cancel", "Discard"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                match answer {
                    Some(0) => this.save(false, Some(next), window, cx),
                    Some(2) => this.proceed(next, window, cx),
                    _ => {}
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    fn proceed(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        match next {
            Next::Close => {
                self.changed_document();
                window.remove_window();
            }
            Next::New => {
                self.changed_document();
                self.document = Document::default();
                images::install(&self.editor, self.document.directory(), cx);
                self.editor.update(cx, |editor, cx| editor.set_text("", cx));
                self.error = None;
                self.scroll.set_offset(gpui::point(px(0.), px(0.)));
                self.update_title(window, cx);
            }
            Next::Open(path) => match Document::open(path) {
                Ok(document) => {
                    self.changed_document();
                    self.editor
                        .update(cx, |editor, cx| editor.set_text(document.saved.clone(), cx));
                    self.document = document;
                    images::install(&self.editor, self.document.directory(), cx);
                    self.error = None;
                    self.scroll.set_offset(gpui::point(px(0.), px(0.)));
                    self.update_title(window, cx);
                }
                Err(error) => self.error = Some(format!("Could not open document: {error}")),
            },
            Next::Import(imported) => {
                self.changed_document();
                self.document = Document::default();
                self.import_source = Some(imported.source.clone());
                self.import_warning = imported.warning;
                images::install(&self.editor, self.save_directory(), cx);
                self.editor
                    .update(cx, |editor, cx| editor.set_text(imported.markdown, cx));
                window.focus(&self.editor.read(cx).focus_handle(cx), cx);
                self.error = None;
                if imported.is_pdf {
                    self.open_pdf(imported.source, cx);
                }
                self.scroll.set_offset(gpui::point(px(0.), px(0.)));
                self.update_title(window, cx);
            }
        }
        cx.notify();
    }

    fn changed_document(&mut self) {
        self.document_generation += 1;
        self.import_source = None;
        self.import_warning = None;
    }

    fn import(&mut self, _: &Import, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompting || self.importing {
            return;
        }
        self.prompting = true;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import as Markdown — PDF, Office, OpenDocument, RTF, EPUB, CSV".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                if let Ok(Ok(Some(paths))) = result
                    && let Some(path) = paths.into_iter().next()
                {
                    this.start_import(path, window, cx);
                }
            });
        })
        .detach();
    }

    fn start_import(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.importing {
            return;
        }
        self.importing = true;
        self.error = None;
        let generation = self.document_generation;
        let task = cx
            .background_executor()
            .spawn(async move { import::convert(&path) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.pending_import = Some((generation, result));
                this.resume_import(window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn resume_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompting {
            return;
        }
        if let Some((generation, result)) = self.pending_import.take() {
            self.importing = false;
            if generation == self.document_generation {
                match result {
                    Ok(imported) => self.request(Next::Import(imported), window, cx),
                    Err(error) => self.error = Some(error),
                }
            }
            cx.notify();
        }
    }

    fn save_directory(&self) -> PathBuf {
        if self.document.path.is_none()
            && let Some(parent) = self.import_source.as_ref().and_then(|p| p.parent())
        {
            return parent.to_path_buf();
        }
        self.document.directory()
    }

    fn open_pdf(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let theme = self.theme.clone();
        self.pdf = Some(cx.new(|cx| {
            PdfView::new(
                path,
                Rc::new(move || theme.get().pdf_style()),
                Rc::new(|| 1.0),
                cx,
            )
        }));
        self.pdf_subscription = self
            .pdf
            .as_ref()
            .map(|pdf| cx.observe(pdf, |_, _, cx| cx.notify()));
    }

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
        {
            self.open_pdf(path, cx);
            self.error = None;
            cx.notify();
        } else if document::is_markdown(&path) {
            self.request(Next::Open(path), window, cx);
        } else {
            self.error =
                Some("Choose a Markdown (.md, .markdown, .mdown, .txt) or PDF file.".into());
            cx.notify();
        }
    }

    fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompting {
            return;
        }
        self.prompting = true;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Markdown or PDF".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                if let Ok(Ok(Some(paths))) = result
                    && let Some(path) = paths.into_iter().next()
                {
                    this.open_path(path, window, cx);
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    fn save(
        &mut self,
        save_as: bool,
        next: Option<Next>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.prompting {
            return;
        }
        if !save_as && let Some(path) = self.document.path.clone() {
            self.write(path, next, window, cx);
            return;
        }
        self.prompting = true;
        let suggested = self.import_source.as_ref().map(|p| p.with_extension("md"));
        let name = suggested
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled.md");
        let path = cx.prompt_for_new_path(&self.save_directory(), Some(name));
        cx.spawn_in(window, async move |this, cx| {
            let result = path.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                if let Ok(Ok(Some(mut path))) = result {
                    if path.extension().is_none() {
                        path.set_extension("md");
                    }
                    if !document::is_markdown(&path) {
                        this.error = Some(
                            "Save Markdown with a .md, .markdown, .mdown, or .txt extension."
                                .into(),
                        );
                        cx.notify();
                        this.resume_import(window, cx);
                        return;
                    }
                    this.write(path, next, window, cx);
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    fn write(
        &mut self,
        path: PathBuf,
        next: Option<Next>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(source) = &self.import_source
            && (std::path::absolute(&path).ok().as_ref() == Some(source)
                || (path.exists() && path.canonicalize().ok() == source.canonicalize().ok()))
        {
            self.error = Some("Choose a different path to preserve the imported source.".into());
            cx.notify();
            return;
        }
        let old_directory = self.save_directory();
        match self.document.save(path, self.editor.read(cx).text()) {
            Ok(()) => {
                if old_directory != self.document.directory() {
                    images::install(&self.editor, self.document.directory(), cx);
                }
                self.error = None;
                self.update_title(window, cx);
                if let Some(next) = next {
                    self.proceed(next, window, cx);
                }
            }
            Err(error) => self.error = Some(format!("Could not save: {error}")),
        }
        cx.notify();
    }
}

fn button(label: &'static str, action: impl gpui::Action, theme: Theme) -> impl IntoElement {
    div()
        .id(label)
        .px_3()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .hover(move |s| s.bg(theme.pdf_style().placeholder_bg))
        .child(label)
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        div().size_full().flex().flex_col().bg(palette.bg).text_color(palette.header_fg).text_size(px(16.))
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::import))
            .on_action(cx.listener(|this, _: &DismissImportWarning, _, cx| { this.import_warning = None; cx.notify(); }))
            .on_action(cx.listener(|this, _: &New, window, cx| this.request(Next::New, window, cx)))
            .on_action(cx.listener(|this, _: &Save, window, cx| this.save(false, None, window, cx)))
            .on_action(cx.listener(|this, _: &SaveAs, window, cx| this.save(true, None, window, cx)))
            .on_action(cx.listener(|this, _: &Close, window, cx| this.request(Next::Close, window, cx)))
            .on_action(cx.listener(|this, _: &ClosePdf, window, cx| { this.pdf = None; this.pdf_subscription = None; window.focus(&this.editor.read(cx).focus_handle(cx), cx); cx.notify(); }))
            .child(div().flex().items_center().gap_2().p_2().text_size(px(13.)).border_b_1().border_color(palette.border)
                .child(button("New", New, theme)).child(button("Open…", Open, theme)).child(button("Save", Save, theme)).child(button("Save As…", SaveAs, theme))
                .child(if self.importing { div().child("Converting…").into_any_element() } else { button("Import as Markdown…", Import, theme).into_any_element() })
                .child(div().flex_1())
                .child(button(theme.toggle_label(), ToggleTheme, theme))
                .child(if self.dirty(cx) { "Unsaved changes" } else { "Markdown · WYSIWYG" })
                .when(self.pdf.is_some(), |bar| bar.child(button("Close PDF", ClosePdf, theme))))
            .when_some(self.import_warning.clone(), |view, warning| view.child(div().flex().items_center().gap_2().p_2().border_b_1().border_color(palette.border)
                .child(div().id("import-warning").flex_1().min_w_0().max_h(px(96.)).overflow_y_scroll().child(warning))
                .child(button("Dismiss", DismissImportWarning, theme))))
            .child(div().flex().flex_1().min_h_0()
                .child(div().id("document-scroll").flex_1().min_w_0().h_full().overflow_y_scroll().track_scroll(&self.scroll).p_6().child(self.editor.clone()))
                .when_some(self.pdf.clone(), |row, pdf| row.child(div().w_1_2().h_full().border_l_1().border_color(palette.border).child(if pdf.read(cx).is_locked() { div().p_6().child("This PDF is password-protected. Open an unlocked copy to view it here.").into_any_element() } else { pdf.into_any_element() }))))
            .when_some(self.error.clone(), |view, error| view.child(div().p_2().bg(theme.error_bg()).child(error)))
    }
}

fn main() {
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    gpui_platform::application().run(move |cx: &mut App| {
        mdoc_editor::bind_keys(cx);
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        cx.bind_keys([
            KeyBinding::new(&format!("{modifier}-n"), New, None),
            KeyBinding::new(&format!("{modifier}-o"), Open, None),
            KeyBinding::new(&format!("{modifier}-shift-i"), Import, None),
            KeyBinding::new(&format!("{modifier}-s"), Save, None),
            KeyBinding::new(&format!("{modifier}-shift-s"), SaveAs, None),
            KeyBinding::new(&format!("{modifier}-w"), Close, None),
            KeyBinding::new(&format!("{modifier}-q"), Close, None),
        ]);
        cx.set_menus(vec![Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New", New),
                MenuItem::action("Open…", Open),
                MenuItem::action("Import as Markdown…", Import),
                MenuItem::action("Save", Save),
                MenuItem::action("Save As…", SaveAs),
                MenuItem::separator(),
                MenuItem::action("Quit", Close),
            ],
            disabled: false,
        }]);
        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let mut workspace = Workspace::new(window, cx);
                    workspace.update_title(window, cx);
                    if let Some(path) = initial {
                        workspace.open_path(path, window, cx);
                    }
                    workspace
                })
            },
        )
        .expect("Could not open the editor window");
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
