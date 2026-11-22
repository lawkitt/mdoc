//! OCR that keeps finished pages. An import with OCR
//! recognizes a PDF one page at a time and keeps each finished page under a
//! folder of the caller's, keyed by the file's digest and the OCR
//! configuration, so a cancelled or stopped import resumes with the pages
//! still to do. Each page goes through pdf-inspector's own page selection
//! with the desktop's OCR options, so a page reads as it does in a
//! whole-document run. Each page also keeps what pdf-inspector's repeated
//! header and footer pass needs from its OCR, and the pass runs across all
//! the pages once the last is recognized, so a running header or footer
//! that OCR reads on every page stays on the first only, as on the desktop
//! (see `docs/specs/recognition.md`, "Repeated headers and footers"). The
//! caller removes the file's kept pages once it has taken the result
//! ([`Completed`]). Desktop imports stay on
//! [`crate::import::prepare_cancellable`].
use crate::image_pdf::ImageFormat;
use crate::import::{
    ImportError, Imported, LeftOut, OcrFailure, Password, Source, build_images_pdf, check_cancel,
    image_warning, import_source, is_pdf, pdf_bytes, pdf_error, pdf_warning,
    prepare_images_cancellable, repeated_lines, with_password,
};
use pdf_inspector::{MarkdownOptions, vision::OcrRepeatEvidence};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

/// Bumped when a kept page's meaning changes, so older pages are not reused.
/// 2: the pdf-inspector 1.26.1 sync (#925), whose pages leave out form
/// field names that format 1's pages carry.
/// 3: #925's phase 2 forks, whose pages keep a small edge line that a
/// one-page OCR run used to drop as page furniture, leave out text drawn
/// in its background's colour and comment annotations' text (OCR reads the
/// page without them), and count repeated header and footer rows.
/// 4: #925's phase 2 follow-up, whose pages keep their OCR's repeat
/// evidence, so a running header or footer OCR reads on every page is
/// found across pages; a page kept without it can't take part.
/// 5: #925's M18, whose native text keeps a sparse page's sentence right
/// under the header: a page kept before it carries native text without that
/// sentence, which the pass across pages can't bring back.
const FORMAT: u32 = 5;

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
    /// Running header and footer rows the page's own run left out as
    /// repeats of an earlier page's: native text only, since a one-page OCR
    /// run finds no repeats. The pass across pages replaces it on the pages
    /// with repeat evidence.
    repeated_lines: u32,
    /// What pdf-inspector's repeated header and footer pass needs from the
    /// page's OCR (one entry, or none for a page whose content isn't
    /// full-page OCR), kept as the run gave it. The page's `markdown` and
    /// `repeated_lines` stay its own run's; the pass across pages gives the
    /// final ones when the document is put together.
    repeat_evidence: Vec<OcrRepeatEvidence>,
}

impl KeptPages {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Remove every page kept for the file at `source`, under any OCR
    /// configuration. Call it while the file still exists: pages are found
    /// by its digest.
    pub fn discard(&self, source: &Path) -> std::io::Result<()> {
        self.discard_images(&[source.to_path_buf()])
    }

