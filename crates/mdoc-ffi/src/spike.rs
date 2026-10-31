//! The mobile feasibility spike (ADR 0085): the core's detector and OCR on
//! the shared fixtures, measured against the detector budgets. The Swift
//! harness (`tools/ios-spike`) and the Mac stand-in (`examples/spike.rs`)
//! both call [`run`], so every platform measures the same code.
use crate::{CoreError, memory::Memory};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

/// ADR 0085's detector budgets.
const PEAK_MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const COLD_LOAD_SECONDS: f64 = 10.;
const TWENTY_PAGE_SCAN_SECONDS: f64 = 60.;
/// `legal-opinion.docx` comes from a 37-page document
/// (tests/fixtures/docx-preview/README.md); its first 20/37 of words stand in
/// for a 20-page document.
const LEGAL_OPINION_PAGES: usize = 37;
const PAGES: usize = 20;
const PROBE_RUNS: usize = 5;

/// Where [`run`] finds its inputs. Every path is read, never written.
#[derive(Clone, Debug, uniffi::Record)]
pub struct SpikeInput {
    /// A copy of the fixtures, flat: `legal-opinion.docx`, `english.pdf`,
    /// `english.txt`, `russian.pdf` and `russian.txt`.
    pub fixtures: String,
    /// The OCR root in the desktop layout (`models/<id>/<revision>/…`).
    pub ocr_root: String,
    /// The GLiNER2 FP16 model folder (`gliner2-pii-fp16-<revision>`).
    pub model_root: String,
    pub runtimes: Runtimes,
}

/// The native libraries the core loads.
#[derive(Clone, Debug, serde::Serialize, uniffi::Record)]
pub struct Runtimes {
    pub pdfium: String,
    pub onnx: String,
}

/// The runtimes that ship inside the app. iOS: linked into it. Android: the
/// APK's own `libpdfium.so` and `libonnxruntime.so` beside this library in
/// the app's native library folder, which needs the APK to extract its
/// libraries (`useLegacyPackaging`): PDFium is opened by file path (ADR
/// 0089). Elsewhere there are none, and callers pass the desktop's
/// downloaded libraries instead.
#[uniffi::export]
pub fn linked_runtimes() -> Result<Runtimes, CoreError> {
    #[cfg(target_os = "ios")]
    {
        let linked = mdoc_import::ocr::linked_runtimes()?;
        Ok(Runtimes {
            pdfium: linked.pdfium.display().to_string(),
            onnx: linked.onnx.display().to_string(),
        })
    }
    #[cfg(target_os = "android")]
    {
        let dir = android::library_dir().ok_or_else(|| CoreError::Ocr {
            message: "The app's native libraries are not extracted.".into(),
        })?;
        let runtimes = Runtimes {
            pdfium: dir.join("libpdfium.so").display().to_string(),
            onnx: dir.join("libonnxruntime.so").display().to_string(),
        };
        for library in [&runtimes.pdfium, &runtimes.onnx] {
            if !Path::new(library).is_file() {
                return Err(CoreError::Ocr {
                    message: format!(
                        "{library} is missing: the app's native libraries are not extracted."
                    ),
                });
            }
        }
        Ok(runtimes)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    Err(CoreError::Ocr {
        message: "Runtimes ship inside the app only on iOS and Android.".into(),
    })
}

/// Where this library was loaded from, as the dynamic linker reports it.
#[cfg(target_os = "android")]
mod android {
    use std::{
        ffi::{CStr, c_char, c_int, c_void},
        path::PathBuf,
    };

    #[repr(C)]
    struct DlInfo {
        fname: *const c_char,
        fbase: *mut c_void,
        sname: *const c_char,
        saddr: *mut c_void,
    }

    unsafe extern "C" {
        fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
    }

    /// The folder holding `libmdoc_ffi.so`; `None` when the linker reports
    /// none, or a path inside the APK (libraries not extracted).
    pub fn library_dir() -> Option<PathBuf> {
        let mut info = DlInfo {
            fname: std::ptr::null(),
            fbase: std::ptr::null_mut(),
            sname: std::ptr::null(),
            saddr: std::ptr::null_mut(),
        };
        let address = library_dir as *const c_void;
        // SAFETY: dladdr only reads the address and fills `info`; fname, when
        // set, points to the linker's own NUL-terminated string.
        if unsafe { dladdr(address, &mut info) } == 0 || info.fname.is_null() {
            return None;
        }
        // SAFETY: checked non-null above; the linker keeps it alive.
        let file = PathBuf::from(unsafe { CStr::from_ptr(info.fname) }.to_str().ok()?);
        file.is_file()
            .then(|| file.parent().map(PathBuf::from))
            .flatten()
    }
}

#[derive(Clone, Debug, serde::Serialize, uniffi::Record)]
pub struct SpikeReport {
    pub core_version: String,
    pub runtimes: Runtimes,
    pub memory_start: Memory,
    pub document: Conversion,
    pub probe: Option<Probe>,
    pub probe_error: Option<String>,
    /// Peak so far: conversion and the probe's detector.
    pub memory_after_probe: Memory,
    pub scan: Option<Scan>,
    pub scan_error: Option<String>,
    /// Peak so far: everything before OCR, so the detector's peak.
    pub memory_after_scan: Memory,
    pub ocr: Vec<OcrRun>,
    pub memory_end: Memory,
    /// Thermal state is the platform's to add; the core cannot read it.
    pub budgets: Vec<Budget>,
}

/// The 20-page document: `legal-opinion.docx` converted in-process.
#[derive(Clone, Debug, serde::Serialize, uniffi::Record)]
pub struct Conversion {
    pub source: String,
    pub conversion_seconds: f64,
    pub words: u64,
    pub pages: u64,
    pub page_words: u64,
    pub page_bytes: u64,
}

/// One full 512-token window, timed on its own.
#[derive(Clone, Debug, serde::Serialize, uniffi::Record)]
pub struct Probe {
    pub tokens: u64,
    pub bytes: u64,
    /// ONNX Runtime into ort plus the first GLiNER2 sessions in the process.
    pub cold_load_seconds: f64,
    pub runtime_seconds: f64,
    pub engine_load_seconds: f64,
    pub run_seconds: Vec<f64>,
    pub median_seconds: f64,
}

/// The real scan path over the 20-page document.
#[derive(Clone, Debug, serde::Serialize, uniffi::Record)]
pub struct Scan {
    pub total_seconds: f64,
    pub verify_seconds: f64,
    pub tokenizer_seconds: f64,
    pub windowing_seconds: f64,
    pub engine_load_seconds: f64,
    pub inference_seconds: f64,
    pub structured_seconds: f64,
    pub windows: u64,
    pub mean_window_tokens: f64,
    pub mean_window_seconds: f64,
    pub max_window_seconds: f64,
    pub detections: u64,
}

#[derive(Clone, Debug, serde::Serialize, uniffi::Record)]
pub struct OcrRun {
    pub fixture: String,
    pub bundle: String,
    /// Model check plus recognition; the first run in a process also loads
    /// PDFium and builds the OCR sessions.
    pub seconds: f64,
    pub markdown: String,
    pub warning: Option<String>,
    /// The recognized words equal the fixture's `.txt` words, ignoring
    /// heading markers and spacing (the desktop OCR smoke test's rule).
    pub matches_expected: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, uniffi::Record)]
