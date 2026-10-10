//! Spellcheck evidence over the OCR fixtures (ADR 0036): flags on the
//! ground-truth transcripts are false positives; flags on the prepared OCR
//! output mix real recognition errors with the same false positives.
//!
//! `cargo run --release -p mdoc-spell --example fixture_report [-- --words]`
use mdoc_spell::{Exclusions, Options, Speller};
use std::{path::Path, time::Instant};

fn main() {
    let show_words = std::env::args().any(|a| a == "--words");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ocr-upstream");
    let started = Instant::now();
    let speller = Speller::load(Options::default());
    println!("load (en_US, en_GB, ru_RU): {:?}", started.elapsed());
    let mut names: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".txt"))
        .collect();
    names.sort();
    println!("| Fixture | Words | Ground-truth flags | OCR-output flags |\n|---|---:|---:|---:|");
    let (mut words_total, mut truth_total, mut ocr_total) = (0, 0, 0);
    for name in names {
        let stem = name.trim_end_matches(".txt");
        let truth = std::fs::read_to_string(root.join(&name)).unwrap();
        let ocr =
            std::fs::read_to_string(root.join(format!("captures/baseline/{stem}.prepared.md")))
                .unwrap_or_default();
        let words = truth.split_whitespace().count();
        let truth_flags = speller.check(&truth, Exclusions::default());
        let ocr_flags = speller.check(&ocr, Exclusions::default());
        println!(
            "| {stem} | {words} | {} | {} |",
            truth_flags.len(),
            ocr_flags.len()
        );
        if show_words {
            let list = |text: &str, flags: &[std::ops::Range<usize>]| {
                flags
                    .iter()
                    .map(|r| text[r.clone()].to_owned())
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            println!("|  truth: {} |", list(&truth, &truth_flags));
            println!("|  ocr: {} |", list(&ocr, &ocr_flags));
        }
        words_total += words;
        truth_total += truth_flags.len();
        ocr_total += ocr_flags.len();
    }
    println!("| **Total** | {words_total} | {truth_total} | {ocr_total} |");
}