    /// Remove every page kept for the images at `pages`, imported together
    /// as one document ([`prepare_resumable_images`]). Call it while the
    /// files still exist: pages are found by their digests.
    pub fn discard_images(&self, pages: &[PathBuf]) -> std::io::Result<()> {
        let mut digests = Vec::with_capacity(pages.len());
        for page in pages {
            match std::fs::File::open(page) {
                Ok(mut file) => {
                    let mut hash = Sha256::new();
                    let mut buffer = zeroize::Zeroizing::new([0; 8192]);
                    loop {
                        let read = file.read(&mut *buffer)?;
                        if read == 0 {
                            break;
                        }
                        hash.update(&buffer[..read]);
                    }
                    digests.push(format_digest(&hash.finalize()));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        self.remove_file_pages(&images_digest(digests))
    }

    /// Remove the pages kept for the file with `digest`, under every OCR
    /// configuration.
    fn remove_file_pages(&self, digest: &str) -> std::io::Result<()> {
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
        self.folder_in_format(FORMAT, digest, config)
    }

    fn folder_in_format(
        &self,
        format: u32,
        digest: &str,
        config: &mdoc_store::settings::OcrConfig,
    ) -> Folder {
        let manifest = crate::ocr::manifest::for_model(config.model);
        let key = format!(
            "{format}\n{}\n{}\n{}",
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

/// A completed import's kept pages: remove them once the result is taken,
/// so an import rejected after its pages finished (cancelled meanwhile, or
/// superseded) resumes with nothing left to recognize.
#[derive(Debug)]
pub struct Completed {
    kept: KeptPages,
    digest: String,
}

impl Completed {
    /// Remove the file's kept pages, under every OCR configuration: pages an
    /// earlier run with another model left are abandoned too.
    pub fn remove(self) {
        let _ = self.kept.remove_file_pages(&self.digest);
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

    /// Start keeping pages: the folder is made once, when a run starts, so
    /// a page finishing after `KeptPages::discard` is not kept.
    fn open(&self) {
        let _ = std::fs::create_dir_all(&self.0);
    }

    /// Keep `page`, whole or not at all, through a temporary file of this
    /// writer's own, so writers in two processes never share one. Keeping
    /// is best effort: a page that could not be written is recognized again
    /// on resume.
    fn store(&self, page: &Page) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = self.path(page.page);
        let partial = path.with_extension(format!(
            "{}-{}.part",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let write = || -> std::io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&partial)?;
            std::io::Write::write_all(&mut file, &serde_json::to_vec(page)?)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&partial, &path)
        };
        if write().is_err() {
            let _ = std::fs::remove_file(&partial);
        }
    }
}

fn remove_folder(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn digest(bytes: &[u8]) -> String {
    format_digest(&Sha256::digest(bytes))
}

fn format_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The key of images imported together, from each one's digest in order:
/// one image's own digest, so a single photo's pages are found by its file.
fn images_digest(digests: Vec<String>) -> String {
    match <[String; 1]>::try_from(digests) {
        Ok([one]) => one,
        Err(several) => digest(several.concat().as_bytes()),
    }
}

/// [`crate::import::prepare_cancellable`], keeping finished OCR pages in
/// `kept`: a PDF imported with OCR resumes with the pages a cancelled or
/// stopped run left, and gives the Markdown and warning of an
/// uninterrupted run. `recognized` hears of each page as it is recognized
/// (page, page count); kept pages are not recognized again. The pages stay
/// kept until the caller removes them through the [`Completed`] returned
/// with the result. Without OCR, or for other formats, this is
/// `prepare_cancellable`, and nothing is kept.
///
/// A PDF opened with `password` is locked: every pass takes the password,
/// and its recognized pages are never kept (#1024 decision 7, until kept
/// pages can be sealed with the case key). It is recognized page by page
/// all the same, `recognized` hearing of each, and a cancelled or stopped
/// run starts over; no [`Completed`] comes with it.
pub fn prepare_resumable(
    path: &Path,
    password: Option<&Password>,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
    kept: &KeptPages,
    recognized: &dyn Fn(u32, u32),
) -> Result<(Imported, Option<Completed>), ImportError> {
    check_cancel(cancel)?;
    let source = Source::read(path)?.unlock(password)?;
    check_cancel(cancel)?;
    let image = ImageFormat::from_bytes(source.bytes()).is_some();
    let Some(installed) = installed.filter(|_| !skip_ocr && (image || is_pdf(source.bytes())))
    else {
        return import_source(source, password, installed, skip_ocr, cancel)
            .map(|imported| (imported, None));
    };
    // Pages are kept under the file's digest: an image's, not the PDF built
    // from it, so `KeptPages::discard` finds them by the file.
    let digest = digest(source.bytes());
    let (source, frames) = match source.image_pdf(installed.config.dpi, cancel) {
        Some(built) => built?,
        None => (source, 0),
    };
    let document = Recognizable {
        source,
        frames,
        image,
        digest,
    };
    recognize_resumable(document, password, installed, cancel, kept, recognized)
}

/// [`crate::import::prepare_images_cancellable`], keeping finished OCR
/// pages in `kept` as [`prepare_resumable`] does: several photos or camera
/// pages imported as one document resume with the pages left. Their pages
/// are kept under the images' digests together, which
/// [`KeptPages::discard_images`] finds.
pub fn prepare_resumable_images(
    paths: &[PathBuf],
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
    kept: &KeptPages,
    recognized: &dyn Fn(u32, u32),
) -> Result<(Imported, Option<Completed>), ImportError> {
    let Some(installed) = installed.filter(|_| !skip_ocr) else {
        return prepare_images_cancellable(paths, installed, skip_ocr, cancel)
            .map(|imported| (imported, None));
    };
    let mut digests = Vec::new();
    let (source, frames) = build_images_pdf(paths, installed.config.dpi, cancel, |bytes| {
        digests.push(digest(bytes));
    })?;
    let digest = images_digest(digests);
    let document = Recognizable {
        source,
        frames,
        image: true,
        digest,
    };
    // The built PDF is never locked.
    recognize_resumable(document, None, installed, cancel, kept, recognized)
}

/// A PDF to recognize page by page, and the key its pages are kept under.
struct Recognizable {
    source: Source,
    /// The frames of animated images the PDF leaves out.
    frames: u64,
    /// The PDF was built from images.
    image: bool,
    digest: String,
}

/// OCR of `document` page by page, keeping each finished page in `kept`
/// unless `password` opens it.
fn recognize_resumable(
    Recognizable {
        source,
        frames,
        image,
        digest,
    }: Recognizable,
    password: Option<&Password>,
    installed: &crate::ocr::Installed,
    cancel: &AtomicBool,
    kept: &KeptPages,
    recognized: &dyn Fn(u32, u32),
) -> Result<(Imported, Option<Completed>), ImportError> {
    use pdf_inspector::vision::{OcrPdfOptions, process_pdf_with_ocr_mem};
    let bytes = pdf_bytes(&source)?;
    // The native pass reads the page count and refuses damaged or locked
    // files before any model loads, as the desktop's does.
    let mut native = process_pdf_with_ocr_mem(bytes, with_password(OcrPdfOptions::new(), password))
        .map_err(|error| pdf_error(error, bytes, password))?;
    let (page_count, mut left_out) = (native.page_count, LeftOut::of_pdf(&native, &[]));
    let annotations = std::mem::take(&mut native.annotations);
    drop(native);
    check_cancel(cancel)?;
    // A locked source's pages are never kept, nor read back.
    let folder = password
        .is_none()
        .then(|| kept.folder(&digest, &installed.config));
    if let Some(folder) = &folder {
        folder.open();
    }
    let markdown_options = installed.options().markdown;
    let mut permit = None;
    let reusable = |kept: &Page| reusable(kept, page_count, &markdown_options);
    let mut pages = recognize_pages(page_count, folder.as_ref(), cancel, reusable, |page| {
        if permit.is_none() {
            permit = Some(
                crate::model_work::Permit::acquire_for("text recognition")
                    .map_err(|e| ImportError::OcrFailed(e.into()))?,
            );
            crate::ocr::load_onnx_runtime(&installed.onnx)
                .map_err(|e| ImportError::OcrFailed(e.into()))?;
        }
        let options = with_password(
            installed
                .options()
                .retain_repeat_evidence(true)
                .page_numbers([page]),
            password,
        );
        let result = process_pdf_with_ocr_mem(bytes, options)
            .map_err(|error| ImportError::OcrFailed(OcrFailure::Engine(error.to_string())))?;
        let review = result.pages.iter().any(|fused| {
            fused.provenance.hosted_recommended || !fused.provenance.warnings.is_empty()
        });
        recognized(page, page_count);
        Ok(Page {
            page,
            repeated_lines: u32::try_from(repeated_lines(&result)).unwrap_or(u32::MAX),
            markdown: result.markdown,
            review,
            repeat_evidence: result.repeat_evidence,
        })
    })?;
    drop(permit);
    strip_repeats(&mut pages, &markdown_options)
        .map_err(|error| ImportError::OcrFailed(OcrFailure::Engine(error.to_string())))?;
    let (markdown, review) = assemble(&pages);
    let mut warning = pdf_warning(&markdown, review, page_count, true)?;
    if image {
        warning = image_warning(warning);
    }
    if image {
        left_out = left_out.of_images(frames);
    } else {
        left_out.animation_frames = frames;
    }
    left_out.repeated_lines = pages
        .iter()
        .map(|page| u64::from(page.repeated_lines))
        .sum();
    Ok((
        Imported {
            ocr_configuration: Some(installed.config.clone()),
            source,
            markdown,
            warning,
            left_out,
            comments: Vec::new(),
            annotations,
            is_pdf: true,
            is_docx: false,
            is_image: image,
        },
        folder.map(|_| Completed {
            kept: kept.clone(),
            digest,
        }),
    ))
}

/// Pages 1 to `page_count`: the ones `folder` kept that are `reusable`, and
/// the rest from `recognize`, each kept as it finishes. A cancel stops
/// before the next page; the finished ones stay kept. Without a folder
/// nothing is read or kept.
fn recognize_pages(
    page_count: u32,
    folder: Option<&Folder>,
    cancel: &AtomicBool,
    reusable: impl Fn(&Page) -> bool,
    mut recognize: impl FnMut(u32) -> Result<Page, ImportError>,
) -> Result<Vec<Page>, ImportError> {
    let mut pages = Vec::with_capacity(page_count as usize);
    for page in 1..=page_count {
        if let Some(kept) = folder
            .and_then(|folder| folder.load(page))
            .filter(&reusable)
        {
            pages.push(kept);
            continue;
        }
        check_cancel(cancel)?;
        let recognized = recognize(page)?;
        if let Some(folder) = folder {
            folder.store(&recognized);
        }
        pages.push(recognized);
    }
    check_cancel(cancel)?;
    Ok(pages)
}

/// Whether a kept page's repeat evidence can take part in the pass across
/// pages: it is the page's own, from a document of `page_count` pages, and
/// pdf-inspector accepts it (its format and geometry). A page that fails is
/// recognized again rather than failing the import at the end.
fn reusable(kept: &Page, page_count: u32, markdown: &MarkdownOptions) -> bool {
    kept.repeat_evidence.iter().all(|evidence| {
        evidence.page_number() == kept.page && evidence.document_page_count() == page_count
    }) && (kept.repeat_evidence.is_empty()
        || pdf_inspector::vision::strip_repeats_across_pages(&kept.repeat_evidence, markdown)
            .is_ok())
}

/// Runs pdf-inspector's repeated header and footer pass across the pages'
/// repeat evidence, as a whole-document run does across the pages it
/// recognizes: each page that took part gets the Markdown and repeated-line
/// count a whole-document run gives it, and is flagged for review if that
/// leaves it empty. `markdown` is the options the page runs used. Pages
/// without evidence keep their own run's, as they would in a
/// whole-document run.
fn strip_repeats(
    pages: &mut [Page],
    markdown: &MarkdownOptions,
) -> Result<(), pdf_inspector::vision::OcrFusionError> {
    let evidence: Vec<OcrRepeatEvidence> = pages
        .iter_mut()
        .flat_map(|page| std::mem::take(&mut page.repeat_evidence))
        .collect();
    if evidence.is_empty() {
        return Ok(());
    }
    let across = pdf_inspector::vision::strip_repeats_across_pages(&evidence, markdown)?;
    for stripped in across.pages {
        if let Some(page) = pages
            .iter_mut()
            .find(|page| page.page == stripped.page_number)
        {
            // A page left with nothing but its repeats is flagged for
            // review, as a whole-document run warns that OCR produced no
            // usable text: a page with evidence shows OCR, or native text
            // that never empties, so an empty page here is an empty OCR
            // page.
            page.review |= stripped.markdown.trim().is_empty();
            page.markdown = stripped.markdown;
            page.repeated_lines = stripped.repeated_lines;
        }
    }
    Ok(())
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
            repeated_lines: 0,
            repeat_evidence: Vec::new(),
        }
    }

    fn config() -> mdoc_store::settings::OcrConfig {
        mdoc_store::settings::OcrConfig::default()
    }

    /// An image's pages are kept under its file's digest, not the digest of
    /// the PDF built from it: a later run reuses them, and discarding them
    /// by the file finds them.
    #[test]
    fn an_image_keeps_its_pages_under_the_file_s_digest() {
        let dir = tempfile::tempdir().unwrap();
        let photo = crate::image_pdf::tests::write_photo(dir.path(), "scan.jpg");
        let kept = KeptPages::new(dir.path().join("kept"));
        let missing = dir.path().join("not installed");
        let installed = crate::ocr::Installed {
            config: config(),
            models: missing.clone(),
            pdfium: missing.clone(),
            onnx: missing,
        };
        // No OCR runtime here: the run stops at loading it, after keeping
        // its folder ready.
        let result = prepare_resumable(
            &photo,
            None,
            Some(&installed),
            false,
            &AtomicBool::new(false),
            &kept,
            &|_, _| {},
        );
        assert!(
            matches!(result, Err(ImportError::OcrFailed(_))),
            "{result:?}"
        );
        let file_digest = digest(&std::fs::read(&photo).unwrap());
        let folder = kept.folder(&file_digest, &installed.config);
        assert!(folder.0.is_dir());
        // A page kept under the file's digest is reused: the run needs no
        // OCR runtime and recognizes nothing.
        folder.store(&Page {
            page: 1,
            markdown: "Kept text of the scanned page.\n".into(),
            review: false,
            repeated_lines: 0,
            repeat_evidence: Vec::new(),
        });
        let recognized = std::cell::Cell::new(0);
        let (imported, completed) = prepare_resumable(
            &photo,
            None,
            Some(&installed),
            false,
            &AtomicBool::new(false),
            &kept,
            &|_, _| recognized.set(recognized.get() + 1),
        )
        .unwrap();
        assert_eq!(recognized.get(), 0);
        assert!(imported.is_image && imported.is_pdf);
        assert_eq!(imported.markdown.trim(), "Kept text of the scanned page.");
        // The photo is the page OCR read, not a picture left out (#1070).
        assert_eq!(imported.left_out, LeftOut::default());
        assert_eq!(imported.note(), None);
        assert!(completed.is_some());
        kept.discard(&photo).unwrap();
        assert!(!folder.0.exists());
    }

    /// An animated GIF's recognized import counts the frames left out and
    /// never its first frame, the page (#1070).
    #[test]
    fn an_animated_image_counts_its_frames_not_its_page() {
        let dir = tempfile::tempdir().unwrap();
        let animated = dir.path().join("animated.gif");
        std::fs::write(
            &animated,
            crate::image_pdf::tests::gif(&[[200, 0, 0, 255], [0, 200, 0, 255], [0, 0, 200, 255]]),
        )
        .unwrap();
        let kept = KeptPages::new(dir.path().join("kept"));
        let missing = dir.path().join("not installed");
        let installed = crate::ocr::Installed {
            config: config(),
            models: missing.clone(),
            pdfium: missing.clone(),
            onnx: missing,
        };
        let never = AtomicBool::new(false);
        // No OCR runtime: the first run stops at loading it; a kept page
        // then stands in for what it would have read.
        let first = prepare_resumable(
            &animated,
            None,
            Some(&installed),
            false,
            &never,
            &kept,
            &|_, _| {},
        );
        assert!(matches!(first, Err(ImportError::OcrFailed(_))), "{first:?}");
        kept.folder(
            &digest(&std::fs::read(&animated).unwrap()),
            &installed.config,
        )
        .store(&Page {
            page: 1,
            markdown: "Kept text of the first frame.\n".into(),
            review: false,
            repeated_lines: 0,
            repeat_evidence: Vec::new(),
        });
        let (imported, _) = prepare_resumable(
            &animated,
            None,
            Some(&installed),
            false,
            &never,
            &kept,
            &|_, _| {},
        )
        .unwrap();
        assert_eq!(
            imported.left_out,
            LeftOut {
                animation_frames: 2,
                ..LeftOut::default()
            }
        );
        assert_eq!(
            imported.note().as_deref(),
            Some("2 animation frames not imported.")
        );
    }

    /// Photos imported together keep their pages under their digests
    /// together, in order: a different order, or one photo alone, is
    /// another document. Discarding the same photos finds them.
    #[test]
    fn photos_imported_together_keep_their_pages_together() {
        let dir = tempfile::tempdir().unwrap();
        let first = crate::image_pdf::tests::write_photo(dir.path(), "page-1.jpg");
        let second = dir.path().join("page-2.png");
        std::fs::write(&second, crate::image_pdf::tests::small_png()).unwrap();
        let kept = KeptPages::new(dir.path().join("kept"));
        let missing = dir.path().join("not installed");
        let installed = crate::ocr::Installed {
            config: config(),
            models: missing.clone(),
            pdfium: missing.clone(),
            onnx: missing,
        };
        let pages = [first.clone(), second.clone()];
        let result = prepare_resumable_images(
            &pages,
            Some(&installed),
            false,
            &AtomicBool::new(false),
            &kept,
            &|_, _| {},
        );
        assert!(
            matches!(result, Err(ImportError::OcrFailed(_))),
            "{result:?}"
        );
        let file_digest = |path: &Path| digest(&std::fs::read(path).unwrap());
        let together = images_digest(vec![file_digest(&first), file_digest(&second)]);
        let folder = kept.folder(&together, &installed.config);
        assert!(folder.0.is_dir());
        assert_ne!(
            together,
            images_digest(vec![file_digest(&second), file_digest(&first)])
        );
        assert_eq!(
            images_digest(vec![file_digest(&first)]),
            file_digest(&first)
        );
        // Another order, or one of them, finds nothing to discard.
        kept.discard_images(&[second.clone(), first.clone()])
            .unwrap();
        kept.discard(&first).unwrap();
        assert!(folder.0.is_dir());
        kept.discard_images(&pages).unwrap();
        assert!(!folder.0.exists());
        // A photo gone since: nothing to find, and no error.
        std::fs::remove_file(&second).unwrap();
        kept.discard_images(&pages).unwrap();
    }

    /// Pages kept before a format change (form field names in their text,
    /// before #925's phase 1; comment annotations' text read from the scan,
    /// before its phase 2; no repeat evidence, before its follow-up; native
    /// text without a sparse page's sentence under the header, before M18)
    /// are recognized again, not reused.
    #[test]
    fn pages_kept_in_an_older_format_are_not_reused() {
        let dir = tempfile::tempdir().unwrap();
        let kept = KeptPages::new(dir.path().into());
        assert_eq!(FORMAT, 5);
        for format in 1..FORMAT {
            let older = kept.folder_in_format(format, "abc", &config());
            older.open();
            older.store(&page(1));
            assert_eq!(older.load(1), Some(page(1)));
        }
        let current = kept.folder("abc", &config());
        current.open();
        assert_eq!(current.load(1), None);
        // A format 3 page, without repeat evidence, doesn't read as a
        // current one even where a current page would be.
        std::fs::write(
            current.path(1),
            br#"{"page":1,"markdown":"Page 1 text.\n","review":false,"repeated_lines":0}"#,
        )
        .unwrap();
        assert_eq!(current.load(1), None);
    }

    /// Repeat evidence for page `page` of a `pages`-page scan, as a one-page
    /// OCR run keeps it: `header`, two lines of text and a footer, read off
    /// a 150 dpi US Letter page. Built from JSON, since pdf-inspector builds
    /// it only from a run. If this stops parsing or is refused after a fork
    /// bump, the evidence changed format, and `FORMAT` must change too.
    fn evidence(page: u32, pages: u32, header: &str) -> OcrRepeatEvidence {
        let span = |text: String, top: f32| {
            serde_json::json!({
                "text": text,
                "polygon": [[150.0, top], [900.0, top], [900.0, top + 30.0], [150.0, top + 30.0]],
                "orientation_degrees": null,
            })
        };
        serde_json::from_value(serde_json::json!({
            "format": 1,
            "page_number": page,
            "document_page_count": pages,
            "page_width": 612.0,
            "page_height": 792.0,
            "pixel_width": 1275,
            "pixel_height": 1650,
            "transform": ["0.48", "0.0", "0.0", "-0.48", "0.0", "791.9999389648438"],
            "spans": [
                span(header.to_owned(), 64.0),
                span(format!("Item {page}.1. The Board reviewed the audited accounts."), 167.0),
                span(format!("Item {page}.2. Revenue increased by eleven percent."), 221.0),
                span("Prepared for the shareholders of Acme Holdings Ltd.".into(), 1544.0),
            ],
            "native": null,
        }))
        .unwrap()
    }

    /// A page as a one-page OCR run keeps it, with its evidence.
    fn scanned(page: u32, pages: u32, header: &str) -> Page {
        Page {
            page,
            markdown: format!("{header}\n\nItem {page}.1.\n"),
            review: false,
            repeated_lines: 0,
            repeat_evidence: vec![evidence(page, pages, header)],
        }
    }

    /// The pass across pages leaves a running header and footer on the
    /// first page only and counts the rest, as a whole-document run does; a
    /// page without evidence (native text) keeps its own run's Markdown and
    /// count.
    #[test]
    fn the_pass_across_pages_drops_a_scans_running_header_and_footer() {
        let header = "Annual Report of the Board 2026 Confidential";
        let native = Page {
            repeated_lines: 1,
            ..page(5)
        };
        let mut pages: Vec<Page> = (1..=4).map(|n| scanned(n, 5, header)).collect();
        pages.push(native.clone());
        strip_repeats(&mut pages, &MarkdownOptions::default()).unwrap();
        let counts: Vec<u32> = pages.iter().map(|page| page.repeated_lines).collect();
        assert_eq!(counts, [0, 2, 2, 2, 1]);
        for page in &pages[..4] {
            let first = page.page == 1;
            assert_eq!(page.markdown.contains(header), first, "{}", page.markdown);
            assert_eq!(page.markdown.contains("Acme Holdings"), first);
            assert!(page.markdown.contains(&format!("Item {}.2.", page.page)));
            assert!(page.repeat_evidence.is_empty());
        }
        assert_eq!(pages[4], native);
        let (markdown, _) = assemble(&pages);
        assert_eq!(markdown.matches(header).count(), 1, "{markdown}");
    }

    /// A scanned page holding only the running header and footer is empty
    /// once they go, and is flagged for review as a whole-document run
    /// flags it, whether its pages were kept by an earlier run or not.
    #[test]
    fn a_page_left_empty_by_the_pass_is_flagged_for_review() {
        let header = "Annual Report of the Board 2026 Confidential";
        let only_repeats = |n: u32| {
            let mut page = scanned(n, 4, header);
            if n == 4 {
                let mut json = serde_json::to_value(&page.repeat_evidence[0]).unwrap();
                let spans = json["spans"].as_array_mut().unwrap();
                spans.retain(|span| !span["text"].as_str().unwrap().starts_with("Item"));
                page.repeat_evidence = vec![serde_json::from_value(json).unwrap()];
            }
            page
        };
        let dir = tempfile::tempdir().unwrap();
        let folder = KeptPages::new(dir.path().into()).folder("abc", &config());
        folder.open();
        let never = AtomicBool::new(false);
        let uninterrupted =
            recognize_pages(4, Some(&folder), &never, |_| true, |n| Ok(only_repeats(n))).unwrap();
        std::fs::remove_file(folder.path(3)).unwrap();
        std::fs::remove_file(folder.path(4)).unwrap();
        let mut ran = Vec::new();
        let resumed = recognize_pages(
            4,
            Some(&folder),
            &never,
            |_| true,
            |n| {
                ran.push(n);
                Ok(only_repeats(n))
            },
        )
        .unwrap();
        assert_eq!(ran, [3, 4]);
        for mut pages in [uninterrupted, resumed] {
            strip_repeats(&mut pages, &MarkdownOptions::default()).unwrap();
            let counts: Vec<u32> = pages.iter().map(|page| page.repeated_lines).collect();
            assert_eq!(counts, [0, 2, 2, 2]);
            assert_eq!(pages[3].markdown.trim(), "");
            assert_eq!(assemble(&pages).1, [4]);
        }
    }

    /// A header whose case number changes from page to page is no repeat,
    /// and two pages are too few: nothing is left out.
    #[test]
    fn the_pass_across_pages_keeps_changing_and_rare_headers() {
        let mut changing: Vec<Page> = (1..=4)
            .map(|n| scanned(n, 4, &format!("Case No. 2-12{n}/2026 Annual Report")))
            .collect();
        strip_repeats(&mut changing, &MarkdownOptions::default()).unwrap();
        for page in &changing {
            assert!(
                page.markdown
                    .contains(&format!("Case No. 2-12{}/2026", page.page))
            );
        }
        // The footer is still an exact repeat.
        let counts: Vec<u32> = changing.iter().map(|page| page.repeated_lines).collect();
        assert_eq!(counts, [0, 1, 1, 1]);

        let header = "Annual Report of the Board 2026 Confidential";
        let mut two: Vec<Page> = (1..=2).map(|n| scanned(n, 2, header)).collect();
        strip_repeats(&mut two, &MarkdownOptions::default()).unwrap();
        assert!(two.iter().all(|page| page.markdown.contains(header)));
        assert!(two.iter().all(|page| page.repeated_lines == 0));
    }

    /// A kept page is reused only with evidence the pass can take: its own
    /// page's, from a document of this length, in a format pdf-inspector
    /// reads. Anything else is recognized again rather than failing the
    /// import when the document is put together.
    #[test]
    fn a_page_with_unusable_evidence_is_recognized_again() {
        let options = MarkdownOptions::default();
        let header = "Annual Report of the Board 2026 Confidential";
        assert!(reusable(&page(1), 4, &options));
        assert!(reusable(&scanned(2, 4, header), 4, &options));
        assert!(!reusable(&scanned(2, 5, header), 4, &options));
        let moved = Page {
            page: 3,
            ..scanned(2, 4, header)
        };
        assert!(!reusable(&moved, 4, &options));
        let newer = |pages| {
            let mut json = serde_json::to_value(evidence(2, pages, header)).unwrap();
            json["format"] = 99.into();
            Page {
                repeat_evidence: vec![serde_json::from_value(json).unwrap()],
                ..scanned(2, pages, header)
            }
        };
        assert!(!reusable(&newer(4), 4, &options));

        let dir = tempfile::tempdir().unwrap();
        let folder = KeptPages::new(dir.path().into()).folder("abc", &config());
        folder.open();
        folder.store(&scanned(1, 2, header));
        folder.store(&newer(2));
        let mut ran = Vec::new();
        recognize_pages(
            2,
            Some(&folder),
            &AtomicBool::new(false),
            |kept| reusable(kept, 2, &options),
            |n| {
                ran.push(n);
                Ok(scanned(n, 2, header))
            },
        )
        .unwrap();
        assert_eq!(ran, [2]);
    }

    /// Cancelled after two of five pages, the run keeps them; the resumed
    /// run recognizes pages 3 to 5 only and gives an uninterrupted run's
    /// pages.
    #[test]
    fn a_resumed_run_recognizes_only_the_pages_left() {
        let dir = tempfile::tempdir().unwrap();
        let kept = KeptPages::new(dir.path().into());
        let folder = kept.folder("abc", &config());
        folder.open();
        let cancel = AtomicBool::new(false);
        let mut first = Vec::new();
        let result = recognize_pages(
            5,
            Some(&folder),
            &cancel,
            |_| true,
            |n| {
                first.push(n);
                if n == 2 {
                    cancel.store(true, Ordering::Relaxed);
                }
                Ok(page(n))
            },
        );
        assert!(matches!(result, Err(ImportError::Cancelled)));
        assert_eq!(first, [1, 2]);
        assert_eq!(folder.load(2), Some(page(2)));
        assert_eq!(folder.load(3), None);

        cancel.store(false, Ordering::Relaxed);
        let mut second = Vec::new();
        let resumed = recognize_pages(
            5,
            Some(&folder),
            &cancel,
            |_| true,
            |n| {
                second.push(n);
                Ok(page(n))
            },
        )
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
        folder.open();
        folder.store(&page(1));
        folder.store(&page(2));
        std::fs::write(folder.path(1), b"{\"page\":1,\"mark").unwrap();
        std::fs::copy(folder.path(2), folder.path(3)).unwrap();
        let mut ran = Vec::new();
        recognize_pages(
            3,
            Some(&folder),
            &AtomicBool::new(false),
            |_| true,
            |n| {
                ran.push(n);
                Ok(page(n))
            },
        )
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
            folder.open();
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
        // A completed import removes its file's pages under every
        // configuration, and only once the caller takes the result.
        let b_cyrillic = kept.folder(&digest(b"%PDF-b"), &cyrillic);
        b_cyrillic.open();
        b_cyrillic.store(&page(1));
        let completed = Completed {
            kept: kept.clone(),
            digest: digest(b"%PDF-b"),
        };
        assert_eq!(b_small.load(1), Some(page(1)));
        completed.remove();
        assert!(!b_small.0.exists() && !b_cyrillic.0.exists());
    }

    /// A page that finishes after its import was discarded (cancelled, then
    /// dropped while the page was still being recognized) is not kept.
    #[test]
    fn a_page_finishing_after_a_discard_is_not_kept() {
        let dir = tempfile::tempdir().unwrap();
        let kept = KeptPages::new(dir.path().join("imports"));
        let source = dir.path().join("a.pdf");
        std::fs::write(&source, b"%PDF-a").unwrap();
        let folder = kept.folder(&digest(b"%PDF-a"), &config());
        folder.open();
        let cancel = AtomicBool::new(false);
        let result = recognize_pages(
            3,
            Some(&folder),
            &cancel,
            |_| true,
            |n| {
                if n == 2 {
                    cancel.store(true, Ordering::Relaxed);
                    kept.discard(&source).unwrap();
                }
                Ok(page(n))
            },
        );
        assert!(matches!(result, Err(ImportError::Cancelled)));
        assert!(!folder.0.exists());
    }

    /// Writers keep a page through temporary files of their own and leave
    /// none behind.
    #[test]
    fn writers_never_share_a_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let folder = KeptPages::new(dir.path().into()).folder("abc", &config());
        folder.open();
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| folder.store(&page(1)));
            }
        });
        assert_eq!(folder.load(1), Some(page(1)));
        let names: Vec<_> = std::fs::read_dir(&folder.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["00001.json"]);
    }

    /// Empty pages leave no gap, and an empty document stays empty, as
    /// pdf-inspector joins a document.
    #[test]
    fn empty_pages_leave_no_gap() {
        let blank = |n| Page {
            page: n,
            markdown: "\n".into(),
            review: false,
            repeated_lines: 0,
            repeat_evidence: Vec::new(),
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
        let result = prepare_resumable(&pdf, None, None, false, &cancel, &kept, &|_, _| {});
        assert!(matches!(result, Err(ImportError::NeedsOcr(..))));
        let (native, completed) =
            prepare_resumable(&pdf, None, None, true, &cancel, &kept, &|_, _| {}).unwrap();
        assert!(completed.is_none());
        let desktop = crate::import::prepare_cancellable(&pdf, None, None, true, &cancel).unwrap();
        assert_eq!(native.markdown, desktop.markdown);
        assert_eq!(native.warning, desktop.warning);
        assert_eq!(native.left_out, desktop.left_out);
        assert!(!dir.path().join("imports").exists());
    }

    /// A locked PDF keeps no recognized page (#1024 decision 7): opened with
    /// its password, a run makes no folder for it and reads none back, while
    /// its open twin's run makes one before its first page. The OCR runtime
    /// here doesn't exist, so each run stops at its first page, after that
    /// choice; a wrong password stops before any.
    #[test]
    fn a_locked_pdf_keeps_no_recognized_pages() {
        let dir = tempfile::tempdir().unwrap();
        let installed = crate::ocr::Installed {
            config: config(),
            models: dir.path().join("no-models"),
            pdfium: dir.path().join("no-pdfium"),
            onnx: dir.path().join("no-onnx-runtime"),
        };
        let fixtures =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdf-source");
        let cancel = AtomicBool::new(false);
        let run = |name: &str, password: Option<&str>, kept: &KeptPages| {
            let password = password.map(|text| Password::new(text.into()));
            let path = fixtures.join(name);
            prepare_resumable(
                &path,
                password.as_ref(),
                Some(&installed),
                false,
                &cancel,
                kept,
                &|_, _| {},
            )
        };
        for name in ["comments-aes128.pdf", "comments-aes256.pdf"] {
            let root = dir.path().join(format!("locked-{name}"));
            let kept = KeptPages::new(root.clone());
            let result = run(name, Some("Petrov-2026"), &kept);
            assert!(matches!(result, Err(ImportError::OcrFailed(_))), "{name}");
            assert!(!root.exists(), "{name}: nothing kept");
            let wrong = run(name, Some("wrong"), &kept);
            assert!(matches!(wrong, Err(ImportError::WrongPassword)), "{name}");
            let none = run(name, None, &kept);
            assert!(matches!(none, Err(ImportError::PasswordRequired)), "{name}");
            assert!(!root.exists(), "{name}");
        }
        let root = dir.path().join("open");
        let kept = KeptPages::new(root.clone());
        assert!(matches!(
            run("comments.pdf", None, &kept),
            Err(ImportError::OcrFailed(_))
        ));
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            1,
            "the open twin's folder"
        );
    }

    /// What `a_locked_scan_is_recognized_with_its_password_and_keeps_nothing`
    /// runs in a process of its own, whose temporary folder is an empty one:
    /// a locked, partly scanned PDF through the desktop's import with OCR
    /// and through the resumable one, with its password and the OCR
    /// runtime the parent installed.
    #[test]
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    ))]
    #[ignore = "run by a_locked_scan_is_recognized_with_its_password_and_keeps_nothing"]
    fn recognize_a_locked_scan() {
        let var = |name: &str| PathBuf::from(std::env::var_os(name).expect(name));
        let installed = crate::ocr::install_in(&var("MDOC_OCR_MODELS")).unwrap();
        let kept = KeptPages::new(var("MDOC_KEPT_PAGES"));
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        let locked = fixtures.join("pdf-source/partly-scanned-aes256.pdf");
        let open = fixtures.join("import/handmade-partly-scanned.pdf");
        let password = Password::new("Petrov-2026".into());
        let never = AtomicBool::new(false);
        // Both OCR passes, the native one and PDFium's, take the password.
        let expected =
            crate::import::prepare_cancellable(&open, None, Some(&installed), false, &never)
                .unwrap();
        let desktop = crate::import::prepare_cancellable(
            &locked,
            Some(&password),
            Some(&installed),
            false,
            &never,
        )
        .unwrap();
        assert_eq!(desktop.markdown, expected.markdown);
        assert_eq!(desktop.warning, expected.warning);
        assert_eq!(desktop.left_out, expected.left_out);
        // Page by page, with progress, keeping nothing and handing back no
        // pages to remove.
        let heard = std::sync::Mutex::new(Vec::new());
        let (resumed, completed) = prepare_resumable(
            &locked,
            Some(&password),
            Some(&installed),
            false,
            &never,
            &kept,
            &|page, pages| heard.lock().unwrap().push((page, pages)),
        )
        .unwrap();
        assert!(completed.is_none());
        assert_eq!(resumed.markdown, expected.markdown);
        let heard = heard.into_inner().unwrap();
        let pages = heard.first().map(|&(_, pages)| pages).unwrap();
        assert_eq!(
            heard,
            (1..=pages).map(|page| (page, pages)).collect::<Vec<_>>()
        );
        assert!(!var("MDOC_KEPT_PAGES").exists(), "nothing kept");
    }

    /// A locked scan through both OCR paths (#1024 decisions 5 and 7), in a
    /// process whose temporary folder is an empty one: the same Markdown as
    /// its open copy, progress for every page, no kept page and no
    /// `Completed`, and the temporary folder still empty afterwards. The OCR
    /// runtime and models are installed beforehand, outside it.
    #[test]
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    ))]
    #[ignore = "downloads OCR runtimes/models; requires Apple Silicon macOS or Windows x64"]
    fn a_locked_scan_is_recognized_with_its_password_and_keeps_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let models = dir.path().join("models");
        crate::ocr::install_in(&models).unwrap();
        let temporary = dir.path().join("temporary");
        std::fs::create_dir(&temporary).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "resume::tests::recognize_a_locked_scan",
                "--exact",
                "--ignored",
                "--test-threads=1",
            ])
            .env("MDOC_OCR_MODELS", &models)
            .env("MDOC_KEPT_PAGES", dir.path().join("kept"))
            .env("TMP", &temporary)
            .env("TEMP", &temporary)
            .env("TMPDIR", &temporary)
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&child.stdout);
        assert!(
            child.status.success() && output.contains("test result: ok. 1 passed"),
            "{output}{}",
            String::from_utf8_lossy(&child.stderr)
        );
        let left: Vec<_> = std::fs::read_dir(&temporary)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    /// Without a folder, pages are recognized and none is read or kept.
    #[test]
    fn without_a_folder_nothing_is_read_or_kept() {
        let dir = tempfile::tempdir().unwrap();
        let kept = KeptPages::new(dir.path().into());
        let folder = kept.folder("abc", &config());
        folder.open();
        folder.store(&page(1));
        let recognized = std::cell::Cell::new(0);
        let pages = recognize_pages(
            2,
            None,
            &AtomicBool::new(false),
            |_| true,
            |n| {
                recognized.set(recognized.get() + 1);
                Ok(page(n))
            },
        )
        .unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(recognized.get(), 2, "the kept page 1 is not read");
        assert_eq!(
            std::fs::read_dir(&folder.0).unwrap().count(),
            1,
            "nothing new kept"
        );
    }

    /// On the OCR fixtures, an uninterrupted run gives the desktop's
    /// whole-document Markdown and warning, and a run cancelled after its
    /// first pages resumes with the rest only and gives the same. The
    /// scanned running header and footer go from the pages after the first
    /// across pages recognized in separate runs, kept or not, as they do in
    /// the desktop's single run, with the same count in the note; the
    /// header's sentence in a sparse page's body stays on both.
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
            (
                fixtures.join("import/handmade-scanned-running-header.pdf"),
                2,
            ),
            (fixtures.join("import/handmade-running-header.pdf"), 1),
            // The sparse page 4 is recognized in a run of its own (#925 M16).
            (
                fixtures.join("import/handmade-scanned-sparse-page-header.pdf"),
                3,
            ),
            (fixtures.join("import/handmade-sparse-page-header.pdf"), 1),
            // Letter and Legal pages; the sparse page 5 is recognized in a
            // run of its own (#925 M17).
            (
                fixtures.join("import/handmade-scanned-mixed-size-header.pdf"),
                4,
            ),
            (fixtures.join("import/handmade-mixed-size-header.pdf"), 1),
            // The header and its sentence right under it on sparse page 4
            // (#925 M18).
            (
                fixtures.join("import/handmade-sparse-page-closest-header.pdf"),
                1,
            ),
        ] {
            let name = pdf.display().to_string();
            let original = std::fs::read(&pdf).unwrap();
            let never = AtomicBool::new(false);
            let desktop = crate::import::prepare(&pdf, Some(&installed), false).unwrap();
            if name.ends_with("handmade-partly-scanned.pdf") {
                // OCR read the scanned pages, so their pictures count (#949).
                assert_eq!(desktop.left_out.pictures, 2, "{name}");
            }
            if name.contains("running-header") {
                assert!(desktop.left_out.repeated_lines >= 3, "{name}");
            }
            if name.contains("sparse-page-header") {
                // The header's sentence stays in page 4's body, uncounted.
                let header = "Annual Report of the Board 2026 Confidential";
                assert_eq!(desktop.markdown.matches(header).count(), 2, "{name}");
                assert_eq!(desktop.left_out.repeated_lines, 4, "{name}");
            }
            if name.contains("mixed-size-header") {
                // The header's sentence stays in page 5's body, uncounted.
                let header = "Annual Report of the Board 2026 Confidential";
                assert_eq!(desktop.markdown.matches(header).count(), 2, "{name}");
                assert_eq!(desktop.left_out.repeated_lines, 6, "{name}");
            }
            if name.contains("closest-header") {
                // Page 4's header goes, its sentence stays, uncounted.
                let header = "Annual Report of the Board 2026 Confidential";
                assert_eq!(desktop.markdown.matches(header).count(), 2, "{name}");
                assert_eq!(desktop.left_out.repeated_lines, 5, "{name}");
            }
            let dir = tempfile::tempdir().unwrap();
            let kept = KeptPages::new(dir.path().join("imports"));
            let (whole, completed) = prepare_resumable(
                &pdf,
                None,
                Some(&installed),
                false,
                &never,
                &kept,
                &|_, _| {},
            )
            .unwrap();
            // Kept until the caller takes the result.
            assert!(dir.path().join("imports").read_dir().unwrap().any(|_| true));
            completed.unwrap().remove();
            assert_eq!(whole.markdown, desktop.markdown, "{name}");
            assert_eq!(whole.warning, desktop.warning, "{name}");
            assert_eq!(whole.left_out, desktop.left_out, "{name}");
            assert_eq!(whole.ocr_configuration, desktop.ocr_configuration, "{name}");
            assert!(!dir.path().join("imports").read_dir().unwrap().any(|_| true));

            let cancel = AtomicBool::new(false);
            let first = std::sync::Mutex::new(Vec::new());
            let stopped = prepare_resumable(
                &pdf,
                None,
                Some(&installed),
                false,
                &cancel,
                &kept,
                &|page, _| {
                    first.lock().unwrap().push(page);
                    if page == stop_after {
                        cancel.store(true, Ordering::Relaxed);
                    }
                },
            );
            assert!(matches!(stopped, Err(ImportError::Cancelled)), "{name}");
            let first = first.into_inner().unwrap();
            assert_eq!(first, (1..=stop_after).collect::<Vec<_>>(), "{name}");

            let second = std::sync::Mutex::new(Vec::new());
            let (resumed, completed) = prepare_resumable(
                &pdf,
                None,
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
            assert_eq!(resumed.left_out, desktop.left_out, "{name}");
            completed.unwrap().remove();
            assert!(!dir.path().join("imports").read_dir().unwrap().any(|_| true));
            assert_eq!(std::fs::read(&pdf).unwrap(), original, "{name}");
        }
    }
}
