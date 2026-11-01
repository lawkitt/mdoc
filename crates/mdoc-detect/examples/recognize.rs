//! `just recognize`: the recognition fixtures' tool (ADR 0093). Not shipped.
//!
//! ```text
//! just recognize <file.md|folder> [--gold] [--snapshot]
//! just recognize --scoreboard [<folder>] [--snapshot]
//! just recognize --refresh-snapshots [<file.md|folder>]
//! just recognize --check [<folder>]
//! ```
//!
//! A file prints its proposals marked as gold is (`⟦CATEGORY|text⟧`);
//! `--gold` prints each fixture's misses, false hits and boundary errors
//! and fails when one is not as its status expects; a document without a
//! status (the holdout) is expected to pass, and without a snapshot the
//! model runs. The model is the
//! installed pinned FP16 from the app data (read only, never downloaded;
//! `MDOC_PII_MODEL_ROOT` and `MDOC_ONNX_RUNTIME` point elsewhere);
//! `--snapshot` reads each fixture's committed `<id>.model.json` instead.
//! `--scoreboard` prints recall and precision per language × category ×
//! kind, and exits 0 whatever the documents did. `--refresh-snapshots` rewrites snapshots (on the Mac, the
//! reference platform). `--check` is the detection gate: it reruns the model
//! over every fixture, fails on a stale snapshot or an unexpected result,
//! and on a model run over its 5-minute budget. Folders default to
//! `tests/fixtures/recognition`; any folder of fixtures works (the holdout).
use mdoc_detect::{
    ModelSession, RawEntity,
    recognition::{
        Comparison, Fixture, Scoreboard, Snapshot, Status, annotate, fixture_paths, proposals,
        ratchet,
    },
};
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

const BUDGET: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Annotate,
    Gold,
    Scoreboard,
    Refresh,
    Check,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn usage() -> String {
    "usage: just recognize <file.md|folder> [--gold] [--snapshot]\n       \
     just recognize --scoreboard [<folder>] [--snapshot]\n       \
     just recognize --refresh-snapshots [<file.md|folder>]\n       \
     just recognize --check [<folder>]"
        .into()
}

fn run() -> Result<bool, String> {
    let mut mode = Mode::Annotate;
    let mut gold = false;
    let mut from_snapshot = false;
    let mut target = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--gold" => gold = true,
            "--snapshot" => from_snapshot = true,
            "--scoreboard" => mode = Mode::Scoreboard,
            "--refresh-snapshots" => mode = Mode::Refresh,
            "--check" => mode = Mode::Check,
            "-h" | "--help" => {
                println!("{}", usage());
                return Ok(true);
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag {flag}\n{}", usage()));
            }
            path if target.is_none() => target = Some(PathBuf::from(path)),
            _ => return Err(usage()),
        }
    }
    if mode == Mode::Annotate && gold {
        mode = Mode::Gold;
    }
    if from_snapshot && matches!(mode, Mode::Refresh | Mode::Check) {
        return Err(
            "--snapshot reads snapshots; --refresh-snapshots and --check run the model".into(),
        );
    }
    let root = match (target, mode) {
        (Some(path), _) => path,
        (None, Mode::Annotate | Mode::Gold) => return Err(usage()),
        (None, _) => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/recognition")
            .canonicalize()
            .map_err(|e| format!("tests/fixtures/recognition: {e}"))?,
    };
    let paths = fixture_paths(&root)?;
    if paths.is_empty() {
        return Err(format!("no fixtures under {}", root.display()));
    }
    let mut model = if from_snapshot {
        None
    } else {
        Some(Model::load()?)
    };
    match mode {
        Mode::Annotate => {
            let mut ok = true;
            for path in &paths {
                let fixture = Fixture::read(path)?;
                let raw = entities(&fixture, model.as_mut())?;
                match proposals(&fixture.source, &raw) {
                    Ok(spans) => {
                        if paths.len() > 1 {
                            println!("== {}", path.display());
                        }
                        print!("{}", annotate(&fixture.source, &spans));
                        if !fixture.source.ends_with('\n') {
                            println!();
                        }
                    }
                    Err(error) => {
                        eprintln!("{}: {error}", path.display());
                        ok = false;
                    }
                }
            }
            Ok(ok)
        }
        Mode::Gold | Mode::Scoreboard => {
            let mut scoreboard = Scoreboard::default();
            let mut ok = true;
            for path in &paths {
                let fixture = Fixture::read(path)?;
                let raw = entities(&fixture, model.as_mut())?;
                let proposed = proposals(&fixture.source, &raw)?;
                let comparison = Comparison::new(&fixture.gold, &proposed);
                scoreboard.add(&fixture, &comparison, &proposed);
                let unexpected = ratchet(fixture.expected(), comparison.passes());
                ok &= unexpected.is_none();
                if mode == Mode::Gold {
                    let status = fixture.status.map_or("no status: pass", Status::name);
                    let result = if comparison.passes() {
                        "passes"
                    } else {
                        "fails"
                    };
                    let mark = if unexpected.is_some() {
                        "UNEXPECTED"
                    } else {
                        "ok"
                    };
                    println!("{mark} {} ({status}): {result}", path.display());
                    print!("{}", comparison.report(&fixture.source));
                    if let Some(why) = unexpected {
                        println!("  {why}");
                    }
                }
            }
            if mode == Mode::Scoreboard {
                // A report: aggregates only, whatever each document did.
                print!("{}", scoreboard.render());
                return Ok(true);
            }
            Ok(ok)
        }
        Mode::Refresh => {
            let model = model.as_mut().expect("model mode");
            let started = Instant::now();
            for path in &paths {
                let fixture = Fixture::read(path)?;
                let snapshot = model.snapshot(&fixture.source)?;
                std::fs::write(fixture.snapshot_path(), snapshot.to_json())
                    .map_err(|e| format!("{}: {e}", fixture.snapshot_path().display()))?;
                println!(
                    "{}: {} entities",
                    fixture.snapshot_path().display(),
                    snapshot.entities.len()
                );
            }
            println!(
                "{} snapshots in {:.1}s",
                paths.len(),
                started.elapsed().as_secs_f64()
            );
            Ok(true)
        }
        Mode::Check => {
            let model = model.as_mut().expect("model mode");
            let started = Instant::now();
            let mut failures = Vec::new();
            for path in &paths {
                let fixture = Fixture::read(path)?;
                let mut problems = Vec::new();
                if let Err(error) = fixture.validate().and_then(|()| fixture.validate_layout()) {
                    problems.push(error);
                }
                let fresh = model.snapshot(&fixture.source)?;
                match Snapshot::read(&fixture.snapshot_path()) {
                    Ok(saved) => {
                        if let Some(why) = saved.stale(&fresh) {
                            problems.push(format!(
                                "stale snapshot ({why}): run just recognize --refresh-snapshots {} on the Mac",
                                path.display()
                            ));
                        }
                    }
                    Err(error) => problems.push(format!("no snapshot: {error}")),
                }
                let proposed = proposals(&fixture.source, &fresh.entities)?;
                let comparison = Comparison::new(&fixture.gold, &proposed);
                if let Some(why) = ratchet(fixture.expected(), comparison.passes()) {
                    problems.push(format!("{why}\n{}", comparison.report(&fixture.source)));
                }
                if !problems.is_empty() {
                    failures.push(format!("{}: {}", path.display(), problems.join("; ")));
                }
            }
            let elapsed = started.elapsed();
            println!(
                "detection gate: {} fixtures, model run {:.1}s (budget {}s)",
                paths.len(),
                elapsed.as_secs_f64(),
                BUDGET.as_secs()
            );
            if elapsed > BUDGET {
                failures.push(format!(
                    "the model run took {:.0}s, over its {}s budget: split the fixtures into a per-PR set and a nightly full set (ADR 0093)",
                    elapsed.as_secs_f64(),
                    BUDGET.as_secs()
                ));
            }
            for failure in &failures {
                println!("FAIL {failure}");
            }
            Ok(failures.is_empty())
        }
    }
}

