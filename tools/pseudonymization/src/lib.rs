//! Offline, read-only qualification shared by separately linked ONNX adapters.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Instant,
};

pub mod rules;

pub const CATEGORIES: &[&str] = &[
    "PERSON", "ORG", "EMAIL", "PHONE", "ADDRESS", "IDENTITY", "TAX", "BANK",
];

#[derive(Clone, Deserialize, Serialize)]
pub struct Artifact {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub url: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Model {
    pub id: String,
    pub engine: String,
    pub repository: String,
    pub revision: String,
    pub files: Vec<Artifact>,
}

pub fn model(id: &str) -> Result<Model> {
    let models: Vec<Model> = serde_json::from_str(include_str!("../models.json"))?;
    models
        .into_iter()
        .find(|m| m.id == id)
        .with_context(|| format!("unknown candidate {id}"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex(&hash.finalize()))
}

pub fn verify(path: &Path, artifact: &Artifact) -> Result<()> {
    ensure!(
        fs::metadata(path)?.len() == artifact.bytes,
        "size mismatch: {}",
        path.display()
    );
    ensure!(
        sha256_file(path)? == artifact.sha256,
        "SHA-256 mismatch: {}",
        path.display()
    );
    Ok(())
}

/// Only explicit setup uses HTTP. Inference never calls this function.
pub fn setup(model: &Model, cache: &Path) -> Result<()> {
    for artifact in &model.files {
        let path = cache.join(&model.id).join(&artifact.path);
        if verify(&path, artifact).is_ok() {
            continue;
        }
        let parent = path.parent().context("artifact has no parent")?;
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        let mut response = ureq::get(&artifact.url).call()?;
        // Bound the transfer even if a server sends an unexpectedly large body.
        std::io::copy(
            &mut response.body_mut().as_reader().take(artifact.bytes + 1),
            &mut temporary,
        )?;
        verify(temporary.path(), artifact)?;
        temporary.as_file().sync_all()?;
        temporary.persist(&path)?;
        eprintln!("Verified {}", path.display());
    }
    verify_model(model, cache)?;
    Ok(())
}

pub fn verify_model(model: &Model, cache: &Path) -> Result<PathBuf> {
    let root = cache.join(&model.id);
    for artifact in &model.files {
        verify(&root.join(&artifact.path), artifact)?;
    }
    Ok(root)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Mention {
    pub category: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Clone, Deserialize)]
pub struct Fixture {
    pub id: String,
    pub language: String,
    pub kind: String,
    pub annotated: String,
    #[serde(default = "one")]
    pub repeat: usize,
}
fn one() -> usize {
    1
}

/// Annotations describe the original source; offsets are half-open UTF-8 bytes.
pub fn expand(annotated: &str, repeat: usize) -> Result<(String, Vec<Mention>)> {
    ensure!((1..=1024).contains(&repeat), "invalid fixture repeat count");
    let mut source = String::new();
    let mut gold = Vec::new();
    for _ in 0..repeat {
        let mut rest = annotated;
        while let Some((prefix, tail)) = rest.split_once('⟦') {
            ensure!(!prefix.contains('⟧'), "unmatched annotation closing marker");
            source.push_str(prefix);
            let (body, suffix) = tail.split_once('⟧').context("unclosed annotation")?;
            let (category, text) = body
                .split_once('|')
                .context("missing annotation category")?;
            ensure!(
                CATEGORIES.contains(&category) && !text.is_empty() && !text.contains('⟦'),
                "invalid annotation {body}"
            );
            let start = source.len();
            source.push_str(text);
            gold.push(Mention {
                category: category.into(),
                start,
                end: source.len(),
                text: text.into(),
            });
            rest = suffix;
        }
        ensure!(!rest.contains('⟧'), "unmatched annotation closing marker");
        source.push_str(rest);
    }
    Ok((source, gold))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Prediction {
    pub category: String,
    pub label: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub score: f32,
}

impl Prediction {
    pub fn valid_for(&self, source: &str) -> bool {
        self.start < self.end
            && CATEGORIES.contains(&self.category.as_str())
            && self.score.is_finite()
            && (0.0..=1.0).contains(&self.score)
            && source.get(self.start..self.end) == Some(self.text.as_str())
    }
}

#[derive(Default, Debug, Deserialize, Serialize)]
pub struct Counts {
    pub expected: usize,
    pub true_positives: usize,
    pub false_positives: usize,
    pub misses: usize,
}

#[derive(Deserialize, Serialize)]
pub struct Assessment {
    pub counts: BTreeMap<String, Counts>,
    pub missed: Vec<Mention>,
    pub false_positives: Vec<Prediction>,
    pub invalid_offsets: Vec<Prediction>,
    pub duplicate_predictions: usize,
}

/// Exact boundary AND category match. Partial names/addresses count as misses
/// and false positives; overlapping predictions cannot earn duplicate credit.
pub fn assess(source: &str, gold: &[Mention], predictions: &[Prediction]) -> Assessment {
    let mut result = Assessment {
        counts: CATEGORIES
            .iter()
            .map(|s| (s.to_string(), Counts::default()))
            .collect(),
        missed: vec![],
        false_positives: vec![],
        invalid_offsets: vec![],
        duplicate_predictions: 0,
    };
    let mut valid: Vec<&Prediction> = Vec::new();
    for prediction in predictions {
        if !prediction.valid_for(source) {
            result.invalid_offsets.push(prediction.clone());
            continue;
        }
        if valid.iter().any(|p| {
            p.category == prediction.category
                && p.start == prediction.start
                && p.end == prediction.end
        }) {
            result.duplicate_predictions += 1;
        } else {
            valid.push(prediction);
        }
    }
    let mut matched = vec![false; gold.len()];
    for prediction in valid {
        let index = gold.iter().enumerate().position(|(i, g)| {
            !matched[i]
                && g.category == prediction.category
                && g.start == prediction.start
                && g.end == prediction.end
        });
        let count = result
            .counts
            .get_mut(&prediction.category)
            .expect("validated category");
        if let Some(i) = index {
            matched[i] = true;
            count.true_positives += 1;
        } else {
            count.false_positives += 1;
            result.false_positives.push(prediction.clone());
        }
    }
    for (i, mention) in gold.iter().enumerate() {
        let count = result
            .counts
            .get_mut(&mention.category)
            .expect("validated gold category");
        count.expected += 1;
        if !matched[i] {
            count.misses += 1;
            result.missed.push(mention.clone());
        }
    }
    result
}

pub fn summarize(paths: &[PathBuf]) -> Result<String> {
    use std::fmt::Write;
    let mut markdown = String::from(
        "# Pseudonymization measurements\n\nGenerated from raw reports. Exact source-byte span and category scoring; long and stress fixtures are separate from the short-text baseline.\n",
    );
    for path in paths {
        let report: Report = serde_json::from_slice(&fs::read(path)?)?;
        let process: serde_json::Value =
            serde_json::from_slice(&fs::read(path.with_extension("process.json"))?)?;
        writeln!(
            markdown,
            "\n## {} — threshold {}\n",
            report.model.id, report.threshold
        )?;
        writeln!(
            markdown,
            "Process success: {}; profile: {}; platform: {}; machine: {}.\n",
            process["success"], report.profile, report.platform, process["machine"]
        )?;
        writeln!(
            markdown,
            "Verify {:.1} ms; load {:.1} ms; drop {:.1} ms; peak RSS {} MiB; pinned setup {} bytes.\n",
            report.verify_ms,
            report.load_ms,
            report.drop_ms,
            report
                .peak_rss_bytes
                .map(|b| format!("{:.1}", b as f64 / 1_048_576.0))
                .unwrap_or("unavailable".into()),
            report.model.files.iter().map(|f| f.bytes).sum::<u64>()
        )?;
        let mut groups: BTreeMap<(String, String), Counts> = BTreeMap::new();
        for fixture in &report.fixtures {
            if fixture.kind == "long" || fixture.kind == "stress" {
                continue;
            }
            for (category, count) in &fixture.assessment.counts {
                for key in [
                    (fixture.language.clone(), category.clone()),
                    (fixture.language.clone(), "ALL".into()),
                ] {
                    let sum = groups.entry(key).or_default();
                    sum.expected += count.expected;
                    sum.true_positives += count.true_positives;
                    sum.false_positives += count.false_positives;
                    sum.misses += count.misses;
                }
            }
        }
        writeln!(
            markdown,
            "| Language | Category | Gold | TP | Misses | FP | Precision | Recall |\n| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |"
        )?;
        for ((language, category), count) in groups {
            let percent = |denominator: usize| {
                if denominator == 0 {
                    "—".into()
                } else {
                    format!(
                        "{:.1}%",
                        count.true_positives as f64 / denominator as f64 * 100.0
                    )
                }
            };
            writeln!(
                markdown,
                "| {language} | {category} | {} | {} | {} | {} | {} | {} |",
                count.expected,
                count.true_positives,
                count.misses,
                count.false_positives,
                percent(count.true_positives + count.false_positives),
                percent(count.expected)
            )?;
        }
        writeln!(
            markdown,
            "\n| Fixture | Bytes | First ms | Warm median ms | TP | Misses | FP | Invalid offsets | Deterministic | Error |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |"
        )?;
        for fixture in report.fixtures {
            let sum = |field: fn(&Counts) -> usize| {
                fixture.assessment.counts.values().map(field).sum::<usize>()
            };
            let mut warm = fixture
                .latency_ms
                .iter()
                .skip(1)
                .copied()
                .collect::<Vec<_>>();
            warm.sort_by(f64::total_cmp);
            let median = if warm.is_empty() {
                "—".into()
            } else {
                format!(
                    "{:.1}",
                    (warm[(warm.len() - 1) / 2] + warm[warm.len() / 2]) / 2.0
                )
            };
            writeln!(
                markdown,
                "| {} | {} | {:.1} | {} | {} | {} | {} | {} | {} | {} |",
                fixture.id,
                fixture.source_bytes,
                fixture.latency_ms[0],
                median,
                sum(|c| c.true_positives),
                sum(|c| c.misses),
                sum(|c| c.false_positives),
                fixture.assessment.invalid_offsets.len(),
                fixture.repeat_deterministic,
                fixture.error.unwrap_or_default().replace(['\n', '|'], " ")
            )?;
        }
    }
    Ok(markdown)
}

/// Conservative evaluation windows in regex words, with overlap. This bounds
/// the GLiNER-v1 word tensor; it is NOT proof of the tokenizer's subword budget.
pub fn windows(source: &str, width: usize, overlap: usize) -> Result<Vec<(usize, usize)>> {
    ensure!(width > overlap && width > 0, "invalid window geometry");
    let regex = regex::Regex::new(r"\w+(?:[-_]\w+)*|\S")?;
    let tokens: Vec<_> = regex.find_iter(source).collect();
    if tokens.len() <= width {
        return Ok(vec![(0, source.len())]);
    }
    let mut output = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let end = (i + width).min(tokens.len());
        output.push((tokens[i].start(), tokens[end - 1].end()));
        if end == tokens.len() {
            break;
        }
        i += width - overlap;
    }
    Ok(output)
}

#[derive(Deserialize, Serialize)]
pub struct FixtureReport {
    pub id: String,
    pub language: String,
    pub kind: String,
    pub source_bytes: usize,
    pub source_sha256: String,
    pub latency_ms: Vec<f64>,
    pub predictions: Vec<Prediction>,
    pub assessment: Assessment,
    pub error: Option<String>,
    pub repeat_deterministic: bool,
}

#[derive(Deserialize, Serialize)]
pub struct Report {
    pub model: Model,
    pub engine_revision: String,
    pub profile: String,
    pub platform: String,
    pub runtime_sha256: String,
    pub corpus_sha256: String,
    pub threshold: f32,
    pub threads: usize,
    pub window_words: usize,
    pub overlap_words: usize,
    pub configuration: serde_json::Value,
    pub verify_ms: f64,
    pub load_ms: f64,
    pub drop_ms: f64,
    pub peak_rss_bytes: Option<u64>,
    pub fixtures: Vec<FixtureReport>,
}

pub struct WorkerArgs {
    pub model: Model,
    pub root: PathBuf,
    pub corpus: PathBuf,
    pub output: PathBuf,
    pub runtime_sha256: String,
    pub threshold: f32,
    pub verify_ms: f64,
}

impl WorkerArgs {
    pub fn read(engine: &str) -> Result<Self> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        ensure!(
            args.len() == 6,
            "worker: CANDIDATE CACHE CORPUS OUTPUT RUNTIME THRESHOLD"
        );
        let started = Instant::now();
        let model = model(args[0].to_str().context("candidate must be UTF-8")?)?;
        ensure!(
            model.engine == engine,
            "candidate belongs to {}",
            model.engine
        );
        let root = verify_model(&model, Path::new(&args[1]))?;
        let runtime_sha256 = sha256_file(Path::new(&args[4]))?;
        let threshold: f32 = args[5]
            .to_str()
            .context("threshold must be UTF-8")?
            .parse()?;
        ensure!(
            threshold.is_finite() && (0.0..=1.0).contains(&threshold),
            "invalid threshold"
        );
        // The process supervisor selects the library in its child environment.
        ensure!(
            std::env::var_os("ORT_DYLIB_PATH").as_ref() == Some(&args[4]),
            "runtime path differs from ORT_DYLIB_PATH"
        );
        Ok(Self {
            model,
            root,
            corpus: (&args[2]).into(),
            output: (&args[3]).into(),
            runtime_sha256,
            threshold,
            verify_ms: started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    pub fn evaluate<M, F>(
        self,
        model: M,
        engine_revision: &str,
        load_ms: f64,
        configuration: serde_json::Value,
        mut infer: F,
    ) -> Result<()>
    where
        F: FnMut(&M, &str) -> Result<Vec<Prediction>>,
    {
        let corpus_bytes = fs::read(&self.corpus)?;
        let corpus_sha256 = hex(&Sha256::digest(&corpus_bytes));
        let fixtures: Vec<Fixture> = serde_json::from_slice(&corpus_bytes)?;
        let mut reports = Vec::new();
        let mut ids = std::collections::BTreeSet::new();
        for fixture in fixtures {
            ensure!(ids.insert(fixture.id.clone()), "duplicate fixture id");
            ensure!(
                ["en", "ru", "mixed"].contains(&fixture.language.as_str()),
                "invalid language"
            );
            let (source, gold) = expand(&fixture.annotated, fixture.repeat)?;
            let hash = hex(&Sha256::digest(source.as_bytes()));
            let mut times = Vec::new();
            let mut predictions = Vec::new();
            let mut error = None;
            let mut deterministic = true;
            // First call plus two warm calls; long documents once, to bound run time.
            for iteration in 0..if fixture.kind == "long" { 1 } else { 3 } {
                let started = Instant::now();
                match infer(&model, &source) {
                    Ok(mut output) => {
                        output.sort_by(|a, b| {
                            (&a.category, a.start, a.end).cmp(&(&b.category, b.start, b.end))
                        });
                        if iteration == 0 {
                            predictions = output;
                        } else if serde_json::to_vec(&output)? != serde_json::to_vec(&predictions)?
                        {
                            deterministic = false;
                        }
                    }
                    Err(e) => {
                        error = Some(format!("{e:#}"));
                    }
                }
                times.push(started.elapsed().as_secs_f64() * 1000.0);
                if error.is_some() {
                    break;
                }
            }
            ensure!(
                hash == hex(&Sha256::digest(source.as_bytes())),
                "source changed during detection"
            );
            let assessment = assess(&source, &gold, &predictions);
            eprintln!(
                "{}: {:.0} ms, {} candidates{}",
                fixture.id,
                times[0],
                predictions.len(),
                if error.is_some() { " (failed)" } else { "" }
            );
            reports.push(FixtureReport {
                id: fixture.id,
                language: fixture.language,
                kind: fixture.kind,
                source_bytes: source.len(),
                source_sha256: hash,
                latency_ms: times,
                predictions,
                assessment,
                error,
                repeat_deterministic: deterministic,
            });
        }
        let peak_rss_bytes = peak_rss_bytes();
        let dropping = Instant::now();
        drop(model);
        let report = Report {
            model: self.model,
            engine_revision: engine_revision.into(),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .into(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            runtime_sha256: self.runtime_sha256,
            corpus_sha256: corpus_sha256.clone(),
            threshold: self.threshold,
            threads: 4,
            window_words: 128,
            overlap_words: 32,
            configuration,
            verify_ms: self.verify_ms,
            load_ms,
            drop_ms: dropping.elapsed().as_secs_f64() * 1000.0,
            peak_rss_bytes,
            fixtures: reports,
        };
        ensure!(
            sha256_file(&self.corpus)? == corpus_sha256,
            "corpus changed during inference"
        );
        fs::write(&self.output, serde_json::to_vec_pretty(&report)?)?;
        if report.fixtures.iter().any(|f| {
            f.error.is_some() || !f.assessment.invalid_offsets.is_empty() || !f.repeat_deterministic
        }) {
            bail!("qualification encountered inference/offset/determinism failures; see report");
        }
        Ok(())
    }
}

#[cfg(unix)]
pub fn peak_rss_bytes() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes the provided rusage on success.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    let usage = unsafe { usage.assume_init() };
    let bytes = u64::try_from(usage.ru_maxrss).ok()?;
    Some(if cfg!(target_os = "macos") {
        bytes
    } else {
        bytes * 1024
    })
}

#[cfg(windows)]
pub fn peak_rss_bytes() -> Option<u64> {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    let mut counters = std::mem::MaybeUninit::<PROCESS_MEMORY_COUNTERS>::zeroed();
    // SAFETY: current-process handle and correctly sized writable counter buffer.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            counters.as_mut_ptr(),
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    };
    if ok == 0 {
        None
    } else {
        Some(unsafe { counters.assume_init() }.PeakWorkingSetSize as u64)
    }
}

#[cfg(not(any(unix, windows)))]
pub fn peak_rss_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prediction(category: &str, start: usize, end: usize, text: &str) -> Prediction {
        Prediction {
            category: category.into(),
            label: category.into(),
            start,
            end,
            text: text.into(),
            score: 0.9,
        }
    }
    #[test]
    fn unicode_repeats_and_exact_scoring() {
        let (source, gold) = expand("🙂 ⟦PERSON|Анна Ёлкина⟧ / ⟦PERSON|Анна Ёлкина⟧\n", 2).unwrap();
        assert_eq!(gold.len(), 4);
        assert_eq!(gold[0].start, 5);
        for m in &gold {
            assert_eq!(&source[m.start..m.end], m.text);
        }
        let p = prediction("PERSON", gold[0].start, gold[0].end, &gold[0].text);
        let mut invalid = p.clone();
        invalid.start += 1;
        let result = assess(&source, &gold, &[p.clone(), p, invalid]);
        assert_eq!(result.counts["PERSON"].true_positives, 1);
        assert_eq!(result.counts["PERSON"].misses, 3);
        assert_eq!(result.invalid_offsets.len(), 1);
        assert_eq!(result.duplicate_predictions, 1);
    }
    #[test]
    fn wrong_category_and_partial_span_are_not_credit() {
        let (source, gold) = expand("⟦PERSON|Alex Morgan⟧", 1).unwrap();
        let result = assess(
            &source,
            &gold,
            &[
                prediction("ORG", 0, 11, "Alex Morgan"),
                prediction("PERSON", 0, 4, "Alex"),
            ],
        );
        assert_eq!(result.counts["PERSON"].misses, 1);
        assert_eq!(result.counts["PERSON"].false_positives, 1);
        assert_eq!(result.counts["ORG"].false_positives, 1);
    }
    #[test]
    fn malformed_gold_is_rejected() {
        for text in ["⟦PERSON|Alex", "⟦DATE|2026⟧", "⟦PERSON|⟧", "Alex⟧"] {
            assert!(expand(text, 1).is_err());
        }
        assert!(expand("", 0).is_err());
    }
    #[test]
    fn windows_cover_unicode_tail_and_boundaries() {
        let source = "🙂 начало Анна конец хвост";
        let chunks = windows(source, 3, 1).unwrap();
        assert_eq!(chunks.first().unwrap().0, 0);
        assert_eq!(chunks.last().unwrap().1, source.len());
        for (start, end) in chunks {
            assert!(source.get(start..end).is_some());
        }
        assert!(windows(source, 2, 2).is_err());
    }
    #[test]
    fn integrity_rejects_partial_and_same_size_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("model");
        let artifact = Artifact {
            path: "model".into(),
            bytes: 3,
            sha256: hex(&Sha256::digest(b"abc")),
            url: String::new(),
        };
        assert!(verify(&path, &artifact).is_err());
        fs::write(&path, "abc").unwrap();
        assert!(verify(&path, &artifact).is_ok());
        fs::write(&path, "abd").unwrap();
        assert!(verify(&path, &artifact).is_err());
        fs::write(&path, "ab").unwrap();
        assert!(verify(&path, &artifact).is_err());
    }
    #[test]
    fn committed_corpus_and_pins_are_well_formed() {
        let fixtures: Vec<Fixture> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/pseudonymization/corpus.json"
        ))
        .unwrap();
        assert!(fixtures.len() >= 12);
        for f in fixtures {
            expand(&f.annotated, f.repeat).unwrap();
        }
        for id in ["multi-v2.1-q8", "pii-base-q8", "gliner2-pii-fp16"] {
            let m = model(id).unwrap();
            assert_eq!(m.revision.len(), 40);
            for a in m.files {
                assert_eq!(a.sha256.len(), 64);
                assert!(a.bytes > 0);
                assert!(a.url.contains(&m.revision));
                assert!(
                    Path::new(&a.path)
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                );
            }
        }
    }
}
