//! OCR that keeps finished pages (ADR 0085 Long jobs). An import with OCR
//! recognizes a PDF one page at a time and keeps each finished page under a
//! folder of the caller's, keyed by the file's digest and the OCR
//! configuration, so a cancelled or stopped import resumes with the pages
//! still to do. Each page goes through pdf-inspector's own page selection
//! with the desktop's OCR options, so a page reads as it does in a
//! whole-document run. The folder is removed once the import completes.
//! Desktop imports stay on [`crate::import::prepare_cancellable`].
use crate::import::{
    ImportError, Imported, OcrFailure, check_cancel, pdf_error, pdf_warning, read_pdf,
};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

/// Bumped when a kept page's meaning changes, so older pages are not reused.
const FORMAT: u32 = 1;

/// The folder that keeps finished pages, one subfolder per file and OCR
/// configuration.
#[derive(Clone, Debug)]
pub struct KeptPages {
    root: PathBuf,
}

/// One recognized page, as kept between runs.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Page {
    /// 1-based.
    page: u32,
    /// The page's Markdown as pdf-inspector assembles a one-page selection.
    markdown: String,
    /// OCR found the page weak, or warned about it: the import's warning
    /// names it.
    review: bool,
}

impl KeptPages {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Remove every page kept for the file at `source`, under any OCR
    /// configuration. Call it while the file still exists: pages are found
    /// by its digest.
    pub fn discard(&self, source: &Path) -> std::io::Result<()> {
        let digest = match std::fs::read(source) {
            Ok(bytes) => digest(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let prefix = format!("{digest}-");
        for entry in entries {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                remove_folder(&entry.path())?;
            }
        }
        Ok(())
    }

    fn folder(&self, digest: &str, config: &mdoc_store::settings::OcrConfig) -> Folder {
        let manifest = crate::ocr::manifest::for_model(config.model);
        let key = format!(
            "{FORMAT}\n{}\n{}\n{}",
            serde_json::to_string(config).unwrap_or_default(),
            manifest.id,
            manifest.revision
        );
        Folder(
            self.root
                .join(format!("{digest}-{}", &self::digest(key.as_bytes())[..16])),
        )
    }
}

/// One file's kept pages under one OCR configuration.
struct Folder(PathBuf);

impl Folder {
    fn path(&self, page: u32) -> PathBuf {
        self.0.join(format!("{page:05}.json"))
    }

    /// Page `page`, when it was kept and reads back whole.
    fn load(&self, page: u32) -> Option<Page> {
        let json = std::fs::read(self.path(page)).ok()?;
        serde_json::from_slice::<Page>(&json)
            .ok()
            .filter(|kept| kept.page == page)
    }

    /// Keep `page`, whole or not at all. Keeping is best effort: a page
    /// that could not be written is recognized again on resume.
    fn store(&self, page: &Page) {
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(&self.0)?;
            let path = self.path(page.page);
            let partial = path.with_extension("json.part");
            std::fs::write(&partial, serde_json::to_vec(page)?)?;
            std::fs::rename(partial, path)
        };
        let _ = write();
    }

    fn remove(&self) {
        let _ = remove_folder(&self.0);
    }
}

