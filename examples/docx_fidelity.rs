//! The DOCX fidelity check (`tests/fixtures/docx-fidelity/README.md`): renders
//! each fixture as the Source pane does and scores it against Word's own PDF
//! export by page count and SSIM, rasterized with gpui-pdf's hayro.
//!
//! Usage: cargo run --example docx_fidelity -- <fixtures dir> [--update-baseline]
//!
//! Lists the reference PDFs still to export, prints a score per fixture and
//! fails when a fixture's score drops more than two points below the baseline
//! recorded for this platform at the last docxide bump.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{Arc, atomic::AtomicBool},
};

/// Points a score may drop below its baseline (the comments grill, #927).
const TOLERANCE: f64 = 2.0;
/// 108 dpi: text strokes span a few pixels, so SSIM sees layout, not hinting.
const SCALE: f32 = 1.5;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next().map(PathBuf::from) else {
        eprintln!("usage: docx_fidelity <fixtures dir> [--update-baseline]");
        return ExitCode::from(2);
    };
    let update = args.next().as_deref() == Some("--update-baseline");
    match run(&dir, update) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

/// `Ok(false)` when a fixture dropped below its baseline.
fn run(dir: &Path, update: bool) -> Result<bool, String> {
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "docx"))
        .collect();
    fixtures.sort();
    let baseline_path = dir.join(format!("baseline-{}.tsv", std::env::consts::OS));
    let baseline = read_baseline(&baseline_path);
    let mut missing = Vec::new();
    let mut scores = BTreeMap::new();
    let mut passed = true;
    println!("fixture\tpages (docxide/Word)\tSSIM\tbaseline");
    for docx in &fixtures {
        let name = docx.file_stem().unwrap().to_string_lossy().into_owned();
        let reference = dir.join("reference").join(format!("{name}.pdf"));
        if !reference.is_file() {
            missing.push(reference);
            continue;
        }
        let preview = mdoc_import::docx_preview::render_in_process(
            docx,
            Arc::new(AtomicBool::new(false)),
            |_| Ok(Vec::new()),
        )
        .map_err(|e| format!("{name}: {e}"))?;
        let ours = pages(&preview.pdf_path)?;
        let word = pages(&reference)?;
        let score = score(&ours, &word);
        let previous = baseline.get(&name).copied();
        let verdict = match previous {
            Some(before) if score < before - TOLERANCE => {
                passed = false;
                format!("{before:.2}  DROPPED")
            }
            Some(before) => format!("{before:.2}"),
            None => "none".into(),
        };
        println!(
            "{name}\t{}/{}\t{score:.2}\t{verdict}",
            ours.len(),
            word.len()
        );
        scores.insert(name, score);
    }
    if !missing.is_empty() {
        println!(
            "\nWord reference PDFs to export (Word for Mac, File > Save As > PDF, \"Best for printing\"):"
        );
        for path in &missing {
            println!("  {}", path.display());
        }
    }
    // Nothing scored or nothing compared is not a pass: say so.
    let unbaselined = scores.keys().filter(|n| !baseline.contains_key(*n)).count();
    println!(
        "\n{} of {} fixtures scored; {} without a reference, {} without a baseline{}",
        scores.len(),
        fixtures.len(),
        missing.len(),
        unbaselined,
        if missing.is_empty() && unbaselined == 0 {
            ""
        } else {
            ": the check is incomplete"
        }
    );
    if update && !scores.is_empty() {
        let text: String = scores
            .iter()
            .map(|(name, score)| format!("{name}\t{score:.2}\n"))
            .collect();
        std::fs::write(&baseline_path, text).map_err(|e| e.to_string())?;
        println!("\nWrote {}", baseline_path.display());
    }
    Ok(passed)
}

fn read_baseline(path: &Path) -> BTreeMap<String, f64> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (name, score) = line.split_once('\t')?;
            Some((name.to_owned(), score.trim().parse().ok()?))
        })
        .collect()
}

type Page = (u32, u32, Vec<u8>);

fn pages(pdf: &Path) -> Result<Vec<Page>, String> {
    let bytes = std::fs::read(pdf).map_err(|e| format!("{}: {e}", pdf.display()))?;
    let doc = gpui_pdf::parse(Arc::new(bytes)).map_err(|e| format!("{}: {e:?}", pdf.display()))?;
    (0..doc.pages().len())
        .map(|i| gpui_pdf::page_luma(&doc, i, SCALE))
        .collect()
}

/// Mean page SSIM ×100 over the longer document: a page one side lacks scores
/// zero, so a different page count costs points.
fn score(ours: &[Page], word: &[Page]) -> f64 {
    let count = ours.len().max(word.len());
    if count == 0 {
        return 100.0;
    }
    let total: f64 = ours.iter().zip(word).map(|(a, b)| ssim(a, b)).sum();
    100.0 * total / count as f64
}

/// Mean SSIM of 8×8 windows (stride 4) over the area both pages cover.
fn ssim(a: &Page, b: &Page) -> f64 {
    let (w, h) = (a.0.min(b.0) as usize, a.1.min(b.1) as usize);
    let (c1, c2) = ((0.01f64 * 255.0).powi(2), (0.03f64 * 255.0).powi(2));
    let (mut total, mut windows) = (0.0, 0usize);
    for y0 in (0..h.saturating_sub(7)).step_by(4) {
        for x0 in (0..w.saturating_sub(7)).step_by(4) {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for y in y0..y0 + 8 {
                for x in x0..x0 + 8 {
                    let p = f64::from(a.2[y * a.0 as usize + x]);
                    let q = f64::from(b.2[y * b.0 as usize + x]);
                    sa += p;
                    sb += q;
                    saa += p * p;
                    sbb += q * q;
                    sab += p * q;
                }
            }
            let (ma, mb) = (sa / 64.0, sb / 64.0);
            let (va, vb) = (saa / 64.0 - ma * ma, sbb / 64.0 - mb * mb);
            let cov = sab / 64.0 - ma * mb;
            total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            windows += 1;
        }
    }
    if windows == 0 {
        1.0
    } else {
        total / windows as f64
    }
}
