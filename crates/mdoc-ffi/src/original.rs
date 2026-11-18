//! A DOCX file's Original pane on iOS: docxide renders the source into a
//! temporary PDF in this process (desktop's worker is a child process, which
//! iOS forbids), under the worker's input limits. The platform shows the PDF
//! and reads its comment marks back with its own PDF reader; the core turns
//! them into anchors. The PDF goes when the platform drops the object.
use std::{
    cell::RefCell,
    path::Path,
    sync::{Arc, Mutex},
};

use mdoc_import::docx_preview::{self, AnchorRect, DocxPreview, LinkMark, RenderOptions};

use crate::CoreError;

/// Reads the rendered PDF's comment marks: its `/Link` annotations whose
/// URI starts with `mdoc-comment:`. Called on the render's thread.
#[uniffi::export(callback_interface)]
pub trait CommentMarkReader: Send + Sync {
    fn comment_marks(&self, pdf_path: String) -> Vec<CommentMark>;
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

/// A rendered DOCX original. Its PDF lives as long as the object.
#[derive(uniffi::Object)]
pub struct DocxOriginal {
    preview: DocxPreview,
}

// One render at a time: two would hold two documents' layouts in memory.
static RENDER: Mutex<()> = Mutex::new(());

thread_local! {
    static MARKS: RefCell<Option<Box<dyn CommentMarkReader>>> = const { RefCell::new(None) };
}

fn read_marks(pdf: &Path) -> Result<Vec<LinkMark>, String> {
    MARKS.with_borrow(|reader| {
        let reader = reader.as_ref().ok_or("no comment mark reader")?;
        Ok(reader
            .comment_marks(pdf.to_string_lossy().into_owned())
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
    })
}

/// The left-out count of `source`'s comments, for an original that could
/// not render: the import counted them on the original instead of the
/// left-out line, so nothing is lost without a note.
#[uniffi::export]
pub fn docx_comments_not_shown(source: String) -> Option<String> {
    mdoc_import::import::source_comments_not_shown(Path::new(&source))
}

#[uniffi::export]
impl DocxOriginal {
    /// Renders `source` into a new folder under `folder`, whose file
    /// protection the PDF inherits. Blocks for the whole render: call it off
    /// the main thread.
    #[uniffi::constructor]
    pub fn render(
        source: String,
        folder: String,
        marks: Box<dyn CommentMarkReader>,
    ) -> Result<Arc<Self>, CoreError> {
        let _one = RENDER.lock().unwrap_or_else(|e| e.into_inner());
        MARKS.set(Some(marks));
        // Phones offer no hidden text: the page as Word prints it.
        let preview = docx_preview::render_in(
            Path::new(&folder),
            Path::new(&source),
            read_marks,
            RenderOptions::default(),
        );
        MARKS.set(None);
        let preview = preview.map_err(|error| CoreError::Import {
            message: error.to_string(),
        })?;
        Ok(Arc::new(Self { preview }))
    }

    pub fn pdf_path(&self) -> String {
        self.preview.pdf_path.to_string_lossy().into_owned()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Scratch;
    use std::path::PathBuf;

    fn fixture(name: &str) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/docx-preview")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    /// Hands back one mark per page it is asked about, like a PDF reader
    /// that found marker `0` at the top of page 1.
    struct OneMark;
    impl CommentMarkReader for OneMark {
        fn comment_marks(&self, pdf_path: String) -> Vec<CommentMark> {
            assert!(
                Path::new(&pdf_path).is_file(),
                "asked before the PDF exists"
            );
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

    #[test]
    fn renders_a_docx_with_its_comments_into_the_folder_and_removes_it_when_dropped() {
        let folder = Scratch::new();
        let original = DocxOriginal::render(
            fixture("comments.docx"),
            folder.path().to_string_lossy().into_owned(),
            Box::new(OneMark),
        )
        .unwrap();
        let pdf = PathBuf::from(original.pdf_path());
        assert!(pdf.starts_with(folder.path()));
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
        assert!(!original.comments().is_empty());
        let anchor = &original.anchors()[0];
        assert_eq!(anchor.page, 0);
        assert_eq!(anchor.rects[0].x, 0.1);
        drop(original);
        // The folder goes on a cleanup thread.
        let started = std::time::Instant::now();
        while pdf.parent().unwrap().exists() {
            assert!(
                started.elapsed().as_secs() < 5,
                "the render's folder stayed"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn a_failed_original_still_counts_its_comments() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/import/handmade-hidden-content.docx");
        let folder = Scratch::new();
        // Tracked changes: the preview refuses it.
        assert!(
            DocxOriginal::render(
                source.to_string_lossy().into_owned(),
                folder.path().to_string_lossy().into_owned(),
                Box::new(OneMark),
            )
            .is_err()
        );
        assert_eq!(
            docx_comments_not_shown(source.to_string_lossy().into_owned()).as_deref(),
            Some("2 comments not shown.")
        );
    }

    #[test]
    fn a_damaged_docx_is_an_import_error() {
        let folder = Scratch::new();
        let source = folder.path().join("broken.docx");
        std::fs::write(&source, b"not a zip").unwrap();
        let error = DocxOriginal::render(
            source.to_string_lossy().into_owned(),
            folder.path().to_string_lossy().into_owned(),
            Box::new(OneMark),
        )
        .err()
        .unwrap();
        assert!(matches!(error, CoreError::Import { .. }), "{error:?}");
    }
}
