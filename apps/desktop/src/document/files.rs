//! Files in and out of a tab: Open and its file dialog, a source or a
//! conversion taking the tab over, Export Markdown, where links resolve,
//! and the name the window title shows.
use super::{DocumentView, LastFolder, Next, document_lifecycle::LifecycleEffect};
use crate::document::text::{self, chooser_unavailable};
use crate::workspace::{self, Open};
use gpui::{App, Context, Focusable, PathPromptOptions, Window, px};
use mdoc_store::document::{self, Document};
use std::path::PathBuf;

impl DocumentView {
    pub(crate) fn display_name(&self) -> String {
        if self.lifecycle.source_only() {
            self.preview
                .source()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| text::SOURCE.into())
        } else {
            self.session.display_name()
        }
    }

    pub(crate) fn open_source(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.lifecycle.show_source();
        self.preview.show_source(path.clone());
        self.open_path(path, window, cx);
        if self.active {
            window.focus(&self.focus, cx);
        }
    }

    /// The window title while this tab shows; the workspace sets it.
    pub(crate) fn window_title(&self) -> String {
        text::window_title(self.display_name())
    }

    /// Replace or close the document, unless one of its dialogs is open.
    pub(crate) fn request(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(LifecycleEffect::Proceed) = self.lifecycle.request() {
            self.proceed(next, window, cx);
        }
    }

    pub(crate) fn proceed(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_feedback = None;
        match next {
            Next::Import(imported) => {
                let imported = *imported;
                let automatic = self.import.imported();
                self.lifecycle.converted(automatic);
                // A PDF shown before its file changed loads again, so its
                // pages and rail are what the import read.
                // An image's Original is always the PDF this import built:
                // the one shown may be of an earlier version of the file.
                let retain_preview = !imported.is_image
                    && self.preview.source() == Some(&imported.source.path)
                    && (self.preview.pdf().is_some() || self.preview.loading())
                    && !self
                        .preview
                        .shows_other_annotations(&imported.source.path, &imported.annotations);
                // A Vault document stays itself: the conversion fills its text.
                let kept =
                    (self.session.document.vault.clone()).map(|id| (id, self.session.name.clone()));
                self.session
                    .import(imported.source.path.clone(), imported.warning);
                if let Some((id, name)) = kept {
                    self.session.document = Document::in_vault(id, String::new());
                    self.session.name = name;
                    self.session.write_due = true;
                }
                self.session.left_out = imported.left_out.line(true);
                self.session.comments_left_out = imported.left_out.comments_not_shown();
                self.preview
                    .imported(&imported.source, imported.comments, imported.annotations);
                self.session.ocr_configuration = imported.ocr_configuration;
                self.reset_pii(cx);
                if !retain_preview {
                    self.close_preview(window, cx);
                }
                self.replace_images(self.save_directory(), window, cx);
                self.editor
                    .update(cx, |editor, cx| editor.set_text(imported.markdown, cx));
                self.reset_markdown_search(cx);
                if self.active {
                    window.focus(&self.editor.read(cx).focus_handle(cx), cx);
                }
                self.error = None;
                if !retain_preview {
                    self.preview.clear_message();
                }
                if retain_preview {
                    self.preview.show();
                } else if imported.is_image {
                    // The import's own PDF of the image is its Original.
                    let path = imported.source.path.clone();
                    self.begin_preview(path.clone());
                    self.show_image_pdf(path, imported.source.bytes().to_vec(), window, cx);
                } else if imported.is_pdf {
                    self.open_pdf(imported.source.path, window, cx);
                } else if imported.is_docx {
                    self.open_docx(imported.source.path, window, cx);
                } else {
                    self.preview.no_preview(imported.source.path);
                }
                self.scroll.set_offset(gpui::point(px(0.), px(0.)));
            }
        }
        cx.notify();
    }

    /// Where relative links and images resolve. A Vault document without a
    /// Source has no folder: they resolve nowhere, as the Vault reads nothing
    /// outside itself.
    pub(crate) fn save_directory(&self) -> PathBuf {
        if self.session.document.vault.is_some() && self.session.source.is_none() {
            return PathBuf::new();
        }
        self.session.directory()
    }

    /// Where Export Markdown starts: the document's folder, else the last
    /// folder a file was imported from or exported to this run, else
    /// Documents (#180).
    fn export_directory(&self, cx: &App) -> PathBuf {
        let last = LastFolder::get(cx);
        // A Vault document's Source is a viewing copy in the app cache,
        // removed with its tab: never a place to export to.
        if self.session.document.vault.is_some() {
            return mdoc_store::document_session::DocumentSession::default().export_directory(last);
        }
        self.session.export_directory(last)
    }

    pub(crate) fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
        {
            self.open_pdf(path, window, cx);
            self.error = None;
            cx.notify();
        } else if mdoc_import::image_pdf::ImageFormat::from_path(&path).is_some() {
            self.open_image(path, window, cx);
            self.error = None;
            cx.notify();
        } else if path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("docx"))
        {
            self.open_docx(path, window, cx);
            cx.notify();
        } else if document::is_markdown(&path) {
            cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::Open(vec![
                path,
            ])));
        } else if self.lifecycle.source_only() {
            let generation = self.begin_preview(path.clone());
            // A renamed image is known by its content, by the same bounded
            // probe admission asks (a generic HEIF's `meta` box may lie far
            // into the file), so its Original shows.
            let task = cx.background_executor().spawn({
                let path = path.clone();
                async move {
                    mdoc_import::image_pdf::format_of_file(&path).map(|format| format.is_some())
                }
            });
            cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                let _ = this.update_in(cx, |this, window, cx| {
                    if matches!(result, Ok(true)) && this.preview.generation() == generation {
                        this.open_image(path, window, cx);
                    } else if this.preview.checked(generation, result.is_ok()) {
                        cx.notify();
                    }
                });
            })
            .detach();
            cx.notify();
        } else {
            self.error = Some(text::CHOOSE_A_SOURCE.into());
            cx.notify();
        }
    }

    pub(crate) fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let Some(LifecycleEffect::AskForFiles) = self.lifecycle.open_files() else {
            return;
        };
        let paths = crate::app::native_picker::paths(
            cx,
            PathPromptOptions {
                files: true,
                directories: false,
                multiple: true,
                prompt: Some(text::OPEN_ONE_OR_MORE.into()),
            },
        );
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.lifecycle.end_prompt();
                match result {
                    Ok(Some(paths)) => {
                        cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::Open(paths)))
                    }
                    Err(error) => {
                        this.error = Some(chooser_unavailable(&error));
                        cx.notify();
                    }
                    _ => {}
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    /// Export Markdown: a copy of the text in a file the user picks. The
    /// document stays where it is.
    pub(crate) fn export_markdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(LifecycleEffect::AskForPath) = self.lifecycle.export() else {
            return;
        };
        let mut name = self.session.suggested_name();
        if !document::is_markdown(std::path::Path::new(&name)) {
            name.push_str(".md");
        }
        let directory = self.export_directory(cx);
        let path = crate::app::native_picker::new_path(cx, &directory, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let result = path.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.lifecycle.end_prompt();
                match result {
                    Ok(Some(mut path)) => {
                        if path.extension().is_none() {
                            path.set_extension("md");
                        }
                        let source = this.session.source.as_deref();
                        // The viewing copy's folder goes with the tab.
                        let viewing = (this.session.document.vault.is_some())
                            .then_some(source)
                            .flatten()
                            .and_then(std::path::Path::parent);
                        if !document::is_markdown(&path) {
                            this.error = Some(text::EXPORT_EXTENSION.into());
                        } else if viewing.is_some_and(|dir| path.starts_with(dir)) {
                            this.error = Some(text::CHOOSE_YOUR_OWN_FOLDER.into());
                        } else if source.is_some_and(|source| {
                            mdoc_store::session_store::identity(source)
                                == mdoc_store::session_store::identity(&path)
                        }) {
                            this.error = Some(text::PRESERVE_THE_SOURCE.into());
                        } else if let Err(error) =
                            document::export(&path, this.editor.read(cx).text())
                        {
                            this.error = Some(text::could_not_export(error));
                        } else {
                            this.error = None;
                            LastFolder::note(&path, cx);
                        }
                    }
                    Err(error) => this.error = Some(chooser_unavailable(&error)),
                    _ => {}
                }
                cx.notify();
                this.resume_import(window, cx);
            });
        })
        .detach();
    }
}