fn remove_folder(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// [`crate::import::prepare_cancellable`], keeping finished OCR pages in
/// `kept`: a PDF imported with OCR resumes with the pages a cancelled or
/// stopped run left, and gives the Markdown and warning of an
/// uninterrupted run. `recognized` hears of each page as it is recognized
/// (page, page count); kept pages are not recognized again. Without OCR, or
/// for other formats, this is `prepare_cancellable`.
pub fn prepare_resumable(
    path: &Path,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
    kept: &KeptPages,
    recognized: &dyn Fn(u32, u32),
) -> Result<Imported, ImportError> {
    let Some(installed) = installed.filter(|_| !skip_ocr) else {
        return crate::import::prepare_cancellable(path, installed, skip_ocr, cancel);
    };
    check_cancel(cancel)?;
    let source = std::path::absolute(path).map_err(ImportError::Read)?;
    let mut file = std::fs::File::open(&source).map_err(ImportError::Read)?;
    let mut header = [0; 1024];
    let n = std::io::Read::read(&mut file, &mut header).map_err(ImportError::Io)?;
    if !header[..n].windows(5).any(|bytes| bytes == b"%PDF-") {
        return crate::import::prepare_cancellable(path, Some(installed), skip_ocr, cancel);
    }
    use pdf_inspector::vision::{OcrPdfOptions, process_pdf_with_ocr_mem};
    let bytes = read_pdf(&source)?;
    // The native pass reads the page count and refuses damaged or locked
    // files before any model loads, as the desktop's does.
    let page_count = process_pdf_with_ocr_mem(&bytes, OcrPdfOptions::new())
        .map_err(pdf_error)?
        .page_count;
    check_cancel(cancel)?;
    let folder = kept.folder(&digest(&bytes), &installed.config);
    let mut permit = None;
    let pages = recognize_pages(page_count, &folder, cancel, |page| {
        if permit.is_none() {
            permit = Some(
                crate::model_work::Permit::acquire_for("text recognition")
                    .map_err(|e| ImportError::OcrFailed(e.into()))?,
            );
            crate::ocr::load_onnx_runtime(&installed.onnx)
                .map_err(|e| ImportError::OcrFailed(e.into()))?;
        }
        let result = process_pdf_with_ocr_mem(&bytes, installed.options().page_numbers([page]))
            .map_err(|error| ImportError::OcrFailed(OcrFailure::Engine(error.to_string())))?;
        let review = result.pages.iter().any(|fused| {
            fused.provenance.hosted_recommended || !fused.provenance.warnings.is_empty()
        });
        recognized(page, page_count);
        Ok(Page {
            page,
            markdown: result.markdown,
            review,
        })
    })?;
    drop(permit);
    let (markdown, review) = assemble(&pages);
    let warning = pdf_warning(&markdown, review, page_count, true)?;
    folder.remove();
    Ok(Imported {
        ocr_configuration: Some(installed.config.clone()),
        source,
        markdown,
        warning,
        is_pdf: true,
        is_docx: false,
    })
}

/// Pages 1 to `page_count`: the ones `folder` kept, and the rest from
/// `recognize`, each kept as it finishes. A cancel stops before the next
/// page; the finished ones stay kept.
fn recognize_pages(
    page_count: u32,
    folder: &Folder,
    cancel: &AtomicBool,
    mut recognize: impl FnMut(u32) -> Result<Page, ImportError>,
) -> Result<Vec<Page>, ImportError> {
    let mut pages = Vec::with_capacity(page_count as usize);
    for page in 1..=page_count {
        if let Some(kept) = folder.load(page) {
            pages.push(kept);
            continue;
        }
        check_cancel(cancel)?;
        let recognized = recognize(page)?;
        folder.store(&recognized);
        pages.push(recognized);
    }
    check_cancel(cancel)?;
    Ok(pages)
}

/// The document's Markdown from its pages, as pdf-inspector joins a
/// document's pages (a blank line between non-empty pages, a final
/// newline), and the pages to review.
fn assemble(pages: &[Page]) -> (String, Vec<u32>) {
    let mut markdown = pages
        .iter()
        .map(|page| page.markdown.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if !markdown.is_empty() {
        markdown.push('\n');
    }
    let review = pages
        .iter()
        .filter(|page| page.review)
        .map(|page| page.page)
        .collect();
    (markdown, review)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn page(page: u32) -> Page {
        Page {
            page,
            markdown: format!("Page {page} text.\n"),
            review: page == 2,
        }
    }

    fn config() -> mdoc_store::settings::OcrConfig {
        mdoc_store::settings::OcrConfig::default()
    }

    /// Cancelled after two of five pages, the run keeps them; the resumed
    /// run recognizes pages 3 to 5 only and gives an uninterrupted run's
    /// pages.
    #[test]
    fn a_resumed_run_recognizes_only_the_pages_left() {
        let dir = tempfile::tempdir().unwrap();
        let kept = KeptPages::new(dir.path().into());
        let folder = kept.folder("abc", &config());
        let cancel = AtomicBool::new(false);
        let mut first = Vec::new();
        let result = recognize_pages(5, &folder, &cancel, |n| {
            first.push(n);
            if n == 2 {
                cancel.store(true, Ordering::Relaxed);
            }
            Ok(page(n))
        });
        assert!(matches!(result, Err(ImportError::Cancelled)));
        assert_eq!(first, [1, 2]);
        assert_eq!(folder.load(2), Some(page(2)));
        assert_eq!(folder.load(3), None);

        cancel.store(false, Ordering::Relaxed);
        let mut second = Vec::new();
        let resumed = recognize_pages(5, &folder, &cancel, |n| {
            second.push(n);
            Ok(page(n))
        })
        .unwrap();
        assert_eq!(second, [3, 4, 5]);
        let uninterrupted: Vec<_> = (1..=5).map(page).collect();
        assert_eq!(resumed, uninterrupted);
        assert_eq!(
            assemble(&resumed),
            (
                "Page 1 text.\n\nPage 2 text.\n\nPage 3 text.\n\nPage 4 text.\n\nPage 5 text.\n"
                    .into(),
                vec![2]
            )
        );
    }

    /// A page that failed mid-write, or reads back as another page, is
    /// recognized again.
    #[test]
    fn a_damaged_page_is_recognized_again() {
        let dir = tempfile::tempdir().unwrap();
        let folder = KeptPages::new(dir.path().into()).folder("abc", &config());
        folder.store(&page(1));
        folder.store(&page(2));
        std::fs::write(folder.path(1), b"{\"page\":1,\"mark").unwrap();
        std::fs::copy(folder.path(2), folder.path(3)).unwrap();
        let mut ran = Vec::new();
        recognize_pages(3, &folder, &AtomicBool::new(false), |n| {
            ran.push(n);
            Ok(page(n))
        })
        .unwrap();
        assert_eq!(ran, [1, 3]);
    }

    /// Pages are kept per file and per OCR configuration; discarding a file
    /// removes its pages under every configuration and leaves other files'.
    #[test]
    fn pages_are_kept_per_file_and_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let kept = KeptPages::new(dir.path().join("imports"));
        let a = dir.path().join("a.pdf");
        let b = dir.path().join("b.pdf");
        std::fs::write(&a, b"%PDF-a").unwrap();
        std::fs::write(&b, b"%PDF-b").unwrap();
        let cyrillic = mdoc_store::settings::OcrConfig {
            model: mdoc_store::settings::OcrModel::Cyrillic,
            ..config()
        };
        let finer = mdoc_store::settings::OcrConfig {
            dpi: 300,
            ..config()
        };
        let a_small = kept.folder(&digest(b"%PDF-a"), &config());
        let a_cyrillic = kept.folder(&digest(b"%PDF-a"), &cyrillic);
        let a_finer = kept.folder(&digest(b"%PDF-a"), &finer);
        let b_small = kept.folder(&digest(b"%PDF-b"), &config());
        for folder in [&a_small, &a_cyrillic, &a_finer, &b_small] {
            folder.store(&page(1));
        }
        assert_ne!(a_small.0, a_cyrillic.0);
        assert_ne!(a_small.0, a_finer.0);
        assert_ne!(a_small.0, b_small.0);
        kept.discard(&a).unwrap();
        for folder in [&a_small, &a_cyrillic, &a_finer] {
            assert!(!folder.0.exists());
        }
        assert_eq!(b_small.load(1), Some(page(1)));
        // Nothing kept, or no file: nothing to do.
        kept.discard(&a).unwrap();
        kept.discard(&dir.path().join("gone.pdf")).unwrap();
        KeptPages::new(dir.path().join("none")).discard(&b).unwrap();
    }

    /// Empty pages leave no gap, and an empty document stays empty, as
    /// pdf-inspector joins a document.
    #[test]
    fn empty_pages_leave_no_gap() {
        let blank = |n| Page {
            page: n,
            markdown: "\n".into(),
            review: false,
        };
        assert_eq!(
            assemble(&[page(1), blank(2), page(3)]).0,
            "Page 1 text.\n\nPage 3 text.\n"
        );
        assert_eq!(assemble(&[blank(1)]).0, "");
    }

    /// Without OCR this is the desktop's import: a partly scanned PDF still
    /// asks for OCR, and keeps nothing.
    #[test]
    fn without_ocr_it_is_the_desktop_import() {
        let dir = tempfile::tempdir().unwrap();
        let kept = KeptPages::new(dir.path().join("imports"));
        let pdf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/import/handmade-partly-scanned.pdf");
        let cancel = AtomicBool::new(false);
        let result = prepare_resumable(&pdf, None, false, &cancel, &kept, &|_, _| {});
        assert!(matches!(result, Err(ImportError::NeedsOcr(..))));
        let native = prepare_resumable(&pdf, None, true, &cancel, &kept, &|_, _| {}).unwrap();
        let desktop = crate::import::prepare_cancellable(&pdf, None, true, &cancel).unwrap();
        assert_eq!(native.markdown, desktop.markdown);
        assert_eq!(native.warning, desktop.warning);
        assert!(!dir.path().join("imports").exists());
    }

    /// On the OCR fixtures, an uninterrupted run gives the desktop's
    /// whole-document Markdown and warning, and a run cancelled after its
    /// first pages resumes with the rest only and gives the same.
    #[test]
    #[ignore = "downloads OCR runtimes/models; requires Apple Silicon macOS or Windows x64"]
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    ))]
    fn resumed_ocr_matches_an_uninterrupted_run_and_the_desktop() {
        let models = tempfile::tempdir().unwrap();
        let installed = crate::ocr::install_in(models.path()).unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        for (pdf, stop_after) in [
            (fixtures.join("ocr-qualification/english.pdf"), 1),
            (fixtures.join("ocr-qualification/russian.pdf"), 1),
            (fixtures.join("import/handmade-partly-scanned.pdf"), 2),
        ] {
            let name = pdf.display().to_string();
            let original = std::fs::read(&pdf).unwrap();
            let never = AtomicBool::new(false);
            let desktop = crate::import::prepare(&pdf, Some(&installed), false).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let kept = KeptPages::new(dir.path().join("imports"));
            let whole = prepare_resumable(&pdf, Some(&installed), false, &never, &kept, &|_, _| {})
                .unwrap();
            assert_eq!(whole.markdown, desktop.markdown, "{name}");
            assert_eq!(whole.warning, desktop.warning, "{name}");
            assert_eq!(whole.ocr_configuration, desktop.ocr_configuration, "{name}");
            assert!(!dir.path().join("imports").read_dir().unwrap().any(|_| true));

            let cancel = AtomicBool::new(false);
            let first = std::sync::Mutex::new(Vec::new());
            let stopped =
                prepare_resumable(&pdf, Some(&installed), false, &cancel, &kept, &|page, _| {
                    first.lock().unwrap().push(page);
                    if page == stop_after {
                        cancel.store(true, Ordering::Relaxed);
                    }
                });
            assert!(matches!(stopped, Err(ImportError::Cancelled)), "{name}");
            let first = first.into_inner().unwrap();
            assert_eq!(first, (1..=stop_after).collect::<Vec<_>>(), "{name}");

            let second = std::sync::Mutex::new(Vec::new());
            let resumed = prepare_resumable(
                &pdf,
                Some(&installed),
                false,
                &never,
                &kept,
                &|page, count| second.lock().unwrap().push((page, count)),
            )
            .unwrap();
            let second = second.into_inner().unwrap();
            assert!(second.iter().all(|(page, _)| *page > stop_after), "{name}");
            if let Some((_, count)) = second.first() {
                assert_eq!(second.len() as u32, count - stop_after, "{name}");
            }
            assert_eq!(resumed.markdown, desktop.markdown, "{name}");
            assert_eq!(resumed.warning, desktop.warning, "{name}");
            assert!(!dir.path().join("imports").read_dir().unwrap().any(|_| true));
            assert_eq!(std::fs::read(&pdf).unwrap(), original, "{name}");
        }
    }
}