pub struct Budget {
    pub name: String,
    pub limit: f64,
    pub measured: Option<f64>,
    pub pass: bool,
}

/// Only the one-window probe, on the same 20 pages, under whatever detector
/// settings the platform set in the environment (`GLINER2_DEVICE`,
/// `MDOC_DETECT_THREADS`): the Android spike times threads and XNNPACK with it.
#[uniffi::export]
pub fn probe(input: SpikeInput) -> Result<Probe, CoreError> {
    let fixtures = PathBuf::from(&input.fixtures);
    let (_, text) = twenty_pages(&fixtures.join("legal-opinion.docx"))?;
    let config = mdoc_store::settings::PiiConfig::default();
    let probe = mdoc_detect::probe_window(
        &text,
        &PathBuf::from(&input.model_root),
        &PathBuf::from(&input.runtimes.onnx),
        &config,
        PROBE_RUNS,
    )?;
    Ok(probe_report(&probe))
}

/// Runs the whole spike once. Only conversion failing stops it; a detector
/// or OCR failure is recorded in the report.
#[uniffi::export]
pub fn run(input: SpikeInput) -> Result<SpikeReport, CoreError> {
    let fixtures = PathBuf::from(&input.fixtures);
    let onnx = PathBuf::from(&input.runtimes.onnx);
    let model_root = PathBuf::from(&input.model_root);
    let memory_start = crate::memory::memory();
    let (document, text) = twenty_pages(&fixtures.join("legal-opinion.docx"))?;
    let config = mdoc_store::settings::PiiConfig::default();
    let (probe, probe_error) =
        match mdoc_detect::probe_window(&text, &model_root, &onnx, &config, PROBE_RUNS) {
            Ok(probe) => (Some(probe_report(&probe)), None),
            Err(error) => (None, Some(error.to_string())),
        };
    let memory_after_probe = crate::memory::memory();
    let (scan, scan_error) =
        match mdoc_detect::scan_at(&text, &AtomicBool::new(false), &model_root, &onnx, &config) {
            Ok((detections, timings)) => (Some(scan_report(detections.len(), &timings)), None),
            Err(error) => (None, Some(error.to_string())),
        };
    let memory_after_scan = crate::memory::memory();
    let ocr = [
        ("english", mdoc_store::settings::OcrModel::V6Small),
        ("russian", mdoc_store::settings::OcrModel::Cyrillic),
    ]
    .into_iter()
    .map(|(fixture, model)| ocr_run(&fixtures, fixture, model, &input))
    .collect();
    let memory_end = crate::memory::memory();
    let budgets = budgets(
        memory_after_scan,
        probe.as_ref(),
        scan.as_ref(),
        scan_error.as_deref(),
    );
    Ok(SpikeReport {
        core_version: env!("CARGO_PKG_VERSION").into(),
        runtimes: input.runtimes,
        memory_start,
        document,
        probe,
        probe_error,
        memory_after_probe,
        scan,
        scan_error,
        memory_after_scan,
        ocr,
        memory_end,
        budgets,
    })
}

