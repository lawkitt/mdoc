use anyhow::Result;
use gliner::model::{GLiNER, input::text::TextInput, params::Parameters, pipeline::span::SpanMode};
use mdoc_pseudonymization_qualification::{Prediction, WorkerArgs, windows};
use orp::params::RuntimeParameters;
use std::time::Instant;

const LABELS: &[(&str, &str)] = &[
    ("person", "PERSON"),
    ("organization", "ORG"),
    ("email", "EMAIL"),
    ("phone number", "PHONE"),
    ("address", "ADDRESS"),
    ("passport number", "IDENTITY"),
    ("social security number", "IDENTITY"),
    ("tax identification number", "TAX"),
    ("bank account number", "BANK"),
    ("IBAN", "BANK"),
    ("bank routing number", "BANK"),
];

// Prefer the PII model card's vocabulary for its trained categories.
const PII_LABELS: &[(&str, &str)] = &[
    ("name", "PERSON"),
    ("organization", "ORG"),
    ("email address", "EMAIL"),
    ("phone number", "PHONE"),
    ("location address", "ADDRESS"),
    ("passport number", "IDENTITY"),
    ("ssn", "IDENTITY"),
    ("tax identification number", "TAX"),
    ("bank account", "BANK"),
    ("IBAN", "BANK"),
    ("routing number", "BANK"),
];

fn main() -> Result<()> {
    let args = WorkerArgs::read("gline")?;
    let vocabulary = if args.model.id == "pii-base-q8" {
        PII_LABELS
    } else {
        LABELS
    };
    let onnx = args
        .model
        .files
        .iter()
        .find(|f| f.path.ends_with(".onnx"))
        .expect("pinned ONNX");
    let started = Instant::now();
    let model = GLiNER::<SpanMode>::new(
        Parameters::default()
            .with_threshold(args.threshold)
            .with_max_length(Some(512)),
        RuntimeParameters::default(),
        args.root.join("tokenizer.json"),
        args.root.join(&onnx.path),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    let labels: Vec<_> = vocabulary.iter().map(|(label, _)| *label).collect();
    args.evaluate(
        model,
        "gline-rs=1.1.0; orp=1.0.0; ort=2.0.0-rc.9",
        load_ms,
        serde_json::json!({ "labels": vocabulary, "max_length": 512, "max_width": 12, "flat_ner": true, "splitter": "regex words", "provider": "CPU" }),
        |model, source| {
            let mut predictions = Vec::new();
            for (base, end) in windows(source, 128, 32)? {
                let input = TextInput::from_str(&[&source[base..end]], &labels)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let output = model.inference(input).map_err(|e| anyhow::anyhow!("{e}"))?;
                for span in output.spans.into_iter().flatten() {
                    let (start, end) = span.offsets();
                    let category = vocabulary
                        .iter()
                        .find(|(label, _)| *label == span.class())
                        .expect("requested label")
                        .1;
                    predictions.push(Prediction {
                        category: category.into(),
                        label: span.class().into(),
                        start: base + start,
                        end: base + end,
                        text: span.text().into(),
                        score: span.probability(),
                    });
                }
            }
            Ok(predictions)
        },
    )
}
