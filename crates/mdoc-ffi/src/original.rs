//! A DOCX file's Original on iOS. The core draws it in memory, and the
//! platform reads the PDF's comment marks with PDFKit on the render thread.
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use mdoc_import::{
    docx_preview::{self, AnchorRect, DocxPreview, LinkMark, PreviewError, RenderOptions},
    import::{Password, Source},
};

use crate::CoreError;

/// Reads the rendered PDF's comment marks: its `/Link` annotations whose
/// URI starts with `mdoc-comment:`. Called on the render's thread.
#[uniffi::export(callback_interface)]
pub trait CommentMarkReader: Send + Sync {
    fn comment_marks(&self, pdf: Vec<u8>) -> Vec<CommentMark>;
}

/// One comment mark: a `/Link` rectangle and its URI after the scheme.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CommentMark {
    /// 0-based page index.
    pub page: u32,
    pub rect: PageRect,
    pub target: String,
}

/// A rectangle in normalized page coordinates: 0..1 of the page's width and
/// height, top-left origin.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct PageRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A comment of the source document, as written: never scanned.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct OriginalComment {
    pub id: String,
    pub author: String,
    pub date: String,
    pub text: String,
    /// The commented passage's text.
    pub quote: String,
    /// The `id` of the comment this one replies to, if any.
    pub parent: Option<String>,
    /// The thread was marked done in Word.
    pub resolved: bool,
}

/// Where a comment sits on one rendered page, in document order.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CommentAnchor {
    /// The comment's `OriginalComment::id`.
    pub id: String,
    /// 0-based page index.
    pub page: u32,
    /// One rectangle per line the passage covers, in reading order.
    pub rects: Vec<PageRect>,
    /// A point comment: its one rectangle is a zero-width spot.
    pub point: bool,
}

/// An open document's unlocked DOCX, shared in Rust without sending its
/// decrypted bytes across the bridge.
#[derive(uniffi::Object)]
pub struct UnlockedDocx(Source);

impl UnlockedDocx {
    pub(crate) fn new(source: Source) -> Arc<Self> {
        Arc::new(Self(source))
    }
}

#[derive(uniffi::Enum)]
pub enum DocxInput {
    File {
        bytes: Vec<u8>,
        password: Option<String>,
    },
    Unlocked {
        source: Arc<UnlockedDocx>,
    },
}

#[uniffi::export]
pub fn docx_preview_max_input_bytes() -> u64 {
    docx_preview::MAX_INPUT_BYTES
}

/// A rendered DOCX original. Its PDF lives as long as the object.
#[derive(uniffi::Object)]
pub struct DocxOriginal {
    preview: DocxPreview,
    unlocked: Option<Arc<UnlockedDocx>>,
}

// One render at a time: two would hold two documents' layouts in memory.
static RENDER: Mutex<()> = Mutex::new(());

#[uniffi::export]
impl DocxOriginal {
    /// Draws the DOCX bytes and reads the PDF's marks in memory. Call off
    /// the main thread. A reopened locked file asks via NeedsPassword.
    #[uniffi::constructor]
    pub fn render(
        input: DocxInput,
        marks: Box<dyn CommentMarkReader>,
    ) -> Result<Arc<Self>, CoreError> {
        let _one = RENDER.lock().unwrap_or_else(|e| e.into_inner());
        let (source, opened_file) = match input {
            DocxInput::File { bytes, password } => {
                let password = password.map(Password::new);
                let source = Source::new("original.docx".into(), None, bytes);
                if source.bytes().len() as u64 > docx_preview::MAX_INPUT_BYTES {
                    return Err(render_error(PreviewError::InputTooLarge, &source));
                }
                (source.unlock(password.as_ref())?, true)
            }
            DocxInput::Unlocked { source } => (source.0.clone(), false),
        };
        // Phones offer no hidden-text toggle: the page is as Word prints it.
        let preview = catch_unwind(AssertUnwindSafe(|| {
            docx_preview::render_in_process(
                &source,
                Arc::new(AtomicBool::new(false)),
                |pdf| {
                    Ok(marks
                        .comment_marks(pdf.as_ref().to_vec())
                        .into_iter()
                        .map(|mark| LinkMark {
                            page: mark.page as usize,
                            rect: AnchorRect {
                                x: mark.rect.x,
                                y: mark.rect.y,
                                w: mark.rect.w,
                                h: mark.rect.h,
                            },
                            target: mark.target,
                        })
                        .collect())
                },
                RenderOptions::default(),
                None,
            )
        }))
        .unwrap_or_else(|_| Err(PreviewError::Render("the renderer stopped".into())))
        .map_err(|error| render_error(error, &source))?;
        Ok(Arc::new(Self {
            preview,
            unlocked: (opened_file && source.unlocked()).then(|| UnlockedDocx::new(source)),
        }))
    }

