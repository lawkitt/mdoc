use anyhow::{Result, ensure};
use gliner2_rs::{
    Chunker, ExecutionMode, InferenceParams, Precision, SchemaTask, SpanConfig, SpanEngine,
    processor::SchemaTransformer,
};
use mdoc_pseudonymization_qualification::{Prediction, WorkerArgs};
use std::{
    cell::RefCell,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

// Label families prevent names, contacts and identifiers competing in one task.
// organization is deliberately measured despite being outside the PII card's taxonomy.
const FAMILIES: &[&[(&str, &str)]] = &[
    &[("person", "PERSON"), ("full_name", "PERSON")],
    &[("organization", "ORG")],
    &[
        ("email", "EMAIL"),
        ("phone_number", "PHONE"),
        ("address", "ADDRESS"),
    ],
    &[
        ("passport_number", "IDENTITY"),
        ("national_id_number", "IDENTITY"),
        ("tax_id", "TAX"),
    ],
    &[
        ("bank_account", "BANK"),
        ("iban", "BANK"),
        ("routing_number", "BANK"),
    ],
];

fn main() -> Result<()> {
    let args = WorkerArgs::read("gliner2")?;
    ensure!(
        std::env::var("GLINER2_DEVICE").as_deref() == Ok("cpu"),
        "qualification requires GLINER2_DEVICE=cpu"
    );
    gliner2_rs::init("mdoc-qualification");
    let started = Instant::now();
    let engine = SpanEngine::new(
        SpanConfig::new(&args.root)
            .with_precision(Precision::Fp16)
            .with_execution(ExecutionMode::Standard)
            .with_intra_threads(4),
    )?;
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    let tasks: Vec<_> = FAMILIES
        .iter()
        .map(|family| {
            SchemaTask::Entities(family.iter().map(|(label, _)| label.to_string()).collect())
        })
        .collect();
    let params = InferenceParams {
        threshold: args.threshold,
        flat_ner: true,
        ..Default::default()
    };
    let transformer =
        SchemaTransformer::from_tokenizer_file(&args.root.join("fp16_v2/tokenizer.json"))?;
    args.evaluate(
        RefCell::new(engine),
        "gliner2-rs=0.9.6; ort=2.0.0-rc.13; hub disabled",
        load_ms,
        serde_json::json!({ "families": FAMILIES, "flat_ner": true, "precision": "Fp16", "execution": "Standard", "splitter": "app tokenizer-bounded windows", "max_schema_text_tokens": 512, "app_detector_sha256": "fc0553c5245b8a42ce8fb744614ee01a5b38fd2ff4a04c618f996c6acb03c38f", "provider": "CPU" }),
        |engine, source| {
            let cancel = AtomicBool::new(false);
            let scan_started = Instant::now();
            let windows = bounded_windows(source, &transformer, &tasks, &cancel, scan_started).map_err(anyhow::Error::msg)?;
            let mut entities = Vec::new();
            for range in windows {
                checkpoint(&cancel, scan_started).map_err(anyhow::Error::msg)?;
                let text = &source[range.clone()];
                let output = engine.borrow_mut().extract_with(text, &tasks, &params)?;
                for mut entity in output.entities {
                    ensure!(text.get(entity.char_start..entity.char_end) == Some(entity.text.as_str()), "invalid local offsets");
                    entity.char_start += range.start;
                    entity.char_end += range.start;
                    entities.push(entity);
                }
            }
            checkpoint(&cancel, scan_started).map_err(anyhow::Error::msg)?;
            Ok(entities
                .into_iter()
                .map(|entity| {
                    let category = FAMILIES
                        .iter()
                        .flat_map(|f| f.iter())
                        .find(|(label, _)| *label == entity.label)
                        .expect("requested label")
                        .1;
                    Prediction {
                        category: category.into(),
                        label: entity.label,
                        start: entity.char_start,
                        end: entity.char_end,
                        text: entity.text,
                        score: entity.score,
                    }
                })
                .collect())
        },
    )
}

const MAX_INPUT_TOKENS: usize = 512;
fn checkpoint(cancel: &AtomicBool, started: Instant) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) || started.elapsed().as_secs() >= 120 {
        return Err("cancelled or deadline reached; discard partial scan".into());
    }
    Ok(())
}
fn bounded_windows(
    source: &str,
    transformer: &SchemaTransformer,
    tasks: &[SchemaTask],
    cancel: &AtomicBool,
    started: Instant,
) -> Result<Vec<std::ops::Range<usize>>, String> {
    let mut pending: Vec<_> = Chunker::new(128, 32)
        .map_err(|e| e.to_string())?
        .split(source)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|chunk| chunk.byte_start..chunk.byte_end)
        .collect();
    let mut windows = Vec::new();
    while let Some(range) = pending.pop() {
        checkpoint(cancel, started)?;
        let record = transformer
            .transform(&source[range.clone()], tasks)
            .map_err(|e| e.to_string())?;
        if record.input_ids.len() <= MAX_INPUT_TOKENS {
            windows.push(range);
            continue;
        }
        // Subdivide using the model's own word splitter, and verify actual
        // schema+text tokens again. Reject a single pathological word rather
        // than truncate it or silently deliver partial detection coverage.
        let words = record.num_words();
        if words < 2 {
            return Err("A source word exceeds the model token limit. Add selections manually; no partial scan was accepted.".into());
        }
        let size = words.div_ceil(2);
        let overlap = (size / 4).min(size - 1);
        for chunk in Chunker::new(size, overlap)
            .map_err(|e| e.to_string())?
            .split(&source[range.clone()])
            .map_err(|e| e.to_string())?
        {
            pending.push(range.start + chunk.byte_start..range.start + chunk.byte_end);
        }
    }
    windows.sort_by_key(|range| range.start);
    Ok(windows)
}