/// The report as pretty JSON, the harness's output file.
#[uniffi::export]
pub fn report_json(report: SpikeReport) -> String {
    serde_json::to_string_pretty(&report).expect("a report serializes")
}

fn twenty_pages(docx: &Path) -> Result<(Conversion, String), CoreError> {
    let started = Instant::now();
    let imported =
        mdoc_import::import::prepare_cancellable(docx, None, true, &AtomicBool::new(false))?;
    let conversion_seconds = started.elapsed().as_secs_f64();
    let words = imported.markdown.split_whitespace().count();
    let text = first_words(&imported.markdown, words * PAGES / LEGAL_OPINION_PAGES).to_owned();
    Ok((
        Conversion {
            source: docx.display().to_string(),
            conversion_seconds,
            words: words as u64,
            pages: PAGES as u64,
            page_words: text.split_whitespace().count() as u64,
            page_bytes: text.len() as u64,
        },
        text,
    ))
}

/// `text` up to the end of the line holding its `count`th word.
fn first_words(text: &str, count: usize) -> &str {
    if count == 0 {
        return "";
    }
    let mut seen = 0;
    let mut in_word = false;
    for (index, c) in text.char_indices() {
        if c.is_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            seen += 1;
            if seen == count {
                let end = text[index..]
                    .find('\n')
                    .map_or(text.len(), |line| index + line + 1);
                return &text[..end];
            }
        }
    }
    text
}

fn seconds(duration: Duration) -> f64 {
    duration.as_secs_f64()
}

fn probe_report(probe: &mdoc_detect::WindowProbe) -> Probe {
    let mut runs: Vec<f64> = probe.runs.iter().copied().map(seconds).collect();
    let run_seconds = runs.clone();
    runs.sort_by(f64::total_cmp);
    Probe {
        tokens: probe.tokens as u64,
        bytes: probe.bytes as u64,
        cold_load_seconds: seconds(probe.runtime + probe.engine_load),
        runtime_seconds: seconds(probe.runtime),
        engine_load_seconds: seconds(probe.engine_load),
        run_seconds,
        median_seconds: runs.get(runs.len() / 2).copied().unwrap_or(0.),
    }
}

fn scan_report(detections: usize, timings: &mdoc_detect::ScanTimings) -> Scan {
    let windows = timings.windows.len().max(1) as f64;
    let inference: Duration = timings.windows.iter().map(|w| w.inference).sum();
    Scan {
        total_seconds: seconds(timings.total),
        verify_seconds: seconds(timings.verify),
        tokenizer_seconds: seconds(timings.tokenizer),
        windowing_seconds: seconds(timings.windowing),
        engine_load_seconds: seconds(timings.engine_load),
        inference_seconds: seconds(inference),
        structured_seconds: seconds(timings.structured),
        windows: timings.windows.len() as u64,
        mean_window_tokens: timings.windows.iter().map(|w| w.tokens as f64).sum::<f64>() / windows,
        mean_window_seconds: seconds(inference) / windows,
        max_window_seconds: timings
            .windows
            .iter()
            .map(|w| seconds(w.inference))
            .fold(0., f64::max),
        detections: detections as u64,
    }
}

