#![allow(dead_code)]
// Policy-only host benchmark. Reuse the exact generic transaction types, without GPUI.
extern crate self as mdoc_editor;
#[path = "transactions.rs"]
mod transactions;
pub use transactions::{EditorTransaction, HistoryChange, SourceEdit, inverse_edits};
mod policy;
use policy::*;
use std::{collections::HashMap, sync::Arc, time::Instant};
fn median(mut times: Vec<f64>) -> f64 {
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}
fn main() {
    for groups in [1, 1000, 20000] {
        let mut source = String::new();
        let mut detections = Vec::new();
        for n in 0..20000 {
            let start = source.len();
            source.push_str(&format!("Name{:05}", n % groups));
            detections.push(Detection {
                range: start..source.len(),
                category: Category::Person,
                score: 0.9,
                recognizer: Recognizer::Model,
            });
            source.push_str(" ordinary prose.\n");
        }
        source.push_str(&" ".repeat(2097152 - source.len()));
        let mut r = Review::default();
        let start = Instant::now();
        r.ingest(&source, detections).unwrap();
        let ingest = start.elapsed().as_secs_f64() * 1000.;
        let mut refresh = Vec::new();
        let mut planning = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            r.refresh(&source);
            refresh.push(start.elapsed().as_secs_f64() * 1000.);
            let start = Instant::now();
            std::hint::black_box(r.plan_all(&source, None).unwrap());
            planning.push(start.elapsed().as_secs_f64() * 1000.);
        }
        let texts: HashMap<_, _> = r
            .groups
            .iter()
            .map(|g| {
                (
                    g.original.clone(),
                    (g.original.clone(), Arc::<str>::from(g.replacement.as_str())),
                )
            })
            .collect();
        let edits = r.plan_all(&source, None).unwrap();
        let plans: Vec<_> = edits
            .iter()
            .map(|(range, _)| {
                let (a, b) = &texts[&source[range.clone()]];
                (range.clone(), a.clone(), b.clone(), Category::Person)
            })
            .collect();
        let start = Instant::now();
        let added = r.tracking.prepare(&plans);
        r.tracking.on_transaction(&EditorTransaction {
            revision: 1,
            changes: vec![HistoryChange {
                before: 0,
                after: 1,
                edits: edits
                    .iter()
                    .map(|(range, value)| SourceEdit {
                        range: range.clone(),
                        new_len: value.len(),
                    })
                    .collect(),
            }],
            retained: vec![0, 1],
        });
        r.tracking.commit(1, added);
        let commit = start.elapsed().as_secs_f64() * 1000.;
        let mut shift = Vec::new();
        for n in 2..103 {
            let start = Instant::now();
            r.tracking.on_transaction(&EditorTransaction {
                revision: n,
                changes: vec![HistoryChange {
                    before: n - 1,
                    after: n,
                    edits: vec![SourceEdit {
                        range: 0..0,
                        new_len: 1,
                    }],
                }],
                retained: vec![0, 1, n],
            });
            shift.push(start.elapsed().as_secs_f64() * 1000.);
        }
        println!(
            "groups={groups}, source={}, mentions={}, ingest_ms={ingest:.3}, refresh_median_ms={:.3}, plan_median_ms={:.3}, provenance_commit_ms={commit:.3}, shift_median_ms={:.3}",
            source.len(),
            edits.len(),
            median(refresh),
            median(planning),
            median(shift)
        );
    }
}
