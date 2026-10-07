//! Read-only comparison of exact structured rules and captured model predictions.
use anyhow::{Context, Result, ensure};
use mdoc_pseudonymization_qualification::{
    Fixture, Report, assess, expand,
    rules::{Rules, resolve},
    sha256_file,
};
use serde_json::json;
use std::{fs, path::PathBuf, time::Instant};

fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(Into::into).collect();
    ensure!(args.len() == 3, "hybrid CORPUS MODEL_REPORT OUTPUT");
    ensure!(!args[2].exists(), "output already exists");
    let report: Report = serde_json::from_slice(&fs::read(&args[1])?)?;
    ensure!(
        report.corpus_sha256 == sha256_file(&args[0])?,
        "model corpus hash differs"
    );
    let process: serde_json::Value =
        serde_json::from_slice(&fs::read(args[1].with_extension("process.json"))?)?;
    ensure!(
        process["success"] == true && process["deadline_exceeded"] == false,
        "model process did not complete"
    );
    let corpus: Vec<Fixture> = serde_json::from_slice(&fs::read(&args[0])?)?;
    ensure!(
        corpus.len() == report.fixtures.len(),
        "model fixture count differs"
    );
    let started = Instant::now();
    let rules = Rules::new()?;
    let rule_build_ms = started.elapsed().as_secs_f64() * 1000.;
    let mut fixtures = Vec::new();
    for fixture in corpus {
        let model = report
            .fixtures
            .iter()
            .find(|f| f.id == fixture.id)
            .context("missing model fixture")?;
        let (source, gold) = expand(&fixture.annotated, fixture.repeat)?;
        ensure!(
            model.error.is_none()
                && model.repeat_deterministic
                && model.assessment.invalid_offsets.is_empty(),
            "invalid or failed model result for {}",
            fixture.id
        );
        // Read-only source hash verified independently, not merely by fixture ID.
        let temp = tempfile::NamedTempFile::new()?;
        fs::write(temp.path(), &source)?;
        ensure!(
            sha256_file(temp.path())? == model.source_sha256,
            "source hash differs"
        );
        let mut times = Vec::new();
        let rule_predictions = rules.scan(&source);
        for _ in 0..101 {
            let start = Instant::now();
            let result = rules.scan(std::hint::black_box(&source));
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            ensure!(
                serde_json::to_vec(&result)? == serde_json::to_vec(&rule_predictions)?,
                "nondeterministic rules"
            );
            times.push(elapsed);
        }
        times.sort_by(f64::total_cmp);
        let model_predictions = resolve(&source, &[], &model.predictions);
        let start = Instant::now();
        let combined = resolve(&source, &rule_predictions, &model.predictions);
        let merge_ms = start.elapsed().as_secs_f64() * 1000.;
        let mut ablations = serde_json::Map::new();
        for label in [
            "rule:email-format",
            "rule:phone-format-context",
            "rule:inn-checksum-context",
            "rule:snils-checksum-context",
            "rule:ru-account-format-context",
            "rule:ru-bic-format-context",
            "rule:iban-mod97-country-length",
        ] {
            let selected: Vec<_> = rule_predictions
                .iter()
                .filter(|p| p.label == label)
                .cloned()
                .collect();
            ablations.insert(
                label.into(),
                serde_json::to_value(assess(
                    &source,
                    &gold,
                    &resolve(&source, &selected, &model.predictions),
                ))?,
            );
        }
        let selected: Vec<_> = rule_predictions
            .iter()
            .filter(|p| matches!(p.category.as_str(), "EMAIL" | "TAX" | "IDENTITY"))
            .cloned()
            .collect();
        let selected_hybrid = resolve(&source, &selected, &model.predictions);
        fixtures.push(json!({
            "id":fixture.id, "language":fixture.language, "kind":fixture.kind,
            "source_bytes":source.len(), "source_sha256":model.source_sha256,
            "rules_median_ms":times[50], "rules_p95_ms":times[95], "merge_ms":merge_ms,
            "model_latency_ms":model.latency_ms, "raw_model_assessment":model.assessment,
            "individual_rule_ablations":ablations,
            "pipelines": {
                "model": {"assessment":assess(&source,&gold,&model_predictions), "predictions":model_predictions},
                "rules": {"assessment":assess(&source,&gold,&rule_predictions), "predictions":rule_predictions},
                "hybrid": {"assessment":assess(&source,&gold,&combined), "predictions":combined},
                "selected_hybrid": {"assessment":assess(&source,&gold,&selected_hybrid), "predictions":selected_hybrid}
            }
        }));
    }
    let seed = "ИНН: 7707083893; contact: test@example.invalid.\n";
    let mut dense = seed.repeat(10_000);
    let target_bytes = 2 * 1024 * 1024;
    let padding = " Ordinary legal prose.";
    dense.push_str(&padding.repeat((target_bytes - dense.len()) / padding.len()));
    dense.push_str(&" ".repeat(target_bytes - dense.len()));
    ensure!(dense.len() == target_bytes, "incorrect stress source size");
    let mut dense_times = Vec::new();
    let mut count = 0;
    for _ in 0..7 {
        let start = Instant::now();
        let output = rules.scan(std::hint::black_box(&dense));
        count = output.len();
        ensure!(count == 20_000, "incorrect stress occurrence count");
        ensure!(
            output.iter().all(|p| p.valid_for(&dense)),
            "invalid stress offsets"
        );
        dense_times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    dense_times.sort_by(f64::total_cmp);
    fs::write(
        &args[2],
        serde_json::to_vec_pretty(&json!({
            "corpus_sha256":report.corpus_sha256, "model_report_sha256":sha256_file(&args[1])?,
            "model_configuration":report.configuration,"threshold":report.threshold,
            "rule_build_ms":rule_build_ms,
            "policy":"rules take precedence; GLiNER conflicts use score/length/start; no heuristic score calibration",
            "stress":{"source_bytes":dense.len(),"occurrences":count,"latency_ms":dense_times,"median_ms":dense_times[3]},
            "fixtures":fixtures
        }))?,
    )?;
    Ok(())
}