fn ocr_run(
    fixtures: &Path,
    fixture: &str,
    model: mdoc_store::settings::OcrModel,
    input: &SpikeInput,
) -> OcrRun {
    let started = Instant::now();
    let result = recognize(fixtures, fixture, model, input);
    let seconds = started.elapsed().as_secs_f64();
    let expected = std::fs::read_to_string(fixtures.join(format!("{fixture}.txt")));
    let (markdown, warning, error) = match result {
        Ok(imported) => (imported.markdown, imported.warning, None),
        Err(error) => (String::new(), None, Some(error.to_string())),
    };
    OcrRun {
        fixture: format!("{fixture}.pdf"),
        bundle: model.name().into(),
        seconds,
        matches_expected: expected.is_ok_and(|expected| same_words(&markdown, &expected)),
        markdown,
        warning,
        error,
    }
}

fn recognize(
    fixtures: &Path,
    fixture: &str,
    model: mdoc_store::settings::OcrModel,
    input: &SpikeInput,
) -> Result<mdoc_import::import::Imported, CoreError> {
    let installed = mdoc_import::ocr::installed_at(
        Path::new(&input.ocr_root),
        &mdoc_store::settings::OcrConfig {
            model,
            ..Default::default()
        },
        PathBuf::from(&input.runtimes.pdfium),
        PathBuf::from(&input.runtimes.onnx),
    )?;
    Ok(mdoc_import::import::prepare_cancellable(
        &fixtures.join(format!("{fixture}.pdf")),
        Some(&installed),
        false,
        &AtomicBool::new(false),
    )?)
}

fn same_words(markdown: &str, expected: &str) -> bool {
    let plain = markdown
        .lines()
        .map(|line| line.trim_start_matches('#').trim())
        .collect::<Vec<_>>()
        .join(" ");
    plain.split_whitespace().eq(expected.split_whitespace())
}

fn budgets(
    detector: Memory,
    probe: Option<&Probe>,
    scan: Option<&Scan>,
    scan_error: Option<&str>,
) -> Vec<Budget> {
    let budget = |name: &str, limit: f64, measured: Option<f64>| Budget {
        name: name.into(),
        limit,
        measured,
        pass: measured.is_some_and(|measured| measured <= limit),
    };
    let gib = |bytes: u64| bytes as f64 / f64::from(1u32 << 30);
    vec![
        budget(
            "detector peak memory (GiB)",
            gib(PEAK_MEMORY_BYTES),
            (detector.peak_footprint_bytes > 0).then(|| gib(detector.peak_footprint_bytes)),
        ),
        budget(
            "cold load (s)",
            COLD_LOAD_SECONDS,
            probe.map(|probe| probe.cold_load_seconds),
        ),
        budget(
            "20-page scan (s)",
            TWENTY_PAGE_SCAN_SECONDS,
            scan.map(|scan| scan.total_seconds)
                .or_else(|| scan_error.is_some().then_some(f64::INFINITY)),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_pages_end_on_a_line_boundary() {
        let text = "one two\nthree four five\nsix\n";
        assert_eq!(first_words(text, 0), "");
        assert_eq!(first_words(text, 2), "one two\n");
        assert_eq!(first_words(text, 3), "one two\nthree four five\n");
        assert_eq!(first_words(text, 5), "one two\nthree four five\n");
        assert_eq!(first_words(text, 9), text);
    }

    #[test]
    fn expected_text_ignores_heading_markers_and_spacing() {
        assert!(same_words("# Title\n\nBody  text\n", "Title Body\ntext"));
        assert!(!same_words("# Title\n", "Title Body"));
    }

    #[test]
    fn budgets_pass_only_measured_values_within_limits() {
        let memory = Memory {
            footprint_bytes: 1,
            peak_footprint_bytes: PEAK_MEMORY_BYTES,
        };
        let rows = budgets(memory, None, None, Some("Scan exceeded two minutes."));
        assert!(rows[0].pass);
        assert_eq!(rows[0].measured, Some(2.));
        assert!(!rows[1].pass && rows[1].measured.is_none());
        assert!(!rows[2].pass);
        assert_eq!(rows[2].measured, Some(f64::INFINITY));
    }

    /// The legal opinion converts in-process on every platform, as iOS
    /// needs (no DOCX preview worker).
    #[test]
    fn legal_opinion_yields_twenty_of_thirty_seven_pages() {
        let docx = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/docx-preview/legal-opinion.docx");
        let (document, text) = twenty_pages(&docx).unwrap();
        let expected = document.words as usize * PAGES / LEGAL_OPINION_PAGES;
        let words = text.split_whitespace().count();
        assert!(words >= expected && words < expected + 400, "{words} words");
        assert_eq!(document.page_bytes as usize, text.len());
    }
}