    pub fn pdf_bytes(&self) -> Vec<u8> {
        self.preview.pdf.as_ref().to_vec()
    }

    /// The unlock obtained on the Original's password card. The document
    /// keeps it while open, including when the Original pane is hidden.
    pub fn unlocked_source(&self) -> Option<Arc<UnlockedDocx>> {
        self.unlocked.clone()
    }

    /// What the rendering left out or may show incompletely.
    pub fn warnings(&self) -> Vec<String> {
        self.preview.warnings.clone()
    }

    pub fn comments(&self) -> Vec<OriginalComment> {
        self.preview
            .comments
            .iter()
            .map(|c| OriginalComment {
                id: c.id.clone(),
                author: c.author.clone(),
                date: c.date.clone(),
                text: c.text.clone(),
                quote: c.anchor_text.clone(),
                parent: c.parent.clone(),
                resolved: c.resolved,
            })
            .collect()
    }

    pub fn anchors(&self) -> Vec<CommentAnchor> {
        self.preview
            .anchors
            .iter()
            .map(|a| CommentAnchor {
                id: a.id.clone(),
                page: a.page as u32,
                rects: a
                    .rects
                    .iter()
                    .map(|r| PageRect {
                        x: r.x,
                        y: r.y,
                        w: r.w,
                        h: r.h,
                    })
                    .collect(),
                point: a.point,
            })
            .collect()
    }
}

fn render_error(error: PreviewError, source: &Source) -> CoreError {
    let mut message = error.to_string();
    // The Original normally shows comments omitted from the Markdown.
    // If a DOCX can't draw, its failure still gives their count.
    if !matches!(error, PreviewError::InputTooLarge)
        && let Some(count) = mdoc_import::import::source_comments_not_shown(source)
    {
        message.push('\n');
        message.push_str(&count);
    }
    CoreError::Import { message }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OneMark;
    impl CommentMarkReader for OneMark {
        fn comment_marks(&self, pdf: Vec<u8>) -> Vec<CommentMark> {
            assert!(pdf.starts_with(b"%PDF-"));
            vec![CommentMark {
                page: 0,
                rect: PageRect {
                    x: 0.1,
                    y: 0.1,
                    w: 0.2,
                    h: 0.02,
                },
                target: "0".into(),
            }]
        }
    }

    fn render(bytes: Vec<u8>) -> Result<Arc<DocxOriginal>, CoreError> {
        DocxOriginal::render(
            DocxInput::File {
                bytes,
                password: None,
            },
            Box::new(OneMark),
        )
    }

    #[test]
    fn renders_pdf_bytes_and_comment_anchors() {
        let original =
            render(include_bytes!("../../../tests/fixtures/docx-preview/comments.docx").to_vec())
                .unwrap();
        assert!(original.pdf_bytes().starts_with(b"%PDF-"));
        assert!(!original.comments().is_empty());
        let anchor = &original.anchors()[0];
        assert_eq!(anchor.page, 0);
        assert_eq!(anchor.rects[0].x, 0.1);
    }

    #[test]
    fn an_ordinary_docx_that_cannot_render_still_counts_its_comments() {
        let plain = include_bytes!("../../../tests/fixtures/import/handmade-hidden-content.docx");
        let counts_comments = |result: Result<Arc<DocxOriginal>, CoreError>| {
            assert!(
                matches!(result, Err(CoreError::Import { message }) if message.contains("2 comments not shown."))
            );
        };
        counts_comments(render(plain.to_vec()));

        let password = "fictional-pass";
        let bytes = craft_offcrypto::encrypt_with_spin_count(plain, password, 1000).unwrap();
        counts_comments(DocxOriginal::render(
            DocxInput::File {
                bytes,
                password: Some(password.into()),
            },
            Box::new(OneMark),
        ));
    }

    #[test]
    fn a_damaged_docx_is_an_import_error() {
        let result = render(b"not a zip".to_vec());
        assert!(matches!(result, Err(CoreError::Import { .. })));
    }
}