/// The fixture's raw entities: from the model, or its snapshot.
fn entities(fixture: &Fixture, model: Option<&mut Model>) -> Result<Vec<RawEntity>, String> {
    match model {
        Some(model) => Ok(model.snapshot(&fixture.source)?.entities),
        None => {
            let snapshot = Snapshot::read(&fixture.snapshot_path())?;
            snapshot
                .validate(&fixture.source)
                .map_err(|e| format!("{}: {e}", fixture.path.display()))?;
            Ok(snapshot.entities)
        }
    }
}

struct Model {
    session: ModelSession,
    config: mdoc_store::settings::PiiConfig,
}

impl Model {
    /// The pinned FP16 at the default threshold, as installed by the app.
    fn load() -> Result<Self, String> {
        let config = mdoc_store::settings::PiiConfig {
            model: mdoc_store::settings::PiiModel::Fp16,
            ..Default::default()
        };
        let root = match std::env::var_os("MDOC_PII_MODEL_ROOT") {
            Some(root) => PathBuf::from(root),
            None => mdoc_detect::root_for(config.model).map_err(|e| e.to_string())?,
        };
        let runtime = match std::env::var_os("MDOC_ONNX_RUNTIME") {
            Some(runtime) => PathBuf::from(runtime),
            None => mdoc_import::ocr::onnx_runtime(false).map_err(|e| {
                format!("no verified ONNX Runtime in the app data ({e}); set up the model in mdoc or set MDOC_ONNX_RUNTIME")
            })?,
        };
        let session = ModelSession::load(&root, &runtime, &config).map_err(|e| {
            format!(
                "{e} (model {}: set up {} in mdoc, or set MDOC_PII_MODEL_ROOT; --snapshot reads committed snapshots instead)",
                root.display(),
                mdoc_detect::model_name(config.model)
            )
        })?;
        Ok(Self { session, config })
    }

    fn snapshot(&mut self, source: &str) -> Result<Snapshot, String> {
        Ok(Snapshot {
            model: mdoc_detect::model_name(self.config.model),
            threshold: self.config.threshold,
            entities: self.session.entities(source).map_err(|e| e.to_string())?,
        })
    }
}
